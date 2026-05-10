//! シンボル定義
//!
//! 名前解決の対象となる宣言の最小情報を保持する．
//! 全 `Symbol` は `SymbolTable.symbols` の連続領域に置かれ `DefId` で参照する．

use crate::scope::ScopeId;
use crate::span::Span;
use crate::ty::TypeInfo;

/// `SymbolTable.symbols` のインデックス
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefId(pub u32);

/// シンボル種別
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Module,
    Trait,
    Reg,
    Mem,
    Input,
    Output,
    OutputFn,
    Instance,
    Fn,
    Stage,
    State,
    Val,
    Param,
    Composite,
}

impl SymbolKind {
    /// hover 表示や Completion で用いるキーワード見出し
    pub fn label(self) -> &'static str {
        match self {
            SymbolKind::Module => "module",
            SymbolKind::Trait => "trait",
            SymbolKind::Reg => "reg",
            SymbolKind::Mem => "mem",
            SymbolKind::Input => "input",
            SymbolKind::Output => "output",
            SymbolKind::OutputFn => "output def",
            SymbolKind::Instance => "instance",
            SymbolKind::Fn => "def",
            SymbolKind::Stage => "stage",
            SymbolKind::State => "state",
            SymbolKind::Val => "val",
            SymbolKind::Param => "param",
            SymbolKind::Composite => "type",
        }
    }
}

/// 可変性
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutability {
    /// reg / mem は `:=` で更新可能
    Reg,
    /// output は `=` で更新可能
    Output,
    /// val / param / input / module / trait / fn / stage / state / instance / composite
    Immutable,
}

/// 単一宣言のレコード
#[derive(Debug, Clone)]
pub struct Symbol {
    pub id: DefId,
    pub name: String,
    pub kind: SymbolKind,
    /// 識別子トークンの span. Goto Definition のジャンプ先
    pub def_span: Span,
    /// 宣言全体の span. Document Symbols 用に予約
    pub full_span: Span,
    /// この宣言が属するスコープ
    pub scope: ScopeId,
    pub ty: Option<TypeInfo>,
    pub mutability: Mutability,
}
