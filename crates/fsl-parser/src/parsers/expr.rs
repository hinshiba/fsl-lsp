use chumsky::{Parser, error::Rich, extra, input::ValueInput, primitive::todo, span::SimpleSpan};
use fsl_lexer::Token;

use crate::{
    Expr,
    parsers::{RecBlock, RecExpr},
};

/// 式パーサ
pub(super) fn expr_def<'tok, I>(
    block: RecBlock<'tok, I>,
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, Expr, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // TODO  実装する  ここでは `block.clone()` をブロック式として
    //       受け取り，`expr.clone()` で部分式を再帰させる
    todo()
}
