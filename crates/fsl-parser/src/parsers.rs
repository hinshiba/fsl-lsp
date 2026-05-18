//! FSL の構文解析器
//!
//! 入力は `fsl-lexer` のトリビア除去済みトークン列．

use chumsky::input::{Input, ValueInput};
use chumsky::prelude::*;
use chumsky::recovery::skip_then_retry_until;
use chumsky::recursive::{Indirect, Recursive};

use crate::ast::*;
use crate::parsers::item::item_def;
use fsl_lexer::{Span, SpannedToken, Token};

mod atom;
mod block;
mod expr;
mod field;
mod item;
mod pattern;
mod typedef;
mod valdecl;

/// 構文エラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

/// 構文解析結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseResult {
    pub unit: CompilationUnit,
    pub errors: Vec<ParseError>,
}

fn token_proj(t: &(Token, SimpleSpan)) -> (&Token, &SimpleSpan) {
    (&t.0, &t.1)
}

/// `Expr` 用の再帰ハンドル
pub(crate) type RecExpr<'tok, I> =
    Recursive<Indirect<'tok, 'tok, I, Expr, extra::Err<Rich<'tok, Token>>>>;

/// 閉じ波括弧 `}` のパーサ  欠落時は何も消費せず復旧する
///
/// 編集途中のソースは閉じ括弧をまだ持たないことが多い．`}` を欠く場合に
/// 解析を打ち切ると本体ブロックがまるごと失われ，スコープ内シンボルの
/// 補完が働かなくなる．欠落時は空解析で復旧して本体を解析結果に残す．
pub(crate) fn rbrace<'tok, I>() -> impl Parser<'tok, I, Token, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::RBrace).recover_with(via_parser(empty().to(Token::RBrace)))
}

/// `expr` の再帰ハンドルを宣言・定義して返す
///
/// 戻り値の `RecExpr` は `Clone` 可能で，そのまま `Parser` として使える
pub(crate) fn expr_parser<'tok, I>() -> RecExpr<'tok, I>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // 実体未定義のハンドルを作り，それを配って実体を構築する
    let mut expr: RecExpr<'tok, I> = Recursive::declare();
    let parser = expr::expr_def(expr.clone());
    expr.define(parser);
    expr
}

pub fn parse_token(tokens: Vec<SpannedToken>, src_end: usize) -> ParseResult {
    // fsl_lexer は `Span = Range<usize>` で位置を持つが
    // chumsky 側は `SimpleSpan` を期待するため事前に変換しておく
    let toks: Vec<(Token, SimpleSpan)> = tokens
        .into_iter()
        .map(|st| (st.tok, SimpleSpan::from(st.span)))
        .collect();
    let eoi: SimpleSpan = SimpleSpan::from(src_end..src_end);
    let proj: fn(&(Token, SimpleSpan)) -> (&Token, &SimpleSpan) = token_proj;
    let stream = toks.as_slice().map(eoi, proj);
    let (unit_opt, errs) = parser().parse(stream).into_output_errors();
    let unit = unit_opt.map(|s| s.inner).unwrap_or_default();
    let errors = errs
        .into_iter()
        .map(|e| {
            let s = e.span();
            ParseError {
                message: format!("{:?}", e),
                span: s.start..s.end,
            }
        })
        .collect();
    ParseResult { unit, errors }
}

// ============================================================
// メインパーサ
// ============================================================

fn parser<'tok, I>() -> impl Parser<'tok, I, Spanned<CompilationUnit>, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // `expr` の再帰結び目をここで一度だけ作り `item_def` 経由で配る
    //
    // アイテム解析が失敗した場合は次のアイテムが解析できる位置まで
    // トークンを読み飛ばして再試行する  破損したアイテムの後ろにある
    // 正常なモジュール／トレイトを取りこぼさないための復旧措置
    item_def(expr_parser())
        .recover_with(skip_then_retry_until(any().ignored(), end()))
        .repeated()
        .collect()
        .map(|items| CompilationUnit { items })
        .spanned()
}
