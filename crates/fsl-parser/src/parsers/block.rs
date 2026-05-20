//! ブロックと制御フロー式のパーサ
//!
//! brace ブロック `{ ... }` と，`seq`/`par`/`any`/`alt` および
//! `val`/`generate`/`relay`/`finish`/`goto` といった文相当の式

use chumsky::{
    IterParser, Parser,
    error::Rich,
    extra,
    input::ValueInput,
    primitive::{choice, just, none_of},
    recovery::via_parser,
    span::{SimpleSpan, Spanned},
};
use fsl_lexer::Token;

use crate::{
    Case, Expr, Expr_,
    parsers::{RecExpr, atom::ident_def, rbrace, valdecl::val_decl_def},
};

/// brace ブロック `{ expr* }`
///
/// 最後の式の値を返す式 `Expr_::Block` として解釈する
pub(super) fn block_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, Expr, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    brace_seq(expr)
        .map_with(|exprs, e| Spanned {
            inner: Expr_::Block(exprs),
            span: e.span(),
        })
        // `Recursive::define` は parser に Clone 境界を要求する
        // `impl Parser` は Clone を露出しないため `.boxed()` で包む
        .boxed()
}

/// 文相当の式  代入式と曖昧にならないキーワード先頭の式の集合
///
/// `val` 宣言・`generate`/`relay`/`finish`/`goto`・`seq`/`par`/`any`/`alt`
pub(super) fn control_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, Expr, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // val 宣言式
    let val = val_decl_def(expr.clone()).map(Expr_::ValDecl).spanned();

    // 引数列 `(e, e, ...)`
    let arg_list = expr
        .clone()
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .collect()
        .delimited_by(just(Token::LParen), just(Token::RParen));

    // generate / relay  ステージ起動
    let generate = just(Token::Generate)
        .ignore_then(ident_def())
        .then(arg_list.clone())
        .map(|(target, args)| Expr_::Generate(target, args))
        .spanned();
    let relay = just(Token::Relay)
        .ignore_then(ident_def())
        .then(arg_list)
        .map(|(target, args)| Expr_::Relay(target, args))
        .spanned();

    // finish / goto
    let finish = just(Token::Finish).map(|_| Expr_::Finish).spanned();
    let goto = just(Token::Goto)
        .ignore_then(ident_def())
        .map(Expr_::Goto)
        .spanned();

    // seq / par  `{ expr* }` を順次・並列実行する式
    let seq = just(Token::Seq)
        .ignore_then(brace_seq(expr.clone()))
        .map_with(|exprs, e| Spanned {
            inner: Expr_::Seq(exprs),
            span: e.span(),
        });
    let par = just(Token::Par)
        .ignore_then(brace_seq(expr.clone()))
        .map_with(|exprs, e| Spanned {
            inner: Expr_::Par(exprs),
            span: e.span(),
        });

    // any / alt  `{ cond : body ... else : body }`
    let any = just(Token::Any)
        .ignore_then(cases(expr.clone()))
        .map_with(|(arms, els), e| Spanned {
            inner: Expr_::Any(arms, els.map(Box::new)),
            span: e.span(),
        });
    let alt = just(Token::Alt)
        .ignore_then(cases(expr.clone()))
        .map_with(|(arms, els), e| Spanned {
            inner: Expr_::Alt(arms, els.map(Box::new)),
            span: e.span(),
        });

    choice((val, generate, relay, finish, goto, seq, par, any, alt)).boxed()
}

// ============================================================
// 局所ヘルパー
// ============================================================

/// `{ expr* }`  式列を `Vec<Expr>` として返す
///
/// FSL の文末セミコロンは任意であり区切りとしても省略できるため
/// 区切りを0個以上のセミコロンとし，先頭・末尾も許容する．
fn brace_seq<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, Vec<Expr>, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // 各式は失敗時に Expr_::Error へ復旧し，後続の式の解析を続行する
    expr.clone()
        .recover_with(via_parser(expr_recovery(expr)))
        .separated_by(just(Token::Semicolon).repeated())
        .allow_leading()
        .allow_trailing()
        .collect()
        // 閉じ `}` の欠落から復旧し，編集途中でも本体を解析結果に残す
        .delimited_by(just(Token::LBrace), rbrace())
}

/// `{ cond : body ... [else : body] }`  any/alt の本体
fn cases<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, (Vec<Spanned<Case>>, Option<Expr>), extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // 通常のケース  `cond : body`
    let case = expr
        .clone()
        .then_ignore(just(Token::Colon))
        .then(expr.clone())
        .map(|(cond, body)| Case { cond, body })
        .spanned();

    // 既定ケース  `else : body`
    let else_arm = just(Token::Else)
        .ignore_then(just(Token::Colon))
        .ignore_then(expr.clone());

    case.separated_by(just(Token::Semicolon).repeated())
        .allow_leading()
        .allow_trailing()
        .collect()
        .then(else_arm.or_not())
        // 閉じ `}` の欠落から復旧する
        .delimited_by(just(Token::LBrace), rbrace())
}

/// 式の解析失敗からの復旧パーサ
///
/// 式として解釈できないトークンを，次に式が始まる位置まで読み飛ばして
/// `Expr_::Error` を生成する．`expr` 自体を先読みに使うため，識別子で
/// 始まる代入・呼出を読み過ぎず，誤りの後ろの正常な式を取りこぼさない．
/// `}` と `;` はブロック区切りとして消費せず上位に残す．
fn expr_recovery<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, Expr, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    none_of([Token::RBrace, Token::Semicolon])
        .and_is(expr.not())
        .repeated()
        .at_least(1)
        .collect::<Vec<_>>()
        .map_with(|_, e| Spanned {
            inner: Expr_::Error,
            span: e.span(),
        })
}
