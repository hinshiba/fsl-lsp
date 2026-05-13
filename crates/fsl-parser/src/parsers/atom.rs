use chumsky::{
    Parser,
    error::Rich,
    extra,
    input::ValueInput,
    select,
    span::{SimpleSpan, Spanned},
};
use fsl_lexer::Token;

use crate::Ident;

pub(super) fn ident_def<'tok, I>()
-> impl Parser<'tok, I, Ident, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    select! {
        Token::Ident(s) = e => Spanned { inner: s, span: e.span()}
    }
}
