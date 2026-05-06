use chumsky::{Parser, error::Rich, extra, input::ValueInput, primitive::todo, span::SimpleSpan};
use fsl_lexer::Token;

use crate::FslType;

pub(super) fn type_def<'tok, I>()
-> impl Parser<'tok, I, FslType, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    todo()
}
