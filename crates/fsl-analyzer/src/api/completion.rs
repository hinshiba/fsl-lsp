//! Completion
//!
//! 指定 offset から見えるシンボル群と，キーワード / 組込み型 / ビルトイン関数の
//! スタティックリストを返す通常補完に加え，`extends` 継承メンバの補完と，
//! `instance.member` 形式のメンバ補完を提供する．

use fsl_parser::{Item, ModuleDef};

use crate::index::{module_bases, Member, ModuleIndex};
use crate::span::contains;
use crate::symbol::Symbol;
use crate::ty::TypeInfo;
use crate::AnalysisResult;

/// FSL のキーワード一覧
pub const KEYWORDS: &[&str] = &[
    "module", "trait", "def", "val", "reg", "mem", "input", "output", "always", "initial",
    "stage", "state", "par", "seq", "any", "alt", "if", "else", "match", "case", "generate",
    "relay", "finish", "goto", "new", "extends", "with", "true", "false", "private", "type",
];

/// 組込み型名
pub const BUILTIN_TYPES: &[&str] = &["Bit", "Boolean", "Int", "Unit", "String", "Array", "List"];

/// completion 候補のまとまり
pub struct CompletionList<'a> {
    pub symbols: Vec<&'a Symbol>,
    /// `extends` / `with` 継承により取り込まれるメンバ
    pub inherited: Vec<Member>,
    pub keywords: &'static [&'static str],
    pub builtin_types: &'static [&'static str],
    pub builtins: &'static [&'static str],
}

/// `offset` における通常 completion 候補を返す
///
/// スコープ内シンボルに加え，`offset` を含むモジュールの継承メンバを併せて返す．
pub fn completions_at<'a>(
    result: &'a AnalysisResult,
    index: &ModuleIndex,
    offset: usize,
) -> CompletionList<'a> {
    let inherited = enclosing_module(result, offset)
        .map(|m| index.members_of_bases(&module_bases(m)))
        .unwrap_or_default();

    CompletionList {
        symbols: result.symbols.visible_at(offset),
        inherited,
        keywords: KEYWORDS,
        builtin_types: BUILTIN_TYPES,
        builtins: crate::builtin::builtins().all(),
    }
}

/// `receiver` のメンバ補完候補を返す
///
/// 解決経路は二通り．
/// `val r = new Mod` で宣言されたインスタンスは型 `Named(Mod)` を辿る．
/// `Bit(n)` のようにモジュール名を直接書いた場合はその名前で索引を引く．
/// いずれにも該当しなければ `None` を返す．
/// `offset` は `receiver` 識別子上の任意位置．
pub fn member_completions(
    result: &AnalysisResult,
    index: &ModuleIndex,
    offset: usize,
    receiver: &str,
) -> Option<Vec<Member>> {
    // インスタンス経由  receiver の型 `Named(<module>)` からメンバを引く
    let scope = result
        .symbols
        .scopes
        .scope_at_offset(offset)
        .or_else(|| result.symbols.scopes.root());
    if let Some(scope) = scope {
        if let Some(def) = result.symbols.lookup_in(scope, receiver) {
            if let Some(TypeInfo::Named(module)) = &result.symbols.def(def).ty {
                return lookup_members(index, module);
            }
        }
    }

    // モジュール名直接参照  `Bit(n).zero` 等のプレリュード型を含む
    lookup_members(index, receiver)
}

/// ワークスペース索引とプレリュードからモジュールのメンバ集合を引く
fn lookup_members(index: &ModuleIndex, name: &str) -> Option<Vec<Member>> {
    if index.get(name).is_some() {
        return Some(index.resolved_members(name));
    }
    let prelude = crate::index::prelude();
    prelude.get(name).map(|_| prelude.resolved_members(name))
}

/// `offset` を本体に含むモジュール定義を返す
fn enclosing_module(result: &AnalysisResult, offset: usize) -> Option<&ModuleDef> {
    result.unit.items.iter().find_map(|item| match &item.inner {
        Item::Module(m) if contains(&(item.span.start..item.span.end), offset) => Some(m),
        _ => None,
    })
}
