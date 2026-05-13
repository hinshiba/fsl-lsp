//! シンボルテーブル
//!
//! スコープ階層・宣言レコード・参照集合を一括管理する．
//! LSP 機能 (Goto Definition / Hover / Completion) は本テーブルの検索 API を介する．

use crate::scope::{ScopeArena, ScopeId};
use crate::span::{contains_inclusive, Span};
use crate::symbol::{DefId, Symbol};

/// 識別子参照の解決結果
#[derive(Debug, Clone)]
pub enum ResolvedTo {
    Def(DefId),
    Builtin(&'static str),
    Unresolved,
}

/// 識別子参照
#[derive(Debug, Clone)]
pub struct Reference {
    pub span: Span,
    pub resolved: ResolvedTo,
    pub scope: ScopeId,
    pub name: String,
}

/// シンボルテーブル本体
#[derive(Debug, Default, Clone)]
pub struct SymbolTable {
    pub symbols: Vec<Symbol>,
    pub scopes: ScopeArena,
    pub references: Vec<Reference>,
}

impl SymbolTable {
    /// 新しいシンボルを登録し ID を返す
    /// `sym.id` は本関数内で確定値に書き換えられる
    pub fn push_symbol(&mut self, mut sym: Symbol) -> DefId {
        let id = DefId(self.symbols.len() as u32);
        sym.id = id;
        let scope = sym.scope;
        self.symbols.push(sym);
        self.scopes.get_mut(scope).defs.push(id);
        id
    }

    /// `id` から Symbol を取得
    pub fn def(&self, id: DefId) -> &Symbol {
        &self.symbols[id.0 as usize]
    }

    /// `scope` を起点に祖先方向へ名前解決する
    /// 同一スコープ内に同名複数あれば後から登録されたものを優先する
    pub fn lookup_in(&self, scope: ScopeId, name: &str) -> Option<DefId> {
        for s in self.scopes.ancestors(scope) {
            if let Some(id) = s
                .defs
                .iter()
                .rev()
                .copied()
                .find(|d| self.symbols[d.0 as usize].name == name)
            {
                return Some(id);
            }
        }
        None
    }

    /// `offset` から見える全シンボルを内側から外側の順で返す
    pub fn visible_at(&self, offset: usize) -> Vec<&Symbol> {
        let Some(scope) = self.scopes.scope_at_offset(offset) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for s in self.scopes.ancestors(scope) {
            for d in &s.defs {
                out.push(&self.symbols[d.0 as usize]);
            }
        }
        out
    }

    /// `offset` 上にある参照を返す
    pub fn ref_at(&self, offset: usize) -> Option<&Reference> {
        self.references
            .iter()
            .find(|r| contains_inclusive(&r.span, offset))
    }

    /// `offset` 上にある定義 (識別子トークン上) を返す
    pub fn def_at(&self, offset: usize) -> Option<&Symbol> {
        self.symbols
            .iter()
            .find(|s| contains_inclusive(&s.def_span, offset))
    }
}
