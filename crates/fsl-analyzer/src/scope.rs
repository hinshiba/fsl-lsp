//! スコープ階層
//!
//! モジュール → 関数/always/initial/stage → ブロック の階層をアリーナ方式で保持する．
//! 各スコープは span を持つため，offset から最深スコープを検索できる．

use crate::span::{contains, Span};
use crate::symbol::DefId;

/// `ScopeArena.scopes` のインデックス
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub u32);

/// スコープ種別
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Root,
    Module,
    Trait,
    Stage,
    State,
    Function,
    Always,
    Initial,
    Block,
    Match,
}

/// 単一スコープ
#[derive(Debug, Clone)]
pub struct Scope {
    pub id: ScopeId,
    pub parent: Option<ScopeId>,
    pub kind: ScopeKind,
    pub span: Span,
    /// このスコープに直接登録された宣言
    pub defs: Vec<DefId>,
    pub children: Vec<ScopeId>,
}

/// スコープ群のアリーナ
#[derive(Debug, Default, Clone)]
pub struct ScopeArena {
    scopes: Vec<Scope>,
}

impl ScopeArena {
    /// 新しいスコープを作成して ID を返す
    /// 親があれば親の `children` にも登録する
    pub fn new_scope(
        &mut self,
        parent: Option<ScopeId>,
        kind: ScopeKind,
        span: Span,
    ) -> ScopeId {
        let id = ScopeId(self.scopes.len() as u32);
        self.scopes.push(Scope {
            id,
            parent,
            kind,
            span,
            defs: Vec::new(),
            children: Vec::new(),
        });
        if let Some(p) = parent {
            self.scopes[p.0 as usize].children.push(id);
        }
        id
    }

    /// 既存スコープの参照
    pub fn get(&self, id: ScopeId) -> &Scope {
        &self.scopes[id.0 as usize]
    }

    /// 既存スコープの可変参照
    pub fn get_mut(&mut self, id: ScopeId) -> &mut Scope {
        &mut self.scopes[id.0 as usize]
    }

    /// 全スコープのイテレータ
    pub fn iter(&self) -> impl Iterator<Item = &Scope> {
        self.scopes.iter()
    }

    /// ルートスコープ ID．アリーナが空でなければ `ScopeId(0)`
    pub fn root(&self) -> Option<ScopeId> {
        if self.scopes.is_empty() {
            None
        } else {
            Some(ScopeId(0))
        }
    }

    /// `offset` を内包する最深スコープを返す
    pub fn scope_at_offset(&self, offset: usize) -> Option<ScopeId> {
        let root = self.root()?;
        let mut current = root;
        // 子に降りられる限り降りる
        loop {
            let next = self.scopes[current.0 as usize]
                .children
                .iter()
                .copied()
                .find(|c| contains(&self.scopes[c.0 as usize].span, offset));
            match next {
                Some(c) => current = c,
                None => return Some(current),
            }
        }
    }

    /// `scope` から root まで遡るイテレータ
    pub fn ancestors(&self, scope: ScopeId) -> Ancestors<'_> {
        Ancestors {
            arena: self,
            current: Some(scope),
        }
    }
}

/// `ScopeArena::ancestors` の戻り値
pub struct Ancestors<'a> {
    arena: &'a ScopeArena,
    current: Option<ScopeId>,
}

impl<'a> Iterator for Ancestors<'a> {
    type Item = &'a Scope;
    fn next(&mut self) -> Option<Self::Item> {
        let id = self.current?;
        let s = self.arena.get(id);
        self.current = s.parent;
        Some(s)
    }
}
