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
        Field::Fn(f) => walk_expr(ctx, diags, &f.body),
        Field::Always(b) | Field::Initial(b) => walk_expr(ctx, diags, b),
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
        StageItem::State(st) => walk_expr(ctx, diags, &st.body),
        StageItem::Expr(e) => walk_expr(ctx, diags, e),
        StageItem::Reg(_) => {}
    }
}

/// 式を歩いて代入演算子と左辺シンボル種別の整合を検査する
///
/// `Block` 廃止により旧 `walk_block` / `walk_stmt` を統合した単一走査．
fn walk_expr(ctx: &AnalysisContext, diags: &mut Vec<Diagnostic>, e: &Expr) {
    match &e.inner {
        // `=` 出力ポート割当  /  `:=` reg/mem 更新
        Expr_::PortAssign(lhs, rhs) => {
            check_eq_assign(ctx, diags, lhs);
            walk_expr(ctx, diags, rhs);
        }
        Expr_::MemAssign(lhs, rhs) => {
            check_reg_assign(ctx, diags, lhs);
            walk_expr(ctx, diags, rhs);
        }
        Expr_::If(c, t, el) => {
            walk_expr(ctx, diags, c);
            walk_expr(ctx, diags, t);
            if let Some(x) = el {
                walk_expr(ctx, diags, x);
            }
        }
        Expr_::Match(s, arms) => {
            walk_expr(ctx, diags, s);
            for a in arms {
                walk_expr(ctx, diags, &a.inner.body);
            }
        }
        Expr_::Any(cases, el) | Expr_::Alt(cases, el) => {
            for c in cases {
                walk_expr(ctx, diags, &c.inner.cond);
                walk_expr(ctx, diags, &c.inner.body);
            }
            if let Some(x) = el {
                walk_expr(ctx, diags, x);
            }
        }
        Expr_::Binary(_, l, r) => {
            walk_expr(ctx, diags, l);
            walk_expr(ctx, diags, r);
        }
        Expr_::Unary(_, x) => walk_expr(ctx, diags, x),
        Expr_::Tuple(xs) | Expr_::Block(xs) | Expr_::Seq(xs) | Expr_::Par(xs) => {
            for x in xs {
                walk_expr(ctx, diags, x);
            }
        }
        Expr_::Call(c, args) => {
            walk_expr(ctx, diags, c);
            for a in args {
                walk_expr(ctx, diags, a);
            }
        }
        Expr_::Generate(_, args) | Expr_::Relay(_, args) => {
            for a in args {
                walk_expr(ctx, diags, a);
            }
        }
        Expr_::ValDecl(v) => walk_expr(ctx, diags, &v.init),
        Expr_::Field(r, _) => walk_expr(ctx, diags, r),
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
        Expr_::Variable(id) => Some(id),
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
