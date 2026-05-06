//! FSL のアナライザ

use std::collections::HashMap;

use fsl_parser::{
    BinaryOp, Block, BlockKind, CompilationUnit, Expr, ExprKind, Field, FnDef, FslType, Item,
    ModuleDef, ParseError, Pattern, RegDecl, Stmt, Statement, Type, ValDecl,
};

pub use fsl_parser::Span;

// ============================================================
// シンボル
// ============================================================

/// シンボルが保持する型情報の単純化表現．
/// 構文上の `Type` を意味解析向けに正規化したもの．
/// ビット幅の畳み込みは未実装．
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolType {
    Unit,
    Boolean,
    Bit { width: Option<usize> },
    Int,
    String,
    Array(Box<SymbolType>),
    List(Box<SymbolType>),
    Tuple(Vec<SymbolType>),
    Named(String),
    Unknown,
}

impl SymbolType {
    pub fn from_ast(ty: &Type) -> Self {
        match &ty.kind {
            FslType::Unit => SymbolType::Unit,
            FslType::Boolean => SymbolType::Boolean,
            FslType::Int => SymbolType::Int,
            FslType::String => SymbolType::String,
            FslType::Bit(expr) => SymbolType::Bit {
                width: const_int(expr),
            },
            FslType::Array(inner) => SymbolType::Array(Box::new(Self::from_ast(inner))),
            FslType::List(inner) => SymbolType::List(Box::new(Self::from_ast(inner))),
            FslType::Tuple(elems) => SymbolType::Tuple(elems.iter().map(Self::from_ast).collect()),
            FslType::Named(name) => SymbolType::Named(name.node.clone()),
        }
    }
}

/// 整数式が定数であれば値を返す．現状は単純な十進・十六進・二進リテラルのみ．
fn const_int(expr: &Expr) -> Option<usize> {
    match &expr.kind {
        ExprKind::Int(s) => parse_int_literal(s),
        _ => None,
    }
}

fn parse_int_literal(s: &str) -> Option<usize> {
    let cleaned: String = s.chars().filter(|c| *c != '_').collect();
    if let Some(rest) = cleaned.strip_prefix("0x") {
        usize::from_str_radix(rest, 16).ok()
    } else if let Some(rest) = cleaned.strip_prefix("0b") {
        usize::from_str_radix(rest, 2).ok()
    } else {
        cleaned.parse::<usize>().ok()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Module,
    Trait,
    Register {
        ty: SymbolType,
    },
    Memory {
        elem: SymbolType,
        size: Option<usize>,
    },
    InputPort {
        ty: SymbolType,
    },
    OutputPort {
        ty: SymbolType,
    },
    OutputFn {
        params: Vec<SymbolType>,
        ret: SymbolType,
    },
    Function {
        is_private: bool,
        params: Vec<SymbolType>,
        ret: Option<SymbolType>,
    },
    Stage {
        params: Vec<SymbolType>,
    },
    State,
    Instance {
        module_name: String,
    },
    Type {
        fields: Vec<(String, SymbolType)>,
    },
    Val {
        ty: Option<SymbolType>,
    },
    Param {
        ty: Option<SymbolType>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub def_span: Span,
    pub used: bool,
}

// ============================================================
// スコープ
// ============================================================

/// 単一スコープのシンボルテーブル．
#[derive(Debug, Default, Clone)]
pub struct Scope {
    pub symbols: HashMap<String, Symbol>,
    pub kind: ScopeKind,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum ScopeKind {
    #[default]
    Top,
    Module,
    Trait,
    Function,
    Stage,
    State,
    Block,
}

impl Scope {
    pub fn new(kind: ScopeKind) -> Self {
        Self {
            symbols: HashMap::new(),
            kind,
        }
    }

    pub fn insert(&mut self, sym: Symbol) -> Option<Symbol> {
        self.symbols.insert(sym.name.clone(), sym)
    }

    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }
}

// ============================================================
// 診断
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
}

impl Diagnostic {
    pub fn from_parse_error(e: ParseError) -> Self {
        Self {
            severity: Severity::Error,
            message: e.message,
            span: e.span,
        }
    }
}

// ============================================================
// 解析結果
// ============================================================

#[derive(Debug, Default, Clone)]
pub struct AnalysisResult {
    /// 構文の全要素
    pub unit: CompilationUnit,
    /// シンボルテーブル
    pub top: Scope,
    /// 診断情報
    pub diagnostics: Vec<Diagnostic>,
}

// ============================================================
// 解析エントリポイント（ひな形）
// ============================================================

/// ソース文字列を直接受け取るエントリポイント．
pub fn parse_source(src: &str) -> AnalysisResult {
    fsl_parser
}

/// CompilationUnit を受け取り，トップレベルのシンボルテーブルと診断を返す．
/// 現状はトップレベル（モジュール・trait）のみを登録する．
/// モジュール内アイテムは最小限のみで，スコープチェインの完全実装は将来作業．
pub fn analyze(unit: &CompilationUnit) -> AnalysisResult {
    let mut result = AnalysisResult::default();
    result.top.kind = ScopeKind::Top;

    for item in &unit.items {
        match item {
            Item::Module(m) => {
                let sym = Symbol {
                    name: m.name.node.clone(),
                    kind: SymbolKind::Module,
                    def_span: m.span.clone(),
                    used: false,
                };
                if let Some(prev) = result.top.insert(sym) {
                    result.diagnostics.push(Diagnostic {
                        severity: Severity::Error,
                        message: format!("module `{}` is defined more than once", prev.name),
                        span: m.name.span.clone(),
                    });
                }
                analyze_module(m, &mut result);
            }
            Item::Trait(t) => {
                let sym = Symbol {
                    name: t.name.node.clone(),
                    kind: SymbolKind::Trait,
                    def_span: t.span.clone(),
                    used: false,
                };
                if let Some(prev) = result.top.insert(sym) {
                    result.diagnostics.push(Diagnostic {
                        severity: Severity::Error,
                        message: format!("trait `{}` is defined more than once", prev.name),
                        span: t.name.span.clone(),
                    });
                }
            }
        }
    }
    result
}

/// モジュールスコープを構築する．今は使用していないが将来的に拡張する．
fn analyze_module(_m: &ModuleDef, _result: &mut AnalysisResult) {
    // TODO: モジュールスコープの構築
    //   - reg/mem/input/output/instance/fn/stage/type/val を Scope に登録
    //   - 関数本体は Function スコープを開いて再帰的に解析
    //   - stage 本体は Stage スコープを開いて state を登録
    //   - 未使用シンボル警告
    //   - 重複定義エラー
    //   - private 関数の外部呼び出し検出
    //   - val・パラメータへの再代入検出
}

// ============================================================
// 訪問者ヘルパ
// ============================================================
//
// 後続のフェーズで以下の訪問者群を実装する．
// - 式の自由変数収集
// - val 再代入検出
// - private 呼び出し検出

#[allow(dead_code)]
fn collect_free_vars(_e: &Expr) -> Vec<String> {
    Vec::new()
}

#[allow(dead_code)]
fn validate_block(_b: &Block) -> Vec<Diagnostic> {
    Vec::new()
}

#[allow(dead_code)]
fn validate_stmt(_s: &Stmt) -> Vec<Diagnostic> {
    Vec::new()
}

#[allow(dead_code)]
fn validate_fn(_f: &FnDef) -> Vec<Diagnostic> {
    Vec::new()
}

#[allow(dead_code)]
fn validate_reg(_r: &RegDecl) -> Vec<Diagnostic> {
    Vec::new()
}

#[allow(dead_code)]
fn validate_val(_v: &ValDecl) -> Vec<Diagnostic> {
    Vec::new()
}

// 以下は将来拡張で参照するためにシンボルを匿名で利用しないよう抑制する
#[allow(dead_code)]
fn _ensure_used_imports(_: Field, _: Statement, _: ExprKind, _: BinaryOp, _: BlockKind, _: Pattern) {
}

// ============================================================
// テスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use fsl_parser::parse;

    #[test]
    fn registers_top_level_module() {
        let (parsed, lex_errs) = parse("module M {}");
        assert!(lex_errs.is_empty());
        assert!(parsed.errors.is_empty());
        let result = analyze(&parsed.unit);
        assert!(result.top.lookup("M").is_some());
        assert!(matches!(
            result.top.lookup("M").unwrap().kind,
            SymbolKind::Module
        ));
    }

    #[test]
    fn duplicate_module_diagnostic() {
        let (parsed, _) = parse("module M {} module M {}");
        let result = analyze(&parsed.unit);
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn registers_trait() {
        let (parsed, _) = parse("trait T { val A = 0 }");
        let result = analyze(&parsed.unit);
        assert!(matches!(
            result.top.lookup("T").unwrap().kind,
            SymbolKind::Trait
        ));
    }

    #[test]
    fn bit_width_parse() {
        assert_eq!(parse_int_literal("0x10"), Some(16));
        assert_eq!(parse_int_literal("0b1010"), Some(10));
        assert_eq!(parse_int_literal("1_000"), Some(1000));
    }
}
