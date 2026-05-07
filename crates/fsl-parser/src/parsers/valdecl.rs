//! 変数的概念の宣言をパースするモジュール

use chumsky::{
    IterParser, Parser,
    error::Rich,
    extra,
    input::ValueInput,
    primitive::{choice, just},
    span::SimpleSpan,
};
use fsl_lexer::Token;

use crate::{
    Ident, MemDecl, RegDecl, ValDecl, ValLhs,
    parsers::{RecExpr, atom::ident_def, typedef::type_def},
};

/// レジスタ宣言のパーサー
///
/// 初期化式 (`= expr`) を持つため `expr` の再帰ハンドルを取る
pub(super) fn reg_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, RegDecl, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::Reg)
        // レジスタ名
        .ignore_then(ident_def())
        // 型
        .then(just(Token::Colon).ignore_then(type_def()))
        // 初期化
        .then(just(Token::Eq).ignore_then(expr.clone()).or_not())
        .map(|((name, ty), init)| RegDecl { name, ty, init })
}

/// メモリ宣言のパーサー: `mem [T] name(size) [= (e, e, ...)]`
pub(super) fn mem_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, MemDecl, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::Mem)
        // 要素型 [T]
        .ignore_then(type_def().delimited_by(just(Token::LBracket), just(Token::RBracket)))
        // 名前
        .then(ident_def())
        // サイズ式 (n)
        .then(
            expr.clone()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        // 初期化リスト = (e, e, ...)
        .then(
            just(Token::Eq)
                .ignore_then(
                    expr.clone()
                        .separated_by(just(Token::Comma))
                        .allow_trailing()
                        .collect()
                        .delimited_by(just(Token::LParen), just(Token::RParen)),
                )
                .or_not(),
        )
        .map(|(((elem_ty, name), size), init_opt)| MemDecl {
            name,
            elem_ty,
            size,
            init: init_opt.unwrap_or_default(),
        })
}

/// `val pat[: T] = expr`
///
/// `pat` は単独の識別子 `x` か `(x, y, ...)` のタプル
/// インスタンス宣言は持たない  field 側で別途処理する
pub(super) fn val_decl_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, ValDecl, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    let lhs = choice((
        // `(x, y, ...)`  タプル分解  曖昧さ解消のため先に試す
        val_tuple_def().map(ValLhs::Tuple),
        // `x`  単独識別子
        ident_def().map(ValLhs::Single),
    ));

    just(Token::Val)
        .ignore_then(lhs)
        .then(just(Token::Colon).ignore_then(type_def()).or_not())
        .then_ignore(just(Token::Eq))
        .then(expr.clone())
        .map(|((pattern, ty), init)| ValDecl { pattern, ty, init })
}

/// タプル宣言 val (a, b, c) = (1, 2, 3) 式
pub(super) fn val_tuple_def<'tok, I>()
-> impl Parser<'tok, I, Vec<Ident>, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    ident_def()
        // ident, ident, ..., ident
        .separated_by(just(Token::Comma))
        .collect()
        // (...)
        .delimited_by(just(Token::LParen), just(Token::RParen))
}
