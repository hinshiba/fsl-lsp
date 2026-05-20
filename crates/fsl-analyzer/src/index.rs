//! ワークスペース横断モジュール索引
//!
//! 複数 `.fsl` ファイルにまたがるモジュール/トレイトの公開インタフェースを
//! 名前引きで保持する．`val new` によるインスタンスのメンバ補完と，
//! `extends` / `with` による継承メンバの解決に用いる．

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use fsl_parser::*;

use crate::symbol::SymbolKind;
use crate::ty::TypeInfo;

/// 暗黙的にインクルードされる組込みプレリュードのソース
///
/// `Bit(n).zero` などを定義し，全ソースへ常に取り込まれる．
const PRELUDE_SRC: &str = include_str!("prelude/BitN.fsl");

/// 組込みプレリュードのインタフェース索引を返す
///
/// 初回呼び出しでパースして以後使い回す．
pub fn prelude() -> &'static ModuleIndex {
    static PRELUDE: OnceLock<ModuleIndex> = OnceLock::new();
    PRELUDE.get_or_init(|| {
        let mut index = ModuleIndex::default();
        index.add_source(PRELUDE_SRC);
        index
    })
}

/// モジュール/トレイトが公開する 1 メンバ
///
/// `instance.member` のメンバ候補，および継承で取り込まれる宣言を表す．
#[derive(Debug, Clone)]
pub struct Member {
    pub name: String,
    pub kind: SymbolKind,
    pub ty: Option<TypeInfo>,
}

/// 単一モジュール/トレイトの公開インタフェース
#[derive(Debug, Clone)]
pub struct Interface {
    pub name: String,
    /// `Module` または `Trait`
    pub kind: SymbolKind,
    /// `extends` と `with` で指定された継承元名
    pub bases: Vec<String>,
    pub members: Vec<Member>,
}

/// 名前引きのモジュール/トレイト索引
///
/// ワークスペース内の全 `.fsl` から収集したインタフェースを保持する．
#[derive(Debug, Default, Clone)]
pub struct ModuleIndex {
    map: HashMap<String, Interface>,
}

impl ModuleIndex {
    /// 解析済み `CompilationUnit` のインタフェースを取り込む
    /// 同名は後勝ちで上書きする
    pub fn add_unit(&mut self, unit: &CompilationUnit) {
        for item in &unit.items {
            let iface = match &item.inner {
                Item::Module(m) => interface_of_module(m),
                Item::Trait(t) => interface_of_trait(t),
            };
            self.map.insert(iface.name.clone(), iface);
        }
    }

    /// ソース文字列をパースしてインタフェースを取り込む
    pub fn add_source(&mut self, src: &str) {
        let (parsed, _) = parse(src);
        self.add_unit(&parsed.unit);
    }

    /// 名前からインタフェースを引く
    pub fn get(&self, name: &str) -> Option<&Interface> {
        self.map.get(name)
    }

    /// `name` のモジュール/トレイトが公開する全メンバを継承込みで返す
    ///
    /// 自身のメンバを先に並べ，継承メンバが同名を上書きしないようにする．
    pub fn resolved_members(&self, name: &str) -> Vec<Member> {
        self.members_of_bases(&[name.to_string()])
    }

    /// `bases` 群が公開する全メンバを継承込みで集める
    ///
    /// `extends` 対象モジュールに継承メンバを注入する用途で用いる．
    /// 先に現れたメンバが同名を勝ち取り，循環継承は visited で打ち切る．
    pub fn members_of_bases(&self, bases: &[String]) -> Vec<Member> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut visited = HashSet::new();
        for b in bases {
            self.collect(b, &mut out, &mut seen, &mut visited);
        }
        out
    }

    /// `name` を起点にメンバを再帰収集する内部ヘルパ
    fn collect(
        &self,
        name: &str,
        out: &mut Vec<Member>,
        seen: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) {
        if !visited.insert(name.to_string()) {
            return;
        }
        let Some(iface) = self.map.get(name) else {
            return;
        };
        for m in &iface.members {
            if seen.insert(m.name.clone()) {
                out.push(m.clone());
            }
        }
        for base in &iface.bases {
            self.collect(base, out, seen, visited);
        }
    }
}

// ============================================================
// インタフェース抽出
// ============================================================

/// `ModuleDef` の `extends` / `with` を継承元名のリストにまとめる純粋関数
pub fn module_bases(m: &ModuleDef) -> Vec<String> {
    let mut bases = Vec::new();
    if let Some(e) = &m.extends {
        bases.push(e.inner.clone());
    }
    if let Some(ws) = &m.with_traits {
        bases.extend(ws.iter().map(|w| w.inner.clone()));
    }
    bases
}

/// モジュール定義から公開インタフェースを作る
fn interface_of_module(m: &ModuleDef) -> Interface {
    Interface {
        name: m.name.inner.clone(),
        kind: SymbolKind::Module,
        bases: module_bases(m),
        members: m.items.iter().flat_map(|f| members_of_field(&f.inner)).collect(),
    }
}

/// トレイト定義から公開インタフェースを作る
fn interface_of_trait(t: &TraitDef) -> Interface {
    Interface {
        name: t.name.inner.clone(),
        kind: SymbolKind::Trait,
        bases: Vec::new(),
        members: t.items.iter().flat_map(|f| members_of_field(&f.inner)).collect(),
    }
}

/// フィールド宣言が公開するメンバ群を取り出す
///
/// `private` 関数は外部から不可視のため除外する．
/// `val` のタプル分配は要素ごとに 1 メンバとする．
fn members_of_field(f: &Field) -> Vec<Member> {
    let one = |name: &Ident, kind, ty| {
        vec![Member {
            name: name.inner.clone(),
            kind,
            ty,
        }]
    };
    match f {
        Field::Input(d) => one(&d.name, SymbolKind::Input, Some(TypeInfo::from_ast(&d.ty))),
        Field::Output(d) => one(&d.name, SymbolKind::Output, Some(TypeInfo::from_ast(&d.ty))),
        Field::OutputFn(d) => one(
            &d.name,
            SymbolKind::OutputFn,
            d.ret.as_ref().map(TypeInfo::from_ast),
        ),
        Field::Reg(d) => one(&d.name, SymbolKind::Reg, Some(TypeInfo::from_ast(&d.ty))),
        Field::Mem(d) => {
            let elem = TypeInfo::from_ast(&d.elem_ty);
            one(&d.name, SymbolKind::Mem, Some(TypeInfo::Array(Box::new(elem))))
        }
        Field::NewInstance(d) => one(
            &d.name,
            SymbolKind::Instance,
            Some(TypeInfo::Named(d.module_name.inner.clone())),
        ),
        Field::Fn(d) if !d.is_private => {
            one(&d.name, SymbolKind::Fn, d.ret.as_ref().map(TypeInfo::from_ast))
        }
        Field::Stage(d) => one(&d.name, SymbolKind::Stage, None),
        Field::Composite(d) => one(&d.name, SymbolKind::Composite, None),
        Field::Val(v) => match &v.pattern {
            ValLhs::Single(n) => one(n, SymbolKind::Val, v.ty.as_ref().map(TypeInfo::from_ast)),
            ValLhs::Tuple(ns) => ns
                .iter()
                .map(|n| Member {
                    name: n.inner.clone(),
                    kind: SymbolKind::Val,
                    ty: None,
                })
                .collect(),
        },
        // private 関数・always・initial・解析失敗は公開メンバを持たない
        Field::Fn(_) | Field::Always(_) | Field::Initial(_) | Field::Error => Vec::new(),
    }
}
