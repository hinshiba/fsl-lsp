//! match パターンのパーサ
//!
//! `case <pat> => <body>` の `<pat>` を解析する

use chumsky::{
    Parser,
    error::Rich,
    extra,
    input::ValueInput,
    primitive::choice,
    select,
    span::{SimpleSpan, Spanned},
};
use fsl_lexer::Token;

use crate::Pattern;

/// パターンのパーサ
pub(super) fn pattern_def<'tok, I>()
-> impl Parser<'tok, I, Pattern, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // `_` も Ident トークンとして字句化されるため
    // 識別子マッチより先にワイルドカードを試す
    let wildcard = select! {
        Token::Ident(s) if s == "_" => Pattern::Wildcard,
    };
    let int_pat = select! {
        Token::IntLit(s) => Pattern::IntLit(s),
    };
    let id_pat = select! {
        Token::Ident(s) = e => Pattern::Ident(Spanned { inner: s, span: e.span() }),
    };
    choice((wildcard, int_pat, id_pat))
}
