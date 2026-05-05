use chumsky::{
    IterParser, Parser,
    error::Rich,
    extra,
    input::ValueInput,
    primitive::{choice, just, todo},
    span::{SimpleSpan, Spanned},
};
use fsl_lexer::Token;

use crate::{
    Field, Item, ModuleDef, RegDecl, TraitDef,
    parsers::{
        atom::{self, ident_def},
        expr::expr_def,
        typedef::type_def,
    },
};

pub(super) fn fields_def<'tok, I>()
-> impl Parser<'tok, I, Vec<Spanned<Field>>, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    let field = choice((reg_def().map(Field::Reg),));

    field
        .repeated()
        .map_with(|f, e| Spanned {
            inner: f,
            span: e.span(),
        })
        .collect()
}

/// レジスタ宣言のパーサー
pub(super) fn reg_def<'tok, I>() -> impl Parser<'tok, I, RegDecl, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::Reg)
        // レジスタ名
        .ignore_then(ident_def())
        // 型
        .then(just(Token::Colon).ignore_then(type_def()))
        // 初期化
        .then(just(Token::Eq).ignore_then(expr_def()).or_not())
        .map(|((name, ty), expr)| RegDecl {
            name,
            ty,
            init: expr,
        })
}
