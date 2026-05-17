use chumsky::{
    IterParser, Parser,
    error::Rich,
    extra,
    input::ValueInput,
    primitive::{choice, just},
    span::{SimpleSpan, Spanned},
};
use fsl_lexer::Token;

use crate::{
    Item, ModuleDef, TraitDef,
    parsers::{RecExpr, atom, field::fields_def},
};

/// アイテムのパーサー
///
/// `expr` の再帰ハンドルを受け取り，モジュール／トレイト本体へ配る
pub(super) fn item_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, Spanned<Item>, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    choice((
        module_def(expr.clone()).map(Item::Module),
        trait_def(expr).map(Item::Trait),
    ))
    .spanned()
}

/// モジュールのパーサー
pub(super) fn module_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, ModuleDef, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    let module_decl = just(Token::Module)
        // モジュール名
        .ignore_then(atom::ident_def())
        // 継承
        .then(just(Token::Extends).ignore_then(atom::ident_def()).or_not())
        // トレイトの実装
        .then(
            just(Token::With)
                .ignore_then(atom::ident_def())
                .repeated()
                .collect()
                .or_not(),
        );

    module_decl
        // {}に囲まれた領域のフィールド
        .then(fields_def(expr).delimited_by(just(Token::LBrace), just(Token::RBrace)))
        .map(|(((name, extends), with_traits), items)| ModuleDef {
            name,
            extends,
            with_traits,
            items,
        })
}

/// トレイトのパーサー
pub(super) fn trait_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, TraitDef, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    just(Token::Trait)
        // トレイト名
        .ignore_then(atom::ident_def())
        // {}に囲まれた領域のフィールド
        .then(fields_def(expr).delimited_by(just(Token::LBrace), just(Token::RBrace)))
        .map(|(name, items)| TraitDef { name, items })
}
