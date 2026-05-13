//! 名前解決
//!
//! AST を歩いて識別子参照 (`Expr_::Path` 等) を `Reference` として
//! `SymbolTable.references` に登録し，スコープ階層をもとに
//! 定義 ID またはビルトインに解決する．

use fsl_parser::*;

use crate::builtin::Builtins;
use crate::span::to_range;
use crate::symbols::{Reference, ResolvedTo, SymbolTable};

/// 名前解決のエントリポイント
/// 事前に `builder::build` で SymbolTable と scopes が構築されている必要がある
pub fn resolve_references(
    unit: &CompilationUnit,
    table: &mut SymbolTable,
    builtins: &Builtins,
) {
    for item in &unit.items {
        match &item.inner {
            Item::Module(m) => walk_module(table, builtins, m),
            Item::Trait(tr) => walk_trait(table, builtins, tr),
        }
    }
}

// ============================================================
// アイテム
// ============================================================

fn walk_module(table: &mut SymbolTable, b: &Builtins, m: &ModuleDef) {
    // extends / with の識別子はフェーズ3で解決するため references には登録しない
    for f in &m.items {
        walk_field(table, b, &f.inner);
    }
}

fn walk_trait(table: &mut SymbolTable, b: &Builtins, tr: &TraitDef) {
    for f in &tr.items {
        walk_field(table, b, &f.inner);
    }
}

// ============================================================
// フィールド
// ============================================================

fn walk_field(table: &mut SymbolTable, b: &Builtins, f: &Field) {
    match f {
        Field::Reg(r) => {
            if let Some(init) = &r.init {
                walk_expr(table, b, init);
            }
        }
        Field::Mem(m) => {
            walk_expr(table, b, &m.size);
            for e in &m.init {
                walk_expr(table, b, e);
            }
        }
        Field::Fn(f) => walk_block(table, b, &f.body),
        Field::Always(blk) | Field::Initial(blk) => walk_block(table, b, blk),
        Field::Stage(s) => {
            for item in &s.body {
                walk_stage_item(table, b, &item.inner);
            }
        }
        Field::Val(v) => walk_expr(table, b, &v.init),
        Field::Input(_)
        | Field::Output(_)
        | Field::OutputFn(_)
        | Field::Instance(_)
        | Field::Composite(_)
        | Field::Error => {}
    }
}

fn walk_stage_item(table: &mut SymbolTable, b: &Builtins, si: &StageItem) {
    match si {
        StageItem::State(s) => walk_stmt(table, b, &s.body),
        StageItem::Reg(r) => {
            if let Some(init) = &r.init {
                walk_expr(table, b, init);
            }
        }
        StageItem::Mem(m) => {
            walk_expr(table, b, &m.size);
            for e in &m.init {
                walk_expr(table, b, e);
            }
        }
        StageItem::Val(v) => walk_expr(table, b, &v.init),
        StageItem::Statement(s) => walk_stmt(table, b, s),
    }
}

// ============================================================
// 文
// ============================================================

fn walk_block(table: &mut SymbolTable, b: &Builtins, blk: &Block) {
    for s in &blk.stmts {
        walk_stmt(table, b, &s.inner);
    }
}

fn walk_stmt(table: &mut SymbolTable, b: &Builtins, stmt: &Statement) {
    match stmt {
        Statement::Val(v) => walk_expr(table, b, &v.init),
        Statement::RegAssign(lhs, rhs) | Statement::Assign(lhs, rhs) => {
            walk_expr(table, b, lhs);
            walk_expr(table, b, rhs);
        }
        Statement::BlockKind(_, blk) => walk_block(table, b, blk),
        Statement::Generate(name, args) | Statement::Relay(name, args) => {
            // 呼び出し対象 (stage 名) と引数を解決
            push_path_ref(table, b, name);
            for a in args {
                walk_expr(table, b, a);
            }
        }
        Statement::Goto(name) => push_path_ref(table, b, name),
        Statement::Expr(e) => walk_expr(table, b, e),
        Statement::Finish => {}
    }
}

// ============================================================
// 式
// ============================================================

fn walk_expr(table: &mut SymbolTable, b: &Builtins, e: &Expr) {
    match &e.inner {
        Expr_::Path(id) => push_path_ref(table, b, id),
        // フィールドアクセスの根のみ解決．フィールド名側はモジュール内部解決の対象
        Expr_::Field(root, _name) => walk_expr(table, b, root),
        Expr_::Call(callee, args) => {
            walk_expr(table, b, callee);
            for a in args {
                walk_expr(table, b, a);
            }
        }
        Expr_::Binary(_, l, r) => {
            walk_expr(table, b, l);
            walk_expr(table, b, r);
        }
        Expr_::Unary(_, x) => walk_expr(table, b, x),
        Expr_::Tuple(xs) => {
            for x in xs {
                walk_expr(table, b, x);
            }
        }
        Expr_::If(c, then_, else_) => {
            walk_expr(table, b, c);
            walk_expr(table, b, then_);
            if let Some(x) = else_ {
                walk_expr(table, b, x);
            }
        }
        Expr_::Match(scrut, arms) => {
            walk_expr(table, b, scrut);
            for arm in arms {
                walk_expr(table, b, &arm.body);
            }
        }
        Expr_::Block(blk) => walk_block(table, b, blk),
        // new <ModName> の解決はフェーズ3
        Expr_::New(_) => {}
        Expr_::Int(_) | Expr_::Str(_) | Expr_::Bool(_) | Expr_::Unit | Expr_::Error => {}
    }
}

// ============================================================
// 参照の登録
// ============================================================

fn push_path_ref(table: &mut SymbolTable, b: &Builtins, id: &Ident) {
    let span = to_range(id.span);
    let scope = table
        .scopes
        .scope_at_offset(span.start)
        .or_else(|| table.scopes.root())
        .expect("root scope must exist");

    let resolved = if let Some(name) = b.canonical(&id.inner) {
        ResolvedTo::Builtin(name)
    } else if let Some(def) = table.lookup_in(scope, &id.inner) {
        ResolvedTo::Def(def)
    } else {
        ResolvedTo::Unresolved
    };

    table.references.push(Reference {
        span,
        resolved,
        scope,
        name: id.inner.clone(),
    });
}
