//! 型のパーサ

use chumsky::{
    IterParser, Parser,
    error::Rich,
    extra,
    input::ValueInput,
    primitive::{choice, just},
    recursive::recursive,
    select,
    span::{SimpleSpan, Spanned},
};
use fsl_lexer::Token;

use crate::{Expr, Expr_, FslType, FslType_, parsers::atom::ident_def};

/// FSL の型表記をパースする
///
/// `Bit(n)` の `n` には任意の式を許容したいが
/// 型と式の間で完全な相互再帰を作ると親 API に大きく波及する
/// 実用上 `Bit(...)` の引数は整数リテラルか
/// 識別子参照のみのため小さい局所サブ式で受ける
pub(super) fn type_def<'tok, I>()
-> impl Parser<'tok, I, FslType, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    recursive(|ty| {
        // `Bit(n)` の n に許す簡易式  整数リテラル
        // または識別子参照のみ
        let bit_arg = {
            let int_lit = select! {
                Token::IntLit(s) = e => Spanned {
                    inner: Expr_::Int(s),
                    span: e.span(),
                },
            };
            let path = select! {
                Token::Ident(s) = e => Spanned {
                    inner: Expr_::Path(Spanned { inner: s, span: e.span() }),
                    span: e.span(),
                },
            };
            choice((int_lit, path))
        };

        //  Bit / Array / List / 名前付き / 組込型は字句化上いずれも `Ident` で来るため
        // 引数列の有無で先に分岐する
        let bit_ty = select! { Token::Ident(s) if s == "Bit" => () }
            .ignore_then(bit_arg.delimited_by(just(Token::LParen), just(Token::RParen)))
            .map(|n: Expr| FslType_::Bit(Box::new(n)))
            .spanned();

        let array_ty = select! { Token::Ident(s) if s == "Array" => () }
            .ignore_then(ty.clone().delimited_by(just(Token::LBracket), just(Token::RBracket)))
            .map(|inner: FslType| FslType_::Array(Box::new(inner)))
            .spanned();

        let list_ty = select! { Token::Ident(s) if s == "List" => () }
            .ignore_then(ty.clone().delimited_by(just(Token::LBracket), just(Token::RBracket)))
            .map(|inner: FslType| FslType_::List(Box::new(inner)))
            .spanned();

        // タプル型 `(T, T, ...)`
        let tuple_ty = ty
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .map(|tys| FslType_::Tuple(tys))
            .spanned();

        // 識別子型  組込名は予約ワード扱いに
        let named_or_builtin = ident_def().map(|name| {
            let kind = match name.inner.as_str() {
                "Unit" => FslType_::Unit,
                "Boolean" => FslType_::Boolean,
                "Int" => FslType_::Int,
                "String" => FslType_::String,
                _ => FslType_::Named(name.clone()),
            };
            Spanned {
                inner: kind,
                span: name.span,
            }
        });

        choice((bit_ty, array_ty, list_ty, tuple_ty, named_or_builtin))
    })
}
