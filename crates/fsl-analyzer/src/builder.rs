//! シンボル収集ビルダー
//!
//! AST を歩いて宣言を `SymbolTable` に登録する．
//! 参照解決は行わず，宣言の登録とスコープ階層の構築のみを担当する．

use chumsky::span::Spanned;
use fsl_parser::*;

use crate::scope::{ScopeId, ScopeKind};
use crate::span::{to_range, Span};
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
            push_named_symbol(t, scope, &r.name, SymbolKind::Reg, full, ty, Mutability::Reg);
        }
        Field::Mem(m) => {
            let elem = TypeInfo::from_ast(&m.elem_ty);
            let ty = Some(TypeInfo::Array(Box::new(elem)));
            push_named_symbol(t, scope, &m.name, SymbolKind::Mem, full, ty, Mutability::Reg);
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
            let ty = Some(TypeInfo::from_ast(&d.ret));
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
        Field::Instance(i) => {
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
            for stmt in &f.body.stmts {
                visit_stmt(t, fn_scope, &stmt.inner, to_range(stmt.span));
            }
        }
        Field::Always(b) => {
            let s = t.scopes.new_scope(Some(scope), ScopeKind::Always, full);
            for stmt in &b.stmts {
                visit_stmt(t, s, &stmt.inner, to_range(stmt.span));
            }
        }
        Field::Initial(b) => {
            let s = t.scopes.new_scope(Some(scope), ScopeKind::Initial, full);
            for stmt in &b.stmts {
                visit_stmt(t, s, &stmt.inner, to_range(stmt.span));
            }
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
            let s = t
                .scopes
                .new_scope(Some(scope), ScopeKind::State, full.clone());
            visit_stmt(t, s, &state.body, full);
        }
        StageItem::Reg(r) => {
            let ty = Some(TypeInfo::from_ast(&r.ty));
            push_named_symbol(t, scope, &r.name, SymbolKind::Reg, full, ty, Mutability::Reg);
        }
        StageItem::Mem(m) => {
            let elem = TypeInfo::from_ast(&m.elem_ty);
            let ty = Some(TypeInfo::Array(Box::new(elem)));
            push_named_symbol(t, scope, &m.name, SymbolKind::Mem, full, ty, Mutability::Reg);
        }
        StageItem::Val(v) => push_val(t, scope, v, full),
        StageItem::Statement(stmt) => visit_stmt(t, scope, stmt, full),
    }
}

// ============================================================
// 文と式 (子スコープと val 宣言のみを記録)
// ============================================================

fn visit_stmt(t: &mut SymbolTable, scope: ScopeId, stmt: &Statement, full: Span) {
    match stmt {
        Statement::Val(v) => push_val(t, scope, v, full),
        Statement::BlockKind(_, b) => visit_subblock(t, scope, b, full),
        Statement::Expr(e) => visit_expr_for_blocks(t, scope, e),
        Statement::RegAssign(_, rhs) | Statement::Assign(_, rhs) => {
            visit_expr_for_blocks(t, scope, rhs)
        }
        _ => {}
    }
}

fn visit_subblock(t: &mut SymbolTable, parent: ScopeId, b: &Block, span: Span) {
    let s = t.scopes.new_scope(Some(parent), ScopeKind::Block, span);
    for stmt in &b.stmts {
        visit_stmt(t, s, &stmt.inner, to_range(stmt.span));
    }
}

fn visit_expr_for_blocks(t: &mut SymbolTable, scope: ScopeId, e: &Expr) {
    let span = to_range(e.span);
    match &e.inner {
        Expr_::Block(b) => visit_subblock(t, scope, b, span),
        Expr_::If(c, then_, else_) => {
            visit_expr_for_blocks(t, scope, c);
            visit_expr_for_blocks(t, scope, then_);
            if let Some(x) = else_ {
                visit_expr_for_blocks(t, scope, x);
            }
        }
        Expr_::Match(scrut, arms) => {
            visit_expr_for_blocks(t, scope, scrut);
            for arm in arms {
                let s = t
                    .scopes
                    .new_scope(Some(scope), ScopeKind::Match, arm.span.clone());
                visit_expr_for_blocks(t, s, &arm.body);
            }
        }
        Expr_::Binary(_, l, r) => {
            visit_expr_for_blocks(t, scope, l);
            visit_expr_for_blocks(t, scope, r);
        }
        Expr_::Unary(_, x) => visit_expr_for_blocks(t, scope, x),
        Expr_::Tuple(xs) => {
            for x in xs {
                visit_expr_for_blocks(t, scope, x);
            }
        }
        Expr_::Call(c, args) => {
            visit_expr_for_blocks(t, scope, c);
            for a in args {
                visit_expr_for_blocks(t, scope, a);
            }
        }
        Expr_::Field(root, _) => visit_expr_for_blocks(t, scope, root),
        // 葉ノードはスコープを増やさない
        Expr_::Path(_)
        | Expr_::Int(_)
        | Expr_::Str(_)
        | Expr_::Bool(_)
        | Expr_::Unit
        | Expr_::New(_)
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
    visit_expr_for_blocks(t, scope, &v.init);
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
