//! 式のパーサ
//!
//! `Block` 廃止により旧来の文もすべて式へ統合された．
//! 主なパーサ構造
//! 1. プリミティブ式  リテラル，識別子，括弧式，`if`，`new`，ブロック式
//! 2. Pratt 演算子による中置・前置・後置の解析
//! 3. 後置 `match { ... }`
//! 4. 代入  `:=` / `=`  と，`val`/`seq`/`par` 等の文相当の式  block.rs

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
    BinaryOp, Expr, Expr_, MatchArm, UnaryOp,
    parsers::{
        RecExpr,
        atom::ident_def,
        block::{block_def, control_def},
        pattern::pattern_def,
    },
};

/// 式パーサ
///
/// 再帰ハンドル `expr` を受け取り，自己再帰する式パーサの実体を構築する
pub(super) fn expr_def<'tok, I>(
    expr: RecExpr<'tok, I>,
) -> impl Parser<'tok, I, Expr, extra::Err<Rich<'tok, Token>>> + Clone
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    // ---- リテラル ----
    let int_lit = select! {
        Token::IntLit(n) => Expr_::IntLit(n),
    }
    .spanned();
    let bit_lit = select! {
        Token::BitLit(n) => Expr_::BitLit(n),
    }
    .spanned();
    let str_lit = select! {
        Token::StringLit(s) => Expr_::StringLit(s),
    }
    .spanned();
    let true_lit = just(Token::True).map(|_| Expr_::Bool(true)).spanned();
    let false_lit = just(Token::False).map(|_| Expr_::Bool(false)).spanned();

    // ---- 識別子参照 ----
    let path_expr = ident_def().map(|id| Expr_::Variable(id)).spanned();

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

    // ---- if 式 ----
    // SCC 縮退により then／else の分岐部は単なる `expr` でよい
    let if_expr = just(Token::If)
        // 条件
        .ignore_then(
            expr.clone()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        // then
        .then(expr.clone())
        // else
        .then(just(Token::Else).ignore_then(expr.clone()).or_not())
        .map(|((c, t), e_opt)| Expr_::If(Box::new(c), Box::new(t), e_opt.map(Box::new)))
        .spanned();

    // ---- new ModName ----
    let new_expr = just(Token::New)
        .ignore_then(ident_def())
        .map(|name| Expr_::New(name))
        .spanned();

    let primary = choice((
        int_lit,
        bit_lit,
        str_lit,
        true_lit,
        false_lit,
        if_expr,
        new_expr,
        paren_or_tuple,
        block_def(expr.clone()),
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
            // 前置  単項演算  `~` ビット否定 / `!` 論理否定 / `-` 単項マイナス
            prefix(
                11,
                just(Token::Tilde),
                |_, rhs, e: &mut MapExtra<'_, '_, _, _>| mk_unary(UnaryOp::BitNot, rhs, e.span()),
            ),
            prefix(
                11,
                just(Token::Bang),
                |_, rhs, e: &mut MapExtra<'_, '_, _, _>| mk_unary(UnaryOp::LogNot, rhs, e.span()),
            ),
            prefix(
                11,
                just(Token::Minus),
                |_, rhs, e: &mut MapExtra<'_, '_, _, _>| mk_unary(UnaryOp::Neg, rhs, e.span()),
            ),
            // 前置  リダクション演算  `&` AND / `|` OR / `^` XOR
            prefix(
                11,
                just(Token::Amp),
                |_, rhs, e: &mut MapExtra<'_, '_, _, _>| mk_unary(UnaryOp::ReducAnd, rhs, e.span()),
            ),
            prefix(
                11,
                just(Token::Pipe),
                |_, rhs, e: &mut MapExtra<'_, '_, _, _>| mk_unary(UnaryOp::ReducOr, rhs, e.span()),
            ),
            prefix(
                11,
                just(Token::Caret),
                |_, rhs, e: &mut MapExtra<'_, '_, _, _>| mk_unary(UnaryOp::ReducXor, rhs, e.span()),
            ),
            // 中置  `n # x` 符号拡張
            infix(
                left(10),
                just(Token::Hash),
                |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::SignExt, l, r, e.span()),
            ),
            // `*`
            infix(
                left(9),
                just(Token::Star),
                |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::Mul, l, r, e.span()),
            ),
            // `+` `-`
            infix(
                left(8),
                just(Token::Plus),
                |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::Add, l, r, e.span()),
            ),
            infix(
                left(8),
                just(Token::Minus),
                |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::Sub, l, r, e.span()),
            ),
            // `++` 連結
            infix(
                left(7),
                just(Token::PlusPlus),
                |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::Concat, l, r, e.span()),
            ),
            // シフト  `<<` 論理左 / `>>` 算術右 / `>>>` 論理右
            infix(
                left(6),
                just(Token::Shl),
                |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::Sll, l, r, e.span()),
            ),
            infix(
                left(6),
                just(Token::Shr),
                |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::Sra, l, r, e.span()),
            ),
            infix(
                left(6),
                just(Token::ShrLogical),
                |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::Srl, l, r, e.span()),
            ),
            // `&`
            infix(
                left(5),
                just(Token::Amp),
                |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::BitAnd, l, r, e.span()),
            ),
            // `|` `^`
            infix(
                left(4),
                just(Token::Pipe),
                |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::BitOr, l, r, e.span()),
            ),
            infix(
                left(4),
                just(Token::Caret),
                |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::BitXor, l, r, e.span()),
            ),
            // 比較  pratt 演算子タプルの最大長  26  に収めるため入れ子にする
            (
                infix(
                    left(3),
                    just(Token::EqEq),
                    |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::Eq, l, r, e.span()),
                ),
                infix(
                    left(3),
                    just(Token::NotEq),
                    |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::Ne, l, r, e.span()),
                ),
                infix(
                    left(3),
                    just(Token::Lt),
                    |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::Lt, l, r, e.span()),
                ),
                infix(
                    left(3),
                    just(Token::Le),
                    |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::Le, l, r, e.span()),
                ),
                infix(
                    left(3),
                    just(Token::Gt),
                    |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::Gt, l, r, e.span()),
                ),
                infix(
                    left(3),
                    just(Token::Ge),
                    |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::Ge, l, r, e.span()),
                ),
            ),
            // 論理
            infix(
                left(2),
                just(Token::AmpAmp),
                |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::LogAnd, l, r, e.span()),
            ),
            infix(
                left(1),
                just(Token::PipePipe),
                |l, _, r, e: &mut MapExtra<'_, '_, _, _>| mk_bin(BinaryOp::LogOr, l, r, e.span()),
            ),
        ))
        .boxed();

    // ---- 後置 match ----
    // `expr match { case <pat> => <body> ; ... }`
    let match_arm = just(Token::Case)
        .ignore_then(pattern_def())
        .then_ignore(just(Token::FatArrow))
        .then(expr.clone())
        .map(|(pattern, body)| MatchArm { pattern, body })
        .spanned();

    let match_arms = match_arm
        .separated_by(just(Token::Semicolon).repeated())
        .allow_leading()
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LBrace), just(Token::RBrace));

    let with_match = pratt_expr
        .foldl_with(
            just(Token::Match).ignore_then(match_arms).repeated(),
            |lhs: Expr, arms, e| Spanned {
                inner: Expr_::Match(Box::new(lhs), arms),
                span: e.span(),
            },
        )
        .boxed();

    // ---- 代入  `lhs := rhs` / `lhs = rhs`  または式そのまま ----
    let assign_or_expr = with_match
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
        .map_with(|(lhs, opt), e| match opt {
            // `:=` は記憶素子への代入
            Some((true, rhs)) => Spanned {
                inner: Expr_::MemAssign(Box::new(lhs), Box::new(rhs)),
                span: e.span(),
            },
            // `=` は端子への代入
            Some((false, rhs)) => Spanned {
                inner: Expr_::PortAssign(Box::new(lhs), Box::new(rhs)),
                span: e.span(),
            },
            None => lhs,
        });

    // `val`/`generate`/`seq`/`par`/`any`/`alt`/`relay`/`finish`/`goto` は
    // いずれもキーワード先頭で代入式と曖昧にならないため先に試す
    choice((control_def(expr.clone()), assign_or_expr)).boxed()
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
