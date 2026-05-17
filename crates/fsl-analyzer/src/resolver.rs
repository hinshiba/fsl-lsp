//! 名前解決
//!
//! AST を歩いて識別子参照 (`Expr_::Variable` 等) を `Reference` として
//! `SymbolTable.references` に登録し，スコープ階層をもとに
//! 定義 ID・ビルトイン・継承メンバのいずれかに解決する．

use std::collections::HashSet;

use fsl_parser::*;

use crate::builtin::Builtins;
use crate::index::{module_bases, ModuleIndex};
use crate::span::to_range;
use crate::symbols::{Reference, ResolvedTo, SymbolTable};

/// 名前解決のエントリポイント
/// 事前に `builder::build` で SymbolTable と scopes が構築されている必要がある
pub fn resolve_references(
    unit: &CompilationUnit,
    table: &mut SymbolTable,
    builtins: &Builtins,
    index: &ModuleIndex,
) {
    for item in &unit.items {
        match &item.inner {
            Item::Module(m) => walk_module(table, builtins, index, m),
            Item::Trait(tr) => walk_trait(table, builtins, tr),
        }
    }
}

// ============================================================
// アイテム
// ============================================================

fn walk_module(table: &mut SymbolTable, b: &Builtins, index: &ModuleIndex, m: &ModuleDef) {
    // extends / with の継承メンバ名を集め，未宣言判定から除外する
    let inherited: HashSet<String> = index
        .members_of_bases(&module_bases(m))
        .into_iter()
        .map(|mem| mem.name)
        .collect();

    for f in &m.items {
        walk_field(table, b, &inherited, &f.inner);
    }
}

fn walk_trait(table: &mut SymbolTable, b: &Builtins, tr: &TraitDef) {
    // trait は継承を持たないため継承メンバは空
    let inherited = HashSet::new();
    for f in &tr.items {
        walk_field(table, b, &inherited, &f.inner);
    }
}

// ============================================================
// フィールド
// ============================================================

fn walk_field(table: &mut SymbolTable, b: &Builtins, inh: &HashSet<String>, f: &Field) {
    match f {
        Field::Reg(r) => {
            if let Some(init) = &r.init {
                walk_expr(table, b, inh, init);
            }
        }
        Field::Mem(m) => {
            walk_expr(table, b, inh, &m.size);
            for e in &m.init {
                walk_expr(table, b, inh, e);
            }
        }
        Field::Fn(f) => walk_expr(table, b, inh, &f.body),
        Field::Always(blk) | Field::Initial(blk) => walk_expr(table, b, inh, blk),
        Field::Stage(s) => {
            for item in &s.body {
                walk_stage_item(table, b, inh, &item.inner);
            }
        }
        Field::Val(v) => walk_expr(table, b, inh, &v.init),
        Field::Input(_)
        | Field::Output(_)
        | Field::OutputFn(_)
        | Field::NewInstance(_)
        | Field::Composite(_)
        | Field::Error => {}
    }
}

fn walk_stage_item(table: &mut SymbolTable, b: &Builtins, inh: &HashSet<String>, si: &StageItem) {
    match si {
        StageItem::State(s) => walk_expr(table, b, inh, &s.body),
        StageItem::Reg(r) => {
            if let Some(init) = &r.init {
                walk_expr(table, b, inh, init);
            }
        }
        StageItem::Expr(e) => walk_expr(table, b, inh, e),
    }
}

// ============================================================
// 式
// ============================================================

/// 式を歩いて識別子参照を登録する
fn walk_expr(table: &mut SymbolTable, b: &Builtins, inh: &HashSet<String>, e: &Expr) {
    match &e.inner {
        Expr_::Variable(id) => push_path_ref(table, b, inh, id),
        // フィールドアクセスの根のみ解決．フィールド名側はモジュール内部解決の対象
        Expr_::Field(root, _name) => walk_expr(table, b, inh, root),
        Expr_::Call(callee, args) => {
            walk_expr(table, b, inh, callee);
            for a in args {
                walk_expr(table, b, inh, a);
            }
        }
        Expr_::Binary(_, l, r) | Expr_::PortAssign(l, r) | Expr_::MemAssign(l, r) => {
            walk_expr(table, b, inh, l);
            walk_expr(table, b, inh, r);
        }
        Expr_::Unary(_, x) => walk_expr(table, b, inh, x),
        Expr_::Tuple(xs) | Expr_::Block(xs) | Expr_::Seq(xs) | Expr_::Par(xs) => {
            for x in xs {
                walk_expr(table, b, inh, x);
            }
        }
        Expr_::If(c, then_, else_) => {
            walk_expr(table, b, inh, c);
            walk_expr(table, b, inh, then_);
            if let Some(x) = else_ {
                walk_expr(table, b, inh, x);
            }
        }
        Expr_::Match(scrut, arms) => {
            walk_expr(table, b, inh, scrut);
            for arm in arms {
                walk_expr(table, b, inh, &arm.inner.body);
            }
        }
        Expr_::Any(cases, else_) | Expr_::Alt(cases, else_) => {
            for c in cases {
                walk_expr(table, b, inh, &c.inner.cond);
                walk_expr(table, b, inh, &c.inner.body);
            }
            if let Some(x) = else_ {
                walk_expr(table, b, inh, x);
            }
        }
        Expr_::ValDecl(v) => walk_expr(table, b, inh, &v.init),
        // generate / relay は呼び出し対象 (stage 名) と引数を解決
        Expr_::Generate(name, args) | Expr_::Relay(name, args) => {
            push_path_ref(table, b, inh, name);
            for a in args {
                walk_expr(table, b, inh, a);
            }
        }
        Expr_::Goto(name) => push_path_ref(table, b, inh, name),
        // new <ModName> の解決はフェーズ3
        Expr_::New(_) => {}
        Expr_::Finish
        | Expr_::IntLit(_)
        | Expr_::BitLit(_)
        | Expr_::StringLit(_)
        | Expr_::Bool(_)
        | Expr_::Unit
        | Expr_::Error => {}
    }
}

// ============================================================
// 参照の登録
// ============================================================

fn push_path_ref(table: &mut SymbolTable, b: &Builtins, inh: &HashSet<String>, id: &Ident) {
    let span = to_range(id.span);
    let scope = table
        .scopes
        .scope_at_offset(span.start)
        .or_else(|| table.scopes.root())
        .expect("root scope must exist");

    // ビルトイン → ローカル定義 → 継承メンバ → プレリュード → 未解決 の順で解決する
    let resolved = if let Some(name) = b.canonical(&id.inner) {
        ResolvedTo::Builtin(name)
    } else if let Some(def) = table.lookup_in(scope, &id.inner) {
        ResolvedTo::Def(def)
    } else if inh.contains(&id.inner) {
        ResolvedTo::External
    } else if crate::index::prelude().get(&id.inner).is_some() {
        // 暗黙プレリュード (BitN.fsl) のモジュール  例: `Bit(n)`
        ResolvedTo::External
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
