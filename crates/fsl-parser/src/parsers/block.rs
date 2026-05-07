//! ブロックと文のパーサ
//!
//! `Block` は `Spanned<Statement>` の列で構成される
//! 文の種類は val 宣言・代入・ブロック種別文・generate/relay/finish/goto・
//! 単独の式  式文 を含む

use chumsky::{
    IterParser, Parser,
    error::Rich,
    extra,
    input::ValueInput,
    primitive::{choice, just},
    select,
    span::SimpleSpan,
};
use fsl_lexer::Token;

use crate::{
    Block, BlockKind, Statement,
    parsers::{RecBlock, RecExpr, atom::ident_def, valdecl::val_decl_def},
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
    let stmt = stmt_def(block.clone(), expr.clone());

    // `Block` 本体  `{ stmt; stmt; ... }`
    // FSL の文末セミコロンは任意であり区切りとしても省略できる
    // `separated_by` の区切りを「0 個以上のセミコロン」にして
    // 先頭・末尾のセミコロンも許容させる
    let body = stmt
        .clone()
        .spanned()
        .separated_by(just(Token::Semicolon).repeated())
        .allow_leading()
        .allow_trailing()
        .collect();

    body.delimited_by(just(Token::LBrace), just(Token::RBrace))
        .map(|stmts| Block { stmts })
        // `Recursive::define` は parser に Clone 境界を要求する
        // `impl Parser` は Clone を露出しないため
        // `.boxed()`  Rc<dyn Parser>  に包んで Clone 可能にする
        .boxed()
}

/// 文 1 件のパーサ
///
/// 戻り値は `Statement`  span 情報は呼出側で付与する
pub(super) fn stmt_def<'tok, I>(
    block: RecBlock<'tok, I>,
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, Statement, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // par/seq/any/alt ブロック  block を再帰参照する
    let block_kind_stmt = select!(
        Token::Par => BlockKind::Par,
        Token::Seq => BlockKind::Seq,
        Token::Any => BlockKind::Any,
        Token::Alt => BlockKind::Alt,
    )
    .then(block.clone())
    .map(|(k, b)| Statement::BlockKind(k, b));

    // val 宣言文
    let val_stmt = val_decl_def(expr.clone()).map(Statement::Val);

    // 引数列  `(e, e, ...)`
    let arg_list = expr
        .clone()
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .collect()
        .delimited_by(just(Token::LParen), just(Token::RParen));

    // generate / relay
    let generate_stmt = just(Token::Generate)
        .ignore_then(ident_def())
        .then(arg_list.clone())
        .map(|(target, args)| Statement::Generate(target, args));

    let relay_stmt = just(Token::Relay)
        .ignore_then(ident_def())
        .then(arg_list)
        .map(|(target, args)| Statement::Relay(target, args));

    // finish / goto
    let finish_stmt = just(Token::Finish).map(|_| Statement::Finish);
    let goto_stmt = just(Token::Goto)
        .ignore_then(ident_def())
        .map(|target| Statement::Goto(target));

    // 式または代入
    // `lhs := rhs`  `lhs = rhs`  またはそのまま式文
    let assign_or_expr = expr
        .clone()
        .then(
            choice((
                just(Token::ColonEq)
                    .ignore_then(expr.clone())
                    .map(|r| (true, r)),
                just(Token::Eq)
                    .ignore_then(expr.clone())
                    .map(|r| (false, r)),
            ))
            .or_not(),
        )
        .map(|(lhs, opt)| match opt {
            Some((true, rhs)) => Statement::RegAssign(lhs, rhs),
            Some((false, rhs)) => Statement::Assign(lhs, rhs),
            None => Statement::Expr(lhs),
        });

    choice((
        val_stmt,
        block_kind_stmt,
        generate_stmt,
        relay_stmt,
        finish_stmt,
        goto_stmt,
        assign_or_expr,
    ))
    .boxed()
}
