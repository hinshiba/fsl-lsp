//! 式のパーサ
//!
//! 主なパーサ構造
//! 1. プリミティブ式  リテラル，識別子，括弧式，`if`，`new`，ブロック式
//! 2. Pratt 演算子による中置・前置・後置の解析
//! 3. 後置 `match { ... }`

use chumsky::{
    IterParser, Parser,
    error::Rich,
    extra,
    input::{MapExtra, ValueInput},
    pratt::{infix, left, postfix, prefix},
    primitive::{choice, just},
    select,
    span::{SimpleSpan, Spanned},
};
use fsl_lexer::Token;

use crate::{
    BinaryOp, Block, Expr, Expr_, MatchArm, Statement, UnaryOp,
    parsers::{RecBlock, RecExpr, atom::ident_def, block::stmt_def, pattern::pattern_def},
};

/// 式パーサ
pub(super) fn expr_def<'tok, I>(
    block: RecBlock<'tok, I>,
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, Expr, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // ---- リテラル ----
    let int_lit = select! {
        Token::IntLit(s) => Expr_::Int(s),
    }
    .spanned();
    let str_lit = select! {
        Token::StringLit(s) => Expr_::Str(s),
    }
    .spanned();
    let true_lit = just(Token::True).map(|_| Expr_::Bool(true)).spanned();
    let false_lit = just(Token::False).map(|_| Expr_::Bool(false)).spanned();

    // ---- 識別子参照 ----
    let path_expr = ident_def().map(|id| Expr_::Path(id)).spanned();

    // ---- 括弧式・タプル・Unit ----
    let paren_or_tuple = expr
        .clone()
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LParen), just(Token::RParen))
        .map_with(|elems: Vec<Expr>, e| {
            let span = e.span();
            if elems.is_empty() {
                Spanned {
                    inner: Expr_::Unit,
                    span,
                }
            } else if elems.len() == 1 {
                let mut elems = elems;
                elems.remove(0)
            } else {
                Spanned {
                    inner: Expr_::Tuple(elems),
                    span,
                }
            }
        });

    // ---- ブロック式 ----
    let block_expr = block.clone().map_with(|b: Block, e| Spanned {
        inner: Expr_::Block(b),
        span: e.span(),
    });

    // ---- if 式 ----
    // then／else は単独式以外にブロックや制御文も来うる
    let branch_body = then_or_else_branch(block.clone(), expr.clone());
    let if_expr = just(Token::If)
        // 条件
        .ignore_then(
            expr.clone()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        // then
        .then(branch_body.clone())
        // else
        .then(just(Token::Else).ignore_then(branch_body.clone()).or_not())
        .map(|((c, t), e_opt)| {
            Expr_::If(Box::new(c), Box::new(t), e_opt.map(Box::new))
        })
        .spanned();

    // ---- new ModName ----
    let new_expr = just(Token::New)
        .ignore_then(ident_def())
        .map(|name| Expr_::New(name))
        .spanned();

    let primary = choice((
        int_lit,
        str_lit,
        true_lit,
        false_lit,
        if_expr,
        new_expr,
        paren_or_tuple,
        block_expr,
        path_expr,
    ))
    .boxed();

    // ---- pratt の各オペランド ----
    let call_args = expr
        .clone()
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LParen), just(Token::RParen));

    let dot_ident = just(Token::Dot).ignore_then(ident_def());

    // pratt 演算子定義  数値が大きいほど強く結合する
    let pratt_expr = primary
        .pratt((
            // 後置  `(args)` 関数呼出 / `.ident` フィールドアクセス
            postfix(
                12,
                call_args,
                |lhs: Expr, args: Vec<Expr>, e: &mut MapExtra<'_, '_, _, _>| Spanned {
                    inner: Expr_::Call(Box::new(lhs), args),
                    span: e.span(),
                },
            ),
            postfix(
                12,
                dot_ident,
                |lhs: Expr, name, e: &mut MapExtra<'_, '_, _, _>| Spanned {
                    inner: Expr_::Field(Box::new(lhs), name),
                    span: e.span(),
                },
            ),
            // 前置 単項演算
            prefix(11, just(Token::Tilde), |_, rhs, e: &mut MapExtra<'_, '_, _, _>| {
                mk_unary(UnaryOp::BitNot, rhs, e.span())
            }),
            prefix(11, just(Token::Bang), |_, rhs, e: &mut MapExtra<'_, '_, _, _>| {
                mk_unary(UnaryOp::LogNot, rhs, e.span())
            }),
            prefix(11, just(Token::Minus), |_, rhs, e: &mut MapExtra<'_, '_, _, _>| {
                mk_unary(UnaryOp::Neg, rhs, e.span())
            }),
            prefix(11, just(Token::Pipe), |_, rhs, e: &mut MapExtra<'_, '_, _, _>| {
                mk_unary(UnaryOp::RedOr, rhs, e.span())
            }),
            // 中置  `n # x` 符号拡張
            infix(left(10), just(Token::Hash), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::SignExt, l, r, e.span())
            }),
            // `*`
            infix(left(9), just(Token::Star), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::Mul, l, r, e.span())
            }),
            // `+` `-`
            infix(left(8), just(Token::Plus), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::Add, l, r, e.span())
            }),
            infix(left(8), just(Token::Minus), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::Sub, l, r, e.span())
            }),
            // `++` 連結
            infix(left(7), just(Token::PlusPlus), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::Concat, l, r, e.span())
            }),
            // シフト
            infix(left(6), just(Token::Shl), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::Shl, l, r, e.span())
            }),
            infix(left(6), just(Token::Shr), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::Shr, l, r, e.span())
            }),
            infix(left(6), just(Token::ShrLogical), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::ShrLogical, l, r, e.span())
            }),
            // `&`
            infix(left(5), just(Token::Amp), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::BitAnd, l, r, e.span())
            }),
            // `|` `^`
            infix(left(4), just(Token::Pipe), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::BitOr, l, r, e.span())
            }),
            infix(left(4), just(Token::Caret), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::BitXor, l, r, e.span())
            }),
            // 比較
            infix(left(3), just(Token::EqEq), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::Eq, l, r, e.span())
            }),
            infix(left(3), just(Token::NotEq), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::Ne, l, r, e.span())
            }),
            infix(left(3), just(Token::Lt), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::Lt, l, r, e.span())
            }),
            infix(left(3), just(Token::Le), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::Le, l, r, e.span())
            }),
            infix(left(3), just(Token::Gt), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::Gt, l, r, e.span())
            }),
            infix(left(3), just(Token::Ge), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::Ge, l, r, e.span())
            }),
            // 論理
            infix(left(2), just(Token::AmpAmp), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::LogAnd, l, r, e.span())
            }),
            infix(left(1), just(Token::PipePipe), |l, _, r, e: &mut MapExtra<'_, '_, _, _>| {
                mk_bin(BinaryOp::LogOr, l, r, e.span())
            }),
        ))
        .boxed();

    // ---- 後置 match ----
    // `expr match { case <pat> => <body> ; ... }`
    let match_arm = just(Token::Case)
        .ignore_then(pattern_def())
        .then_ignore(just(Token::FatArrow))
        .then(branch_body.clone())
        .map_with(|(p, body), e| {
            let s = e.span();
            MatchArm {
                pattern: p,
                body,
                span: s.start..s.end,
            }
        });

    let match_arms = just(Token::Semicolon)
        .repeated()
        .ignore_then(match_arm)
        .separated_by(just(Token::Semicolon).repeated())
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LBrace), just(Token::RBrace));

    let with_match = pratt_expr
        .foldl_with(
            just(Token::Match).ignore_then(match_arms).repeated(),
            |lhs: Expr, arms: Vec<MatchArm>, e| Spanned {
                inner: Expr_::Match(Box::new(lhs), arms),
                span: e.span(),
            },
        )
        .boxed();

    with_match
}

// ============================================================
// 補助関数
// ============================================================

fn mk_bin(op: BinaryOp, l: Expr, r: Expr, span: SimpleSpan) -> Expr {
    Spanned {
        inner: Expr_::Binary(op, Box::new(l), Box::new(r)),
        span,
    }
}

fn mk_unary(op: UnaryOp, rhs: Expr, span: SimpleSpan) -> Expr {
    Spanned {
        inner: Expr_::Unary(op, Box::new(rhs)),
        span,
    }
}

/// 分岐部  `if`-then/else や `match` の本体  を解析する
///
/// ブロック・式・`par`/`seq`/`any`/`alt` 制御文・`generate`/`relay`/
/// `finish`/`goto` 等の文も式として受ける  文として解析した場合は
/// 単一文ブロックの式 `Expr_::Block(Block { stmts: vec![s] })` に格上げする
fn then_or_else_branch<'tok, I>(
    block: RecBlock<'tok, I>,
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, Expr, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // ブロックは Expr_::Block で式に格上げ
    let block_branch = block.clone().map_with(|b: Block, e| Spanned {
        inner: Expr_::Block(b),
        span: e.span(),
    });

    // 制御文・宣言文の先頭キーワードを `rewind` で覗き見し
    // それらが現れたときに限り `stmt_def` を呼んで Stmt を消費する
    // それ以外の場合は通常の式  `expr`  にフォールバックする
    // この順番でないと `assign_or_expr` がほぼ何でも食ってしまう
    let stmt_keyword = select! {
        Token::Par => (),
        Token::Seq => (),
        Token::Any => (),
        Token::Alt => (),
        Token::Val => (),
        Token::Generate => (),
        Token::Relay => (),
        Token::Finish => (),
        Token::Goto => (),
    };
    let stmt_branch = stmt_keyword
        .rewind()
        .ignore_then(stmt_def(block.clone(), expr.clone()))
        .map_with(|s: Statement, e| {
            let span = e.span();
            let stmt = Spanned { inner: s, span };
            Spanned {
                inner: Expr_::Block(Block { stmts: vec![stmt] }),
                span,
            }
        });

    choice((stmt_branch, block_branch, expr))
}
