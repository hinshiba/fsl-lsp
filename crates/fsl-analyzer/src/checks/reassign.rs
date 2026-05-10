//! 変数再代入検査
//!
//! `=` (出力ポート割当) と `:=` (reg/mem 更新) について
//! 左辺シンボル種別と演算子の整合性を確認する．

use fsl_parser::*;

use super::Check;
use crate::context::AnalysisContext;
use crate::span::to_range;
use crate::symbol::{Symbol, SymbolKind};
use crate::symbols::ResolvedTo;
use crate::{Diagnostic, Severity};

pub struct ReassignCheck;

impl Check for ReassignCheck {
    fn name(&self) -> &'static str {
        "reassign"
    }

    fn run(&self, ctx: &AnalysisContext, diags: &mut Vec<Diagnostic>) {
        for item in &ctx.unit.items {
            if let Item::Module(m) = &item.inner {
                for f in &m.items {
                    walk_field(ctx, diags, &f.inner);
                }
            }
        }
    }
}

// ============================================================
// 走査
// ============================================================

fn walk_field(ctx: &AnalysisContext, diags: &mut Vec<Diagnostic>, f: &Field) {
    match f {
        Field::Fn(f) => walk_block(ctx, diags, &f.body),
        Field::Always(b) | Field::Initial(b) => walk_block(ctx, diags, b),
        Field::Stage(s) => {
            for it in &s.body {
                walk_stage_item(ctx, diags, &it.inner);
            }
        }
        _ => {}
    }
}

fn walk_stage_item(ctx: &AnalysisContext, diags: &mut Vec<Diagnostic>, si: &StageItem) {
    match si {
        StageItem::State(st) => walk_stmt(ctx, diags, &st.body),
        StageItem::Statement(s) => walk_stmt(ctx, diags, s),
        _ => {}
    }
}

fn walk_block(ctx: &AnalysisContext, diags: &mut Vec<Diagnostic>, b: &Block) {
    for s in &b.stmts {
        walk_stmt(ctx, diags, &s.inner);
    }
}

fn walk_stmt(ctx: &AnalysisContext, diags: &mut Vec<Diagnostic>, stmt: &Statement) {
    match stmt {
        Statement::Assign(lhs, _) => check_eq_assign(ctx, diags, lhs),
        Statement::RegAssign(lhs, _) => check_reg_assign(ctx, diags, lhs),
        Statement::BlockKind(_, b) => walk_block(ctx, diags, b),
        Statement::Expr(e) => walk_blocks_in_expr(ctx, diags, e),
        _ => {}
    }
}

fn walk_blocks_in_expr(ctx: &AnalysisContext, diags: &mut Vec<Diagnostic>, e: &Expr) {
    match &e.inner {
        Expr_::Block(b) => walk_block(ctx, diags, b),
        Expr_::If(c, t, el) => {
            walk_blocks_in_expr(ctx, diags, c);
            walk_blocks_in_expr(ctx, diags, t);
            if let Some(x) = el {
                walk_blocks_in_expr(ctx, diags, x);
            }
        }
        Expr_::Match(s, arms) => {
            walk_blocks_in_expr(ctx, diags, s);
            for a in arms {
                walk_blocks_in_expr(ctx, diags, &a.body);
            }
        }
        Expr_::Binary(_, l, r) => {
            walk_blocks_in_expr(ctx, diags, l);
            walk_blocks_in_expr(ctx, diags, r);
        }
        Expr_::Unary(_, x) => walk_blocks_in_expr(ctx, diags, x),
        Expr_::Tuple(xs) => {
            for x in xs {
                walk_blocks_in_expr(ctx, diags, x);
            }
        }
        Expr_::Call(c, args) => {
            walk_blocks_in_expr(ctx, diags, c);
            for a in args {
                walk_blocks_in_expr(ctx, diags, a);
            }
        }
        Expr_::Field(r, _) => walk_blocks_in_expr(ctx, diags, r),
        _ => {}
    }
}

// ============================================================
// 個別検査
// ============================================================

fn check_eq_assign(ctx: &AnalysisContext, diags: &mut Vec<Diagnostic>, lhs: &Expr) {
    let Some(sym) = root_symbol_of(ctx, lhs) else {
        return;
    };
    match sym.kind {
        SymbolKind::Output => {}
        SymbolKind::Reg | SymbolKind::Mem => diags.push(Diagnostic {
            severity: Severity::Error,
            message: format!(
                "`{}` は {} であるため再代入には `:=` を使ってください",
                sym.name,
                sym.kind.label()
            ),
            span: to_range(lhs.span),
        }),
        SymbolKind::Val | SymbolKind::Param | SymbolKind::Input => diags.push(Diagnostic {
            severity: Severity::Error,
            message: format!("`{}` ({}) は再代入できません", sym.name, sym.kind.label()),
            span: to_range(lhs.span),
        }),
        _ => {}
    }
}

fn check_reg_assign(ctx: &AnalysisContext, diags: &mut Vec<Diagnostic>, lhs: &Expr) {
    let Some(sym) = root_symbol_of(ctx, lhs) else {
        return;
    };
    if !matches!(sym.kind, SymbolKind::Reg | SymbolKind::Mem) {
        diags.push(Diagnostic {
            severity: Severity::Error,
            message: format!(
                "`{}` ({}) は reg/mem ではないため `:=` で更新できません",
                sym.name,
                sym.kind.label()
            ),
            span: to_range(lhs.span),
        });
    }
}

// ============================================================
// LHS の root シンボル
// ============================================================

fn root_ident(e: &Expr) -> Option<&Ident> {
    match &e.inner {
        Expr_::Path(id) => Some(id),
        Expr_::Field(root, _) => root_ident(root),
        Expr_::Call(callee, _) => root_ident(callee),
        _ => None,
    }
}

fn root_symbol_of<'a>(ctx: &'a AnalysisContext, lhs: &Expr) -> Option<&'a Symbol> {
    let id = root_ident(lhs)?;
    let span = to_range(id.span);
    let r = ctx.references.iter().find(|r| r.span == span)?;
    match r.resolved {
        ResolvedTo::Def(d) => Some(ctx.symbols.def(d)),
        _ => None,
    }
}
