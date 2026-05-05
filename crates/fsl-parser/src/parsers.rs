//! FSL の構文解析器
//!
//! 入力は `fsl-lexer` のトリビア除去済みトークン列．

use chumsky::input::{Input, MappedInput, ValueInput};
use chumsky::pratt::{infix, left, postfix, prefix};
use chumsky::prelude::*;
use chumsky::recursive::Recursive;

use crate::ast::{self, *};
use crate::parsers::item::item_def;
use fsl_lexer::{Span, SpannedToken, Token};

mod atom;
mod expr;
mod field;
mod item;
mod typedef;

/// 構文エラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

/// 構文解析結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseResult {
    pub unit: CompilationUnit,
    pub errors: Vec<ParseError>,
}

fn token_proj(t: &(Token, Span)) -> (&Token, &Span) {
    (&t.0, &t.1)
}

type Toks<'a> =
    MappedInput<'a, Token, Span, &'a [(Token, Span)], fn(&(Token, Span)) -> (&Token, &Span)>;
type Extra<'a> = extra::Err<Rich<'a, Token, Span>>;

pub fn parse_token(tokens: Vec<SpannedToken>, src_end: usize) -> ParseResult {
    let eoi: Span = src_end..src_end;
    let proj: fn(&(Token, Span)) -> (&Token, &Span) = token_proj;
    let stream = tokens.as_slice().map(eoi, proj);
    let (unit_opt, errs) = parser().parse(stream).into_output_errors();
    let unit = unit_opt.unwrap_or_default();
    let errors = errs
        .into_iter()
        .map(|e| ParseError {
            message: format!("{:?}", e),
            span: e.span().clone(),
        })
        .collect();
    ParseResult { unit, errors }
}

// ============================================================
// メインパーサ
// ============================================================

fn parser<'tok, I>() -> impl Parser<'tok, I, Spanned<CompilationUnit>, extra::Err<Rich<'tok, Token>>>
where
    I: ValueInput<'tok, Token = Token, Span = SimpleSpan>,
{
    item_def()
        .repeated()
        .collect()
        .map_with(|items, e| Spanned {
            inner: CompilationUnit { items },
            span: e.span(),
        })
}

// ---- 補助関数 ----

fn parser2<'a>() -> impl Parser<'a, Toks<'a>, CompilationUnit, Extra<'a>> {
    // ---- 前方宣言 ----
    let mut expr = Recursive::declare();
    let mut stmt = Recursive::declare();
    let mut block = Recursive::declare();
    let mut ty = Recursive::declare();

    // ---- 型 ----
    {
        // パディング: `;` を任意個数許容
        let named_or_builtin = ident().map_with(|name, e| {
            let span: Span = e.span();
            let kind = match name.node.as_str() {
                "Unit" => TypeKind::Unit,
                "Boolean" => TypeKind::Boolean,
                "Int" => TypeKind::Int,
                "String" => TypeKind::String,
                _ => TypeKind::Named(name.clone()),
            };
            Type { kind, span }
        });

        let bit_ty = just(Token::Ident("Bit".to_string()))
            .ignore_then(
                expr.clone()
                    .delimited_by(just(Token::LParen), just(Token::RParen)),
            )
            .map_with(|n, e| Type {
                kind: TypeKind::Bit(Box::new(n)),
                span: e.span(),
            });

        let array_ty = just(Token::Ident("Array".to_string()))
            .ignore_then(
                ty.clone()
                    .delimited_by(just(Token::LBracket), just(Token::RBracket)),
            )
            .map_with(|inner, e| Type {
                kind: TypeKind::Array(Box::new(inner)),
                span: e.span(),
            });

        let list_ty = just(Token::Ident("List".to_string()))
            .ignore_then(
                ty.clone()
                    .delimited_by(just(Token::LBracket), just(Token::RBracket)),
            )
            .map_with(|inner, e| Type {
                kind: TypeKind::List(Box::new(inner)),
                span: e.span(),
            });

        let tuple_ty = ty
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .map_with(|tys, e| Type {
                kind: TypeKind::Tuple(tys),
                span: e.span(),
            });

        // Bit/Array/List は識別子として字句化されるため，最初に試す
        ty.define(choice((
            bit_ty,
            array_ty,
            list_ty,
            tuple_ty,
            named_or_builtin,
        )));
    }

    // ---- 識別子リスト・パラメータ列 ----
    let param = ident()
        .then(just(Token::Colon).ignore_then(ty.clone()).or_not())
        .map(|(name, ty)| Param { name, ty });

    let params = param
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LParen), just(Token::RParen));

    // ---- パターン ----
    let pattern = {
        let wildcard = select_ref! {
            Token::Ident(s) if s == "_" => Pattern::Wildcard
        };
        let id_pat = ident().map(Pattern::Ident);
        let int_pat = select_ref! {
            Token::IntLit(s) => Pattern::IntLit(s.clone())
        };
        // ワイルドカードを先に試す（`_` も Ident トークン）
        choice((wildcard, int_pat, id_pat))
    };

    // ---- 式 ----
    {
        let int_lit = select_ref! {
            Token::IntLit(s) = e => Expr {
                kind: ExprKind::Int(s.clone()),
                span: e.span(),
            }
        };
        let str_lit = select_ref! {
            Token::StringLit(s) = e => Expr {
                kind: ExprKind::Str(s.clone()),
                span: e.span(),
            }
        };
        let true_lit = just(Token::True).map_with(|_, e| Expr {
            kind: ExprKind::Bool(true),
            span: e.span(),
        });
        let false_lit = just(Token::False).map_with(|_, e| Expr {
            kind: ExprKind::Bool(false),
            span: e.span(),
        });

        let path_expr = ident().map_with(|id, e| Expr {
            kind: ExprKind::Path(id),
            span: e.span(),
        });

        // 括弧式・タプル・Unit
        let paren_or_tuple = expr
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .map_with(|elems, e| {
                let span: Span = e.span();
                if elems.is_empty() {
                    Expr {
                        kind: ExprKind::Unit,
                        span,
                    }
                } else if elems.len() == 1 {
                    let mut elems = elems;
                    elems.remove(0)
                } else {
                    Expr {
                        kind: ExprKind::Tuple(elems),
                        span,
                    }
                }
            });

        let block_expr = block.clone().map(|b: Block| {
            let span = b.span.clone();
            Expr {
                kind: ExprKind::Block(b),
                span,
            }
        });

        // if 式
        let if_expr = {
            let cond = expr
                .clone()
                .delimited_by(just(Token::LParen), just(Token::RParen));
            let then_branch = then_or_else_branch(stmt.clone(), expr.clone(), block.clone());
            just(Token::If)
                .ignore_then(cond)
                .then(then_branch.clone())
                .then(just(Token::Else).ignore_then(then_branch).or_not())
                .map_with(|((c, t), e_opt), info| Expr {
                    kind: ExprKind::If(Box::new(c), Box::new(t), e_opt.map(Box::new)),
                    span: info.span(),
                })
        };

        // new ModName
        let new_expr = just(Token::New)
            .ignore_then(ident())
            .map_with(|name, e| Expr {
                kind: ExprKind::New(name),
                span: e.span(),
            });

        // primary
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

        // pratt 演算子定義（高い数値ほど強く結合する）
        // postfix 12: `(args)` 関数呼び出し，`.ident` フィールド
        let call_args = expr
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        let dot_ident = just(Token::Dot).ignore_then(ident());

        let pratt_expr = primary
            .pratt((
                postfix(
                    12,
                    call_args,
                    |lhs: Expr, args, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        let span: Span = e.span();
                        Expr {
                            kind: ExprKind::Call(Box::new(lhs), args),
                            span,
                        }
                    },
                ),
                postfix(
                    12,
                    dot_ident,
                    |lhs: Expr, name: Ident, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        let span: Span = e.span();
                        Expr {
                            kind: ExprKind::Field(Box::new(lhs), name),
                            span,
                        }
                    },
                ),
                prefix(
                    11,
                    just(Token::Tilde),
                    |_, rhs: Expr, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| Expr {
                        kind: ExprKind::Unary(UnaryOp::BitNot, Box::new(rhs)),
                        span: e.span(),
                    },
                ),
                prefix(
                    11,
                    just(Token::Bang),
                    |_, rhs: Expr, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| Expr {
                        kind: ExprKind::Unary(UnaryOp::LogNot, Box::new(rhs)),
                        span: e.span(),
                    },
                ),
                prefix(
                    11,
                    just(Token::Minus),
                    |_, rhs: Expr, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| Expr {
                        kind: ExprKind::Unary(UnaryOp::Neg, Box::new(rhs)),
                        span: e.span(),
                    },
                ),
                prefix(
                    11,
                    just(Token::Pipe),
                    |_, rhs: Expr, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| Expr {
                        kind: ExprKind::Unary(UnaryOp::RedOr, Box::new(rhs)),
                        span: e.span(),
                    },
                ),
                infix(
                    left(10),
                    just(Token::Hash),
                    |l: Expr, _, r: Expr, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::SignExt, l, r, e.span())
                    },
                ),
                infix(
                    left(9),
                    just(Token::Star),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::Mul, l, r, e.span())
                    },
                ),
                infix(
                    left(8),
                    just(Token::Plus),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::Add, l, r, e.span())
                    },
                ),
                infix(
                    left(8),
                    just(Token::Minus),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::Sub, l, r, e.span())
                    },
                ),
                infix(
                    left(7),
                    just(Token::PlusPlus),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::Concat, l, r, e.span())
                    },
                ),
                infix(
                    left(6),
                    just(Token::Shl),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::Shl, l, r, e.span())
                    },
                ),
                infix(
                    left(6),
                    just(Token::Shr),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::Shr, l, r, e.span())
                    },
                ),
                infix(
                    left(6),
                    just(Token::ShrLogical),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::ShrLogical, l, r, e.span())
                    },
                ),
                infix(
                    left(5),
                    just(Token::Amp),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::BitAnd, l, r, e.span())
                    },
                ),
                infix(
                    left(4),
                    just(Token::Pipe),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::BitOr, l, r, e.span())
                    },
                ),
                infix(
                    left(4),
                    just(Token::Caret),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::BitXor, l, r, e.span())
                    },
                ),
                infix(
                    left(3),
                    just(Token::EqEq),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::Eq, l, r, e.span())
                    },
                ),
                infix(
                    left(3),
                    just(Token::NotEq),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::Ne, l, r, e.span())
                    },
                ),
                infix(
                    left(3),
                    just(Token::Lt),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::Lt, l, r, e.span())
                    },
                ),
                infix(
                    left(3),
                    just(Token::Le),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::Le, l, r, e.span())
                    },
                ),
                infix(
                    left(3),
                    just(Token::Gt),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::Gt, l, r, e.span())
                    },
                ),
                infix(
                    left(3),
                    just(Token::Ge),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::Ge, l, r, e.span())
                    },
                ),
                infix(
                    left(2),
                    just(Token::AmpAmp),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::LogAnd, l, r, e.span())
                    },
                ),
                infix(
                    left(1),
                    just(Token::PipePipe),
                    |l, _, r, e: &mut chumsky::input::MapExtra<'_, '_, _, _>| {
                        mk_bin(BinaryOp::LogOr, l, r, e.span())
                    },
                ),
            ))
            .boxed();

        // match 後置 (`expr match { case ... }`)
        let match_arm = just(Token::Case)
            .ignore_then(pattern.clone())
            .then_ignore(just(Token::FatArrow))
            .then(then_or_else_branch(
                stmt.clone(),
                expr.clone(),
                block.clone(),
            ))
            .map_with(|(p, body), e| MatchArm {
                pattern: p,
                body,
                span: e.span(),
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
                |lhs: Expr, arms: Vec<MatchArm>, info| {
                    let span: Span = info.span();
                    Expr {
                        kind: ExprKind::Match(Box::new(lhs), arms),
                        span,
                    }
                },
            )
            .boxed();

        expr.define(with_match);
    }

    // ---- ブロック ----
    {
        let body = just(Token::Semicolon)
            .repeated()
            .ignore_then(stmt.clone())
            .separated_by(just(Token::Semicolon).repeated())
            .allow_trailing()
            .collect::<Vec<_>>();

        let blk = body
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map_with(|stmts, e| Block {
                stmts,
                span: e.span(),
            });
        block.define(blk);
    }

    // ---- val 共通 ----
    let val_pattern = {
        let single = ident().map(ValPattern::Single);
        let tuple = ident()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .map(ValPattern::Tuple);
        choice((tuple, single))
    };

    // ---- 文 ----
    {
        let val_stmt = just(Token::Val)
            .ignore_then(val_pattern.clone())
            .then(just(Token::Colon).ignore_then(ty.clone()).or_not())
            .then_ignore(just(Token::Eq))
            .then(expr.clone())
            .map_with(|((pattern, ty), init), e| ValDecl {
                pattern,
                ty,
                init,
                span: e.span(),
            });

        let val_stmt_wrap = val_stmt.clone().map_with(|v, e| Stmt {
            kind: StmtKind::Val(v),
            span: e.span(),
        });

        let block_kind_stmt = choice((
            just(Token::Par).to(BlockKind::Par),
            just(Token::Seq).to(BlockKind::Seq),
            just(Token::Any).to(BlockKind::Any),
            just(Token::Alt).to(BlockKind::Alt),
        ))
        .then(block.clone())
        .map_with(|(k, b), e| Stmt {
            kind: StmtKind::BlockKind(k, b),
            span: e.span(),
        });

        let arg_list = expr
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        let generate_stmt = just(Token::Generate)
            .ignore_then(ident())
            .then(arg_list.clone())
            .map_with(|(target, args), e| Stmt {
                kind: StmtKind::Generate(target, args),
                span: e.span(),
            });

        let relay_stmt = just(Token::Relay)
            .ignore_then(ident())
            .then(arg_list)
            .map_with(|(target, args), e| Stmt {
                kind: StmtKind::Relay(target, args),
                span: e.span(),
            });

        let finish_stmt = just(Token::Finish).map_with(|_, e| Stmt {
            kind: StmtKind::Finish,
            span: e.span(),
        });

        let goto_stmt = just(Token::Goto)
            .ignore_then(ident())
            .map_with(|target, e| Stmt {
                kind: StmtKind::Goto(target),
                span: e.span(),
            });

        // 式または代入
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
            .map_with(|(lhs, opt), e| {
                let span: Span = e.span();
                let kind = match opt {
                    Some((true, rhs)) => StmtKind::RegAssign(lhs, rhs),
                    Some((false, rhs)) => StmtKind::Assign(lhs, rhs),
                    None => StmtKind::Expr(lhs),
                };
                Stmt { kind, span }
            });

        let s = choice((
            val_stmt_wrap,
            block_kind_stmt,
            generate_stmt,
            relay_stmt,
            finish_stmt,
            goto_stmt,
            assign_or_expr,
        ))
        .boxed();

        stmt.define(s);
    }

    // ---- 各種モジュールアイテム ----
    let val_stmt_def = just(Token::Val)
        .ignore_then(val_pattern.clone())
        .then(just(Token::Colon).ignore_then(ty.clone()).or_not())
        .then_ignore(just(Token::Eq))
        .then(expr.clone())
        .map_with(|((pattern, ty), init), e| ValDecl {
            pattern,
            ty,
            init,
            span: e.span(),
        });

    let reg_decl = just(Token::Reg)
        .ignore_then(ident())
        .then_ignore(just(Token::Colon))
        .then(ty.clone())
        .then(just(Token::Eq).ignore_then(expr.clone()).or_not())
        .map_with(|((name, ty), init), e| RegDecl {
            name,
            ty,
            init,
            span: e.span(),
        });

    let mem_decl = just(Token::Mem)
        .ignore_then(
            ty.clone()
                .delimited_by(just(Token::LBracket), just(Token::RBracket)),
        )
        .then(ident())
        .then(
            expr.clone()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(
            just(Token::Eq)
                .ignore_then(
                    expr.clone()
                        .separated_by(just(Token::Comma))
                        .allow_trailing()
                        .collect::<Vec<_>>()
                        .delimited_by(just(Token::LParen), just(Token::RParen)),
                )
                .or_not(),
        )
        .map_with(|(((elem_ty, name), size), init_opt), e| MemDecl {
            name,
            elem_ty,
            size,
            init: init_opt.unwrap_or_default(),
            span: e.span(),
        });

    let input_decl = just(Token::Input)
        .ignore_then(ident())
        .then_ignore(just(Token::Colon))
        .then(ty.clone())
        .map_with(|(name, ty), e| PortDecl {
            name,
            ty,
            span: e.span(),
        });

    // output port / output def
    let output_item = just(Token::Output).ignore_then(choice((
        // `output def name(params): T`
        just(Token::Def)
            .ignore_then(ident())
            .then(params.clone())
            .then_ignore(just(Token::Colon))
            .then(ty.clone())
            .map_with(|((name, params), ret), e| {
                Field::OutputFn(OutputFnDecl {
                    name,
                    params,
                    ret,
                    span: e.span(),
                })
            }),
        // `output name: T`
        ident()
            .then_ignore(just(Token::Colon))
            .then(ty.clone())
            .map_with(|(name, ty), e| {
                Field::Output(PortDecl {
                    name,
                    ty,
                    span: e.span(),
                })
            }),
    )));

    let fn_def = {
        // `[private] def [recv.]name(params)[: T] (= body | seq block | par block)`
        let body = choice((
            just(Token::Eq)
                .ignore_then(choice((
                    block.clone(),
                    expr.clone().map(|e: Expr| {
                        let span = e.span.clone();
                        Block {
                            stmts: vec![Stmt {
                                kind: StmtKind::Expr(e),
                                span: span.clone(),
                            }],
                            span,
                        }
                    }),
                )))
                .map(|b| (FnBodyKind::Expr, b)),
            just(Token::Seq)
                .ignore_then(block.clone())
                .map(|b| (FnBodyKind::Seq, b)),
            just(Token::Par)
                .ignore_then(block.clone())
                .map(|b| (FnBodyKind::Par, b)),
        ));

        just(Token::Private)
            .or_not()
            .then_ignore(just(Token::Def))
            .then(ident())
            .then(just(Token::Dot).ignore_then(ident()).or_not())
            .then(params.clone())
            .then(just(Token::Colon).ignore_then(ty.clone()).or_not())
            .then(body)
            .map_with(
                |(((((priv_kw, first), recv_opt), params), ret), (body_kind, body)), e| {
                    let (receiver, name) = if let Some(real) = recv_opt {
                        (Some(first), real)
                    } else {
                        (None, first)
                    };
                    FnDef {
                        is_private: priv_kw.is_some(),
                        receiver,
                        name,
                        params,
                        ret,
                        body_kind,
                        body,
                        span: e.span(),
                    }
                },
            )
    };

    let always_block = just(Token::Always)
        .ignore_then(block.clone())
        .map_with(|b, e| {
            let mut b = b;
            b.span = e.span();
            Field::Always(b)
        });
    let initial_block = just(Token::Initial)
        .ignore_then(block.clone())
        .map_with(|b, e| {
            let mut b = b;
            b.span = e.span();
            Field::Initial(b)
        });

    // stage 定義
    let state_def = just(Token::State)
        .ignore_then(ident())
        .then(stmt.clone())
        .map_with(|(name, body), e| StateDef {
            name,
            body,
            span: e.span(),
        });

    let stage_item = choice((
        state_def.map(StageItem::State),
        reg_decl.clone().map(StageItem::Reg),
        mem_decl.clone().map(StageItem::Mem),
        val_stmt_def.clone().map(StageItem::Val),
        stmt.clone().map(StageItem::Stmt),
    ));

    let stage_body = just(Token::Semicolon)
        .repeated()
        .ignore_then(stage_item)
        .separated_by(just(Token::Semicolon).repeated())
        .allow_trailing()
        .collect::<Vec<_>>();

    let stage_def = just(Token::Stage)
        .ignore_then(ident())
        .then(params.clone())
        .then(stage_body.delimited_by(just(Token::LBrace), just(Token::RBrace)))
        .map_with(|((name, params), body), e| StageDef {
            name,
            params,
            body,
            span: e.span(),
        });

    let type_field = ident()
        .then_ignore(just(Token::Colon))
        .then(ty.clone())
        .map(|(name, ty)| TypeField { name, ty });

    let type_def = just(Token::Type)
        .ignore_then(ident())
        .then(
            type_field
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .map_with(|(name, fields), e| TypeDef {
            name,
            fields,
            span: e.span(),
        });

    // val または new によるインスタンス
    let val_or_instance = just(Token::Val)
        .ignore_then(val_pattern.clone())
        .then(just(Token::Colon).ignore_then(ty.clone()).or_not())
        .then_ignore(just(Token::Eq))
        .then(choice((
            // `new ModName` を即値とするケース
            just(Token::New)
                .ignore_then(ident())
                .map(|name| Either2::Instance(name)),
            expr.clone().map(Either2::Expr),
        )))
        .map_with(|((pattern, ty), rhs), e| {
            let span: Span = e.span();
            match (&pattern, rhs) {
                (ValPattern::Single(name), Either2::Instance(module_name)) => {
                    Field::Instance(InstanceDecl {
                        name: name.clone(),
                        module_name,
                        span,
                    })
                }
                (_, Either2::Instance(module_name)) => {
                    // tuple パターンに対する new は構文外として ValDecl に格上げ
                    Field::Val(ValDecl {
                        pattern,
                        ty,
                        init: Expr {
                            kind: ExprKind::New(module_name.clone()),
                            span: module_name.span.clone(),
                        },
                        span,
                    })
                }
                (_, Either2::Expr(init)) => Field::Val(ValDecl {
                    pattern,
                    ty,
                    init,
                    span,
                }),
            }
        });

    let module_item = choice((
        reg_decl.map(Field::Reg),
        mem_decl.map(Field::Mem),
        input_decl.map(Field::Input),
        output_item,
        always_block,
        initial_block,
        stage_def.map(Field::Stage),
        type_def.map(Field::Type),
        val_or_instance,
        fn_def.map(Field::Fn),
    ))
    .boxed();

    let module_items = just(Token::Semicolon)
        .repeated()
        .ignore_then(module_item)
        .separated_by(just(Token::Semicolon).repeated())
        .allow_trailing()
        .collect::<Vec<_>>();

    // ---- module / trait ----
    let module_def = just(Token::Module)
        .ignore_then(ident())
        .then(just(Token::Extends).ignore_then(ident()).or_not())
        .then(
            just(Token::With)
                .ignore_then(ident())
                .repeated()
                .collect::<Vec<_>>(),
        )
        .then(
            module_items
                .clone()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|(((name, extends), with_traits), items), e| ModuleDef {
            name,
            extends,
            with_traits,
            items,
            span: e.span(),
        });

    let trait_def = just(Token::Trait)
        .ignore_then(ident())
        .then(
            module_items
                .clone()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|(name, items), e| TraitDef {
            name,
            items,
            span: e.span(),
        });

    let item = choice((module_def.map(Item::Module), trait_def.map(Item::Trait)));

    let unit = just(Token::Semicolon)
        .repeated()
        .ignore_then(item)
        .separated_by(just(Token::Semicolon).repeated())
        .allow_trailing()
        .collect::<Vec<_>>()
        .map(|items| CompilationUnit { items });

    unit.then_ignore(just(Token::Semicolon).repeated())
        .then_ignore(end())
}

fn mk_bin(op: BinaryOp, l: Expr, r: Expr, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Binary(op, Box::new(l), Box::new(r)),
        span,
    }
}

enum Either2 {
    Instance(Ident),
    Expr(Expr),
}

/// 分岐部（if-then/else, match-case body）の解析．
/// ブロック・制御文・式を受ける．
fn then_or_else_branch<'a>(
    stmt: Recursive<chumsky::recursive::Indirect<'a, 'a, Toks<'a>, Stmt, Extra<'a>>>,
    expr: Recursive<chumsky::recursive::Indirect<'a, 'a, Toks<'a>, Expr, Extra<'a>>>,
    block: Recursive<chumsky::recursive::Indirect<'a, 'a, Toks<'a>, Block, Extra<'a>>>,
) -> impl Parser<'a, Toks<'a>, Expr, Extra<'a>> + Clone {
    let block_branch = block.map(|b: Block| {
        let span = b.span.clone();
        Expr {
            kind: ExprKind::Block(b),
            span,
        }
    });
    let stmt_branch = stmt_like_branch_or_expr(stmt, expr);
    choice((block_branch, stmt_branch))
}

fn stmt_like_branch_or_expr<'a>(
    stmt: Recursive<chumsky::recursive::Indirect<'a, 'a, Toks<'a>, Stmt, Extra<'a>>>,
    expr: Recursive<chumsky::recursive::Indirect<'a, 'a, Toks<'a>, Expr, Extra<'a>>>,
) -> impl Parser<'a, Toks<'a>, Expr, Extra<'a>> + Clone {
    // 制御文先頭のキーワードをチェックして stmt として解析．
    // それ以外は expr として解析．
    let stmt_keyword = select_ref! {
        Token::Par => (),
        Token::Seq => (),
        Token::Any => (),
        Token::Alt => (),
        Token::Generate => (),
        Token::Relay => (),
        Token::Finish => (),
        Token::Goto => (),
    };
    choice((
        stmt_keyword.rewind().ignore_then(stmt.map(|s: Stmt| {
            let span = s.span.clone();
            Expr {
                kind: ExprKind::Block(Block {
                    stmts: vec![s],
                    span: span.clone(),
                }),
                span,
            }
        })),
        expr,
    ))
}
