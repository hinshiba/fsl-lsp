use chumsky::{
    IterParser, Parser,
    error::Rich,
    extra,
    input::ValueInput,
    primitive::{choice, just, todo},
    select,
    span::SimpleSpan,
};
use fsl_lexer::Token;

use crate::{
    Block, BlockKind, Expr, Statement,
    parsers::{RecBlock, RecExpr, valdecl::val_decl_def},
};

/// 各種命令の列
///
/// `block` と `expr` は呼び出し側で `Recursive::declare()` して
/// 渡される未定義ハンドル
pub(super) fn block_def<'tok, I>(
    block: RecBlock<'tok, I>,
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, Block, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // par/seq/any/alt ブロック  block を再帰参照する
    let block_kind = select!(
        Token::Par => BlockKind::Par,
        Token::Seq => BlockKind::Seq,
        Token::Any => BlockKind::Any,
        Token::Alt => BlockKind::Alt,
    )
    .then(block.clone());

    // 1つの命令
    let statement = choice((
        val_decl_def(expr.clone()).map(Statement::Val),
        block_kind.map(|(k, b)| Statement::BlockKind(k, b)),
    ));

    statement
        .then_ignore(just(Token::Semicolon).not())
        .spanned()
        .repeated()
        .collect()
        .map(|t| Block { stmts: t })
        // `Recursive::define` は parser に Clone 境界を要求する
        // `impl Parser` は Clone を露出しないため
        // `.boxed()` (Rc<dyn Parser>) に包んで Clone 可能にする
        .boxed()
}

fn regassign_def<'tok, I>() -> impl Parser<'tok, I, (Expr, Expr), extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    todo()
}

fn assign_def<'tok, I>() -> impl Parser<'tok, I, (Expr, Expr), extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    todo()
}
