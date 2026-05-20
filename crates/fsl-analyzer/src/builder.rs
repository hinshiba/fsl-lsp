//! シンボル収集ビルダー
//!
//! AST を歩いて宣言を `SymbolTable` に登録する．
//! 参照解決は行わず，宣言の登録とスコープ階層の構築のみを担当する．

use chumsky::span::Spanned;
use fsl_parser::*;

use crate::scope::{ScopeId, ScopeKind};
use crate::span::{Span, to_range};
use crate::symbol::{DefId, Mutability, Symbol, SymbolKind};
use crate::symbols::SymbolTable;
use crate::ty::TypeInfo;

/// `SymbolTable` 構築のエントリポイント
pub fn build(unit: &CompilationUnit) -> SymbolTable {
    let mut t = SymbolTable::default();
    let root = t.scopes.new_scope(None, ScopeKind::Root, 0..usize::MAX);

    for item in &unit.items {
        let full = to_range(item.span);
        match &item.inner {
            Item::Module(m) => visit_module(&mut t, root, m, full),
            Item::Trait(tr) => visit_trait(&mut t, root, tr, full),
        }
    }
    t
}

// ============================================================
// アイテム
// ============================================================

fn visit_module(t: &mut SymbolTable, parent: ScopeId, m: &ModuleDef, full: Span) {
    push_named_symbol(
        t,
        parent,
        &m.name,
        SymbolKind::Module,
        full.clone(),
        None,
        Mutability::Immutable,
    );

    let scope = t.scopes.new_scope(Some(parent), ScopeKind::Module, full);
    for f in &m.items {
        visit_field(t, scope, &f.inner, to_range(f.span));
    }
}

fn visit_trait(t: &mut SymbolTable, parent: ScopeId, tr: &TraitDef, full: Span) {
    push_named_symbol(
        t,
        parent,
        &tr.name,
        SymbolKind::Trait,
        full.clone(),
        None,
        Mutability::Immutable,
    );

    let scope = t.scopes.new_scope(Some(parent), ScopeKind::Trait, full);
    for f in &tr.items {
        visit_field(t, scope, &f.inner, to_range(f.span));
    }
}

// ============================================================
// フィールド
// ============================================================

fn visit_field(t: &mut SymbolTable, scope: ScopeId, f: &Field, full: Span) {
    match f {
        Field::Reg(r) => {
            let ty = Some(TypeInfo::from_ast(&r.ty));
            push_named_symbol(
                t,
                scope,
                &r.name,
                SymbolKind::Reg,
                full,
                ty,
                Mutability::Reg,
            );
        }
        Field::Mem(m) => {
            let elem = TypeInfo::from_ast(&m.elem_ty);
            let ty = Some(TypeInfo::Array(Box::new(elem)));
            push_named_symbol(
                t,
                scope,
                &m.name,
                SymbolKind::Mem,
                full,
                ty,
                Mutability::Reg,
            );
        }
        Field::Input(p) => {
            let ty = Some(TypeInfo::from_ast(&p.ty));
            push_named_symbol(
                t,
                scope,
                &p.name,
                SymbolKind::Input,
                full,
                ty,
                Mutability::Immutable,
            );
        }
        Field::Output(p) => {
            let ty = Some(TypeInfo::from_ast(&p.ty));
            push_named_symbol(
                t,
                scope,
                &p.name,
                SymbolKind::Output,
                full,
                ty,
                Mutability::Output,
            );
        }
        Field::OutputFn(d) => {
            let ty = d.ret.as_ref().map(TypeInfo::from_ast);
            push_named_symbol(
                t,
                scope,
                &d.name,
                SymbolKind::OutputFn,
                full.clone(),
                ty,
                Mutability::Immutable,
            );
            // 関数スコープに params を登録 (本体は無し)
            let fn_scope = t.scopes.new_scope(Some(scope), ScopeKind::Function, full);
            push_params(t, fn_scope, &d.params);
        }
        Field::NewInstance(i) => {
            let ty = Some(TypeInfo::Named(i.module_name.inner.clone()));
            push_named_symbol(
                t,
                scope,
                &i.name,
                SymbolKind::Instance,
                full,
                ty,
                Mutability::Immutable,
            );
        }
        Field::Fn(f) => {
            let ty = f.ret.as_ref().map(TypeInfo::from_ast);
            push_named_symbol(
                t,
                scope,
                &f.name,
                SymbolKind::Fn,
                full.clone(),
                ty,
                Mutability::Immutable,
            );
            // 関数スコープを開いて params + body を歩く
            let fn_scope = t
                .scopes
                .new_scope(Some(scope), ScopeKind::Function, full.clone());
            push_params(t, fn_scope, &f.params);
            visit_body(t, fn_scope, &f.body);
        }
        Field::Always(b) => {
            let s = t.scopes.new_scope(Some(scope), ScopeKind::Always, full);
            visit_body(t, s, b);
        }
        Field::Initial(b) => {
            let s = t.scopes.new_scope(Some(scope), ScopeKind::Initial, full);
            visit_body(t, s, b);
        }
        Field::Stage(stage) => {
            push_named_symbol(
                t,
                scope,
                &stage.name,
                SymbolKind::Stage,
                full.clone(),
                None,
                Mutability::Immutable,
            );
            let stage_scope = t.scopes.new_scope(Some(scope), ScopeKind::Stage, full);
            push_params(t, stage_scope, &stage.params);
            for item in &stage.body {
                visit_stage_item(t, stage_scope, &item.inner, to_range(item.span));
            }
        }
        Field::Composite(c) => {
            push_named_symbol(
                t,
                scope,
                &c.name,
                SymbolKind::Composite,
                full,
                None,
                Mutability::Immutable,
            );
        }
        Field::Val(v) => push_val(t, scope, v, full),
        Field::Error => {}
    }
}

// ============================================================
// stage アイテム
// ============================================================

fn visit_stage_item(t: &mut SymbolTable, scope: ScopeId, si: &StageItem, full: Span) {
    match si {
        StageItem::State(state) => {
            push_named_symbol(
                t,
                scope,
                &state.name,
                SymbolKind::State,
                full.clone(),
                None,
                Mutability::Immutable,
            );
            let s = t.scopes.new_scope(Some(scope), ScopeKind::State, full);
            visit_body(t, s, &state.body);
        }
        StageItem::Reg(r) => {
            let ty = Some(TypeInfo::from_ast(&r.ty));
            push_named_symbol(
                t,
                scope,
                &r.name,
                SymbolKind::Reg,
                full,
                ty,
                Mutability::Reg,
            );
        }
        // val 宣言・代入・制御フロー等はすべて式
        StageItem::Expr(e) => visit_expr(t, scope, e),
    }
}

// ============================================================
// 式 (子スコープと val 宣言のみを記録)
// ============================================================

/// fn / always / initial / state の本体を解析する
///
/// 本体直下のブロックは新たなスコープを開かず，渡された `scope` に
/// 直接 val 宣言を登録する  params と本体宣言を同一スコープに置くため
fn visit_body(t: &mut SymbolTable, scope: ScopeId, body: &Expr) {
    match &body.inner {
        Expr_::Block(es) | Expr_::Seq(es) | Expr_::Par(es) => {
            for e in es {
                visit_expr(t, scope, e);
            }
        }
        _ => visit_expr(t, scope, body),
    }
}

/// 式を歩いて子スコープと val 宣言を記録する
fn visit_expr(t: &mut SymbolTable, scope: ScopeId, e: &Expr) {
    let span = to_range(e.span);
    match &e.inner {
        // val 宣言をスコープに登録
        Expr_::ValDecl(v) => push_val(t, scope, v, span),
        // ブロック類は新たな子スコープを開く
        Expr_::Block(es) | Expr_::Seq(es) | Expr_::Par(es) => {
            let s = t.scopes.new_scope(Some(scope), ScopeKind::Block, span);
            for x in es {
                visit_expr(t, s, x);
            }
        }
        Expr_::Any(cases, else_) | Expr_::Alt(cases, else_) => {
            let s = t.scopes.new_scope(Some(scope), ScopeKind::Block, span);
            for c in cases {
                visit_expr(t, s, &c.inner.cond);
                visit_expr(t, s, &c.inner.body);
            }
            if let Some(x) = else_ {
                visit_expr(t, s, x);
            }
        }
        Expr_::If(c, then_, else_) => {
            visit_expr(t, scope, c);
            visit_expr(t, scope, then_);
            if let Some(x) = else_ {
                visit_expr(t, scope, x);
            }
        }
        Expr_::Match(scrut, arms) => {
            visit_expr(t, scope, scrut);
            for arm in arms {
                let s = t
                    .scopes
                    .new_scope(Some(scope), ScopeKind::Match, to_range(arm.span));
                visit_expr(t, s, &arm.inner.body);
            }
        }
        Expr_::Binary(_, l, r) | Expr_::PortAssign(l, r) | Expr_::MemAssign(l, r) => {
            visit_expr(t, scope, l);
            visit_expr(t, scope, r);
        }
        Expr_::Unary(_, x) => visit_expr(t, scope, x),
        Expr_::Tuple(xs) => {
            for x in xs {
                visit_expr(t, scope, x);
            }
        }
        Expr_::Call(c, args) => {
            visit_expr(t, scope, c);
            for a in args {
                visit_expr(t, scope, a);
            }
        }
        Expr_::Generate(_, args) | Expr_::Relay(_, args) => {
            for a in args {
                visit_expr(t, scope, a);
            }
        }
        Expr_::Field(root, _) => visit_expr(t, scope, root),
        // 葉ノードはスコープを増やさない
        Expr_::Variable(_)
        | Expr_::IntLit(_)
        | Expr_::BitLit(_)
        | Expr_::StringLit(_)
        | Expr_::Bool(_)
        | Expr_::Unit
        | Expr_::New(_)
        | Expr_::Goto(_)
        | Expr_::Finish
        | Expr_::Error => {}
    }
}

// ============================================================
// val・params の登録
// ============================================================

fn push_val(t: &mut SymbolTable, scope: ScopeId, v: &ValDecl, full: Span) {
    let ty = v.ty.as_ref().map(TypeInfo::from_ast);
    match &v.pattern {
        ValLhs::Single(name) => {
            push_named_symbol(
                t,
                scope,
                name,
                SymbolKind::Val,
                full,
                ty,
                Mutability::Immutable,
            );
        }
        ValLhs::Tuple(names) => {
            // タプル分配は要素毎に独立した Val として登録．要素型は未推論
            for name in names {
                push_named_symbol(
                    t,
                    scope,
                    name,
                    SymbolKind::Val,
                    full.clone(),
                    None,
                    Mutability::Immutable,
                );
            }
        }
    }
    // 初期化式内のブロックも歩く
    visit_expr(t, scope, &v.init);
}

fn push_params(t: &mut SymbolTable, scope: ScopeId, params: &[Spanned<Param>]) {
    for p in params {
        let ty = p.inner.ty.as_ref().map(TypeInfo::from_ast);
        push_named_symbol(
            t,
            scope,
            &p.inner.name,
            SymbolKind::Param,
            to_range(p.span),
            ty,
            Mutability::Immutable,
        );
    }
}

fn push_named_symbol(
    t: &mut SymbolTable,
    scope: ScopeId,
    name: &Ident,
    kind: SymbolKind,
    full: Span,
    ty: Option<TypeInfo>,
    mutability: Mutability,
) -> DefId {
    t.push_symbol(Symbol {
        id: DefId(0),
        name: name.inner.clone(),
        kind,
        def_span: to_range(name.span),
        full_span: full,
        scope,
        ty,
        mutability,
    })
}
