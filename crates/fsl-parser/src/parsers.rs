//! FSL の構文解析器
//!
//! 入力は `fsl-lexer` のトリビア除去済みトークン列．

use chumsky::input::{Input, ValueInput};
use chumsky::prelude::*;
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

// Block と Expr は互いを要求する
// `Recursive::declare()` の未定義状態相互参照を作り
// 各サブパーサに `.clone()` で配ってから `define()` で実体を結ぶ
//
// `Indirect<'src, 'b, ...>` の `'b` は内部パーサの寿命  所有パーサ
// しか流さないため `'tok = 'b` で問題ない

/// `Block` 用の相互再帰ハンドル
pub(crate) type RecBlock<'tok, I> =
    Recursive<Indirect<'tok, 'tok, I, Block, extra::Err<Rich<'tok, Token>>>>;

/// `Expr` 用の相互再帰ハンドル
pub(crate) type RecExpr<'tok, I> =
    Recursive<Indirect<'tok, 'tok, I, Expr, extra::Err<Rich<'tok, Token>>>>;

/// Block と Expr の相互再帰パーサを宣言・定義して返す
///
/// 戻り値の `RecBlock` / `RecExpr` は `Clone` 可能で，
/// そのまま `Parser` として使える
pub(crate) fn block_and_expr<'tok, I>() -> (RecBlock<'tok, I>, RecExpr<'tok, I>)
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // 実体未定義のハンドルを作る
    let mut block: RecBlock<'tok, I> = Recursive::declare();
    let mut expr: RecExpr<'tok, I> = Recursive::declare();

    //未定義ハンドルを配って実体パーサを構築する
    // `block.clone()` / `expr.clone()` は内部の `Rc` を bump するだけ
    let block_parser = block::block_def(block.clone(), expr.clone());
    let expr_parser = expr::expr_def(block.clone(), expr.clone());

    // 確定
    block.define(block_parser);
    expr.define(expr_parser);

    (block, expr)
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
    item_def()
        .repeated()
        .collect()
        .map(|items| CompilationUnit { items })
        .spanned()
}
