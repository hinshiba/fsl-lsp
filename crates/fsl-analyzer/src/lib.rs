//! FSL の意味解析クレート
//!
//! AST を受け取り，名前解決とシンボルテーブル構築を行ったうえで
//! 登録された Check 群を順に実行して診断を集める．
//! LSP 機能 (Goto Definition / Hover / Completion) 用の検索 API も併せて提供する．

pub mod api;
pub mod builder;
pub mod builtin;
pub mod checks;
pub mod context;
pub mod index;
pub mod resolver;
pub mod scope;
pub mod span;
pub mod symbol;
pub mod symbols;
pub mod ty;

pub use fsl_parser::{CompilationUnit, Span};
pub use index::{Interface, Member, ModuleIndex};
pub use scope::{Scope, ScopeArena, ScopeId, ScopeKind};
pub use symbol::{DefId, Mutability, Symbol, SymbolKind};
pub use symbols::{Reference, ResolvedTo, SymbolTable};
pub use ty::TypeInfo;

use context::AnalysisContext;
use fsl_parser::parse;

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

// ============================================================
// 解析結果
// ============================================================

/// 解析結果. AST と診断とシンボルテーブルを束ねる
#[derive(Debug, Default, Clone)]
pub struct AnalysisResult {
    pub unit: CompilationUnit,
    pub diagnostics: Vec<Diagnostic>,
    pub symbols: SymbolTable,
}

// ============================================================
// 解析エントリポイント
// ============================================================

/// ソース文字列を直接受け取るエントリポイント
/// 外部ファイルを考慮しない単一ファイル解析
pub fn analyze(src: &str) -> AnalysisResult {
    analyze_with_index(src, &ModuleIndex::default())
}

/// ワークスペース索引を併用するエントリポイント
///
/// パース → シンボル収集 → 名前解決 → Check 群の順に実行する．
/// `index` は `extends` 継承メンバの解決に用いる．
pub fn analyze_with_index(src: &str, index: &ModuleIndex) -> AnalysisResult {
    let (parsed, lex_errs) = parse(src);
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // 字句エラー
    for span in lex_errs {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: "lexical error".to_string(),
            span,
        });
    }

    // 構文エラー
    for e in parsed.errors {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: e.message,
            span: e.span,
        });
    }

    // シンボル収集と名前解決
    let mut symbols = builder::build(&parsed.unit);
    resolver::resolve_references(&parsed.unit, &mut symbols, builtin::builtins(), index);

    // 意味解析チェック群を実行
    {
        let ctx = AnalysisContext {
            unit: &parsed.unit,
            symbols: &symbols,
            references: &symbols.references,
            builtins: builtin::builtins(),
        };
        checks::run_all_checks(&ctx, &mut diagnostics);
    }

    // span 順に整列して LSP 表示順を安定させる
    diagnostics.sort_by_key(|d| (d.span.start, d.span.end));

    AnalysisResult {
        unit: parsed.unit,
        diagnostics,
        symbols,
    }
}

// ============================================================
// テスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// エラー診断のメッセージ一覧を取り出す
    fn errors(result: &AnalysisResult) -> Vec<String> {
        result
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.message.clone())
            .collect()
    }

    // --- 既存基本ケース --------------------------------------

    #[test]
    fn empty_source_has_no_diagnostics() {
        let result = analyze("");
        assert!(result.diagnostics.is_empty());
        assert!(result.unit.items.is_empty());
    }

    #[test]
    fn well_formed_module_has_no_diagnostics() {
        let result = analyze("module M {}");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    }

    #[test]
    fn broken_source_emits_error_diagnostic() {
        let result = analyze("module {}");
        assert!(!result.diagnostics.is_empty());
        assert!(
            result
                .diagnostics
                .iter()
                .all(|d| d.severity == Severity::Error)
        );
    }

    // --- reassign ------------------------------------------

    #[test]
    fn val_cannot_be_reassigned_with_eq() {
        let src = "module M { def f(): Unit = { val x = 1 x = 2 } }";
        let r = analyze(src);
        assert!(
            errors(&r).iter().any(|m| m.contains("再代入できません")),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn val_cannot_be_reassigned_with_colon_eq() {
        let src = "module M { def f(): Unit = { val x = 1 x := 2 } }";
        let r = analyze(src);
        assert!(
            errors(&r).iter().any(|m| m.contains(":=")),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn input_cannot_be_assigned() {
        let src = "module M { input a: Bit(8) always { a = 0 } }";
        let r = analyze(src);
        assert!(
            errors(&r).iter().any(|m| m.contains("再代入できません")),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn param_cannot_be_assigned() {
        let src = "module M { def f(x: Bit(8)): Unit = { x = 1 } }";
        let r = analyze(src);
        assert!(
            errors(&r).iter().any(|m| m.contains("再代入できません")),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn reg_with_eq_is_error() {
        let src = "module M { reg r: Bit(8) always { r = 1 } }";
        let r = analyze(src);
        assert!(
            errors(&r).iter().any(|m| m.contains("`:=`")),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn reg_with_colon_eq_is_ok() {
        let src = "module M { reg r: Bit(8) always { r := 1 } }";
        let r = analyze(src);
        assert!(errors(&r).is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn output_with_eq_is_ok() {
        let src = "module M { output o: Bit(8) always { o = 1 } }";
        let r = analyze(src);
        assert!(errors(&r).is_empty(), "{:?}", r.diagnostics);
    }

    // --- unresolved ----------------------------------------

    #[test]
    fn unknown_identifier_is_error() {
        let src = "module M { reg r: Bit(8) always { r := unknown } }";
        let r = analyze(src);
        assert!(
            errors(&r).iter().any(|m| m.contains("unknown")),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn builtin_display_is_allowed() {
        let src = r#"module M { always { _display("hi") } }"#;
        let r = analyze(src);
        assert!(errors(&r).is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn builtin_time_is_allowed() {
        let src = r#"module M { always { _display("%d", _time) } }"#;
        let r = analyze(src);
        assert!(errors(&r).is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn module_local_symbol_resolves() {
        let src = "module M { reg r: Bit(8) always { r := r + 1 } }";
        let r = analyze(src);
        assert!(errors(&r).is_empty(), "{:?}", r.diagnostics);
    }

    // --- スコープ ------------------------------------------

    #[test]
    fn val_in_function_body_is_visible() {
        let src = "module M { def f(): Bit(8) = { val x = 1 x } }";
        let r = analyze(src);
        assert!(errors(&r).is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn val_in_inner_block_shadows() {
        let src = "module M { def f(): Bit(8) = { val x = 1 { val x = 2 x } } }";
        let r = analyze(src);
        assert!(errors(&r).is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn val_outside_scope_is_unresolved() {
        let src = "module M { def f(): Bit(8) = { { val y = 1 } y } }";
        let r = analyze(src);
        assert!(
            errors(&r).iter().any(|m| m.contains("y")),
            "{:?}",
            r.diagnostics
        );
    }

    // --- api 層 --------------------------------------------

    #[test]
    fn definition_at_returns_def_span_of_reg() {
        let src = "module M { reg count: Bit(8) always { count := count + 1 } }";
        let r = analyze(src);
        // "count :=" の位置の参照を解決 → 定義の "count" を指す
        let offset = src.find("count :=").unwrap();
        let def = api::definition_at(&r, offset).expect("def must exist");
        assert_eq!(&src[def], "count");
    }

    #[test]
    fn visible_symbols_include_outer_and_inner() {
        let src = "module M { reg r: Bit(8) def f(): Bit(8) = { val x = 1 x } }";
        let r = analyze(src);
        let inside = src.rfind('x').unwrap();
        let names: Vec<_> = r
            .symbols
            .visible_at(inside)
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert!(names.iter().any(|n| n == "r"), "{:?}", names);
        assert!(names.iter().any(|n| n == "x"), "{:?}", names);
        assert!(names.iter().any(|n| n == "f"), "{:?}", names);
    }

    // --- サンプル回帰 (パニックしないこと) ------------------

    #[test]
    fn cpu8_sample_does_not_panic() {
        let src = include_str!("../../../fsl-sample/fsl_tutorial_samples-main/cpu8.fsl");
        let _ = analyze(src);
    }

    // --- extends 継承 (外部ファイル想定) --------------------

    /// 別ソースの trait を継承したモジュールで継承メンバ参照が解決する
    #[test]
    fn extends_trait_member_resolves_across_files() {
        let trait_src = "trait Op { val ADD = 0b100000 }";
        let mod_src = "module M extends Op { output o: Bit(6) always { o = ADD } }";
        let mut index = ModuleIndex::default();
        index.add_source(trait_src);
        index.add_source(mod_src);

        let r = analyze_with_index(mod_src, &index);
        assert!(errors(&r).is_empty(), "{:?}", r.diagnostics);
    }

    /// extends が無ければ同じ参照は未宣言エラーになる
    #[test]
    fn unknown_without_extends_still_errors() {
        let src = "module M { output o: Bit(6) always { o = ADD } }";
        let r = analyze(src);
        assert!(
            errors(&r).iter().any(|m| m.contains("ADD")),
            "{:?}",
            r.diagnostics
        );
    }

    /// 多段の extends で継承メンバが解決する
    #[test]
    fn extends_chain_member_resolves() {
        let base = "trait Base { val ROOT = 1 }";
        let mid = "module Mid extends Base { }";
        let leaf = "module Leaf extends Mid { always { val x = ROOT } }";
        let mut index = ModuleIndex::default();
        index.add_source(base);
        index.add_source(mid);
        index.add_source(leaf);

        let r = analyze_with_index(leaf, &index);
        assert!(errors(&r).is_empty(), "{:?}", r.diagnostics);
    }

    /// 通常補完に継承メンバが含まれる
    #[test]
    fn completions_include_inherited_members() {
        let trait_src = "trait Op { val ADD = 0b100000 }";
        let mod_src = "module M extends Op { always { val x = 1 } }";
        let mut index = ModuleIndex::default();
        index.add_source(trait_src);

        let r = analyze_with_index(mod_src, &index);
        let offset = mod_src.find("val x").unwrap();
        let list = api::completions_at(&r, &index, offset);
        assert!(
            list.inherited.iter().any(|m| m.name == "ADD"),
            "inherited: {:?}",
            list.inherited.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
    }

    // --- val new のメンバ補完 (外部ファイル想定) ------------

    /// 別ファイルで定義したモジュールインスタンスの出力端子を補完する
    #[test]
    fn member_completion_lists_module_outputs() {
        let add4 = "module add4 { input a: Bit(4) output out: Bit(4) output co: Bit(1) }";
        let user = "module Top { val a0 = new add4 always { val x = a0 } }";
        let mut index = ModuleIndex::default();
        index.add_source(add4);
        index.add_source(user);

        let r = analyze_with_index(user, &index);
        let offset = user.rfind("a0").unwrap();
        let members =
            api::member_completions(&r, &index, offset, "a0").expect("instance members");
        let names: Vec<_> = members.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"out"), "{:?}", names);
        assert!(names.contains(&"co"), "{:?}", names);
    }

    /// 実サンプルで trait 継承の名前解決が成立する
    #[test]
    fn p32execunit_inherits_opcode_trait() {
        let opcode = include_str!("../../../fsl-sample/p32ExecUnit-main/p32Opcode.fsl");
        let exec = include_str!("../../../fsl-sample/p32ExecUnit-main/p32ExecUnit.fsl");
        let mut index = ModuleIndex::default();
        index.add_source(opcode);
        index.add_source(exec);

        let with = analyze_with_index(exec, &index);
        let without = analyze(exec);
        // 継承解決により未宣言エラーが減る
        assert!(
            errors(&with).len() < errors(&without).len(),
            "with={} without={}",
            errors(&with).len(),
            errors(&without).len()
        );
        // SPECIAL は trait p32Opcode 由来なので未宣言にならない
        assert!(!errors(&with).iter().any(|m| m.contains("`SPECIAL`")));
    }

    /// インスタンスでないレシーバはメンバ補完を返さない
    #[test]
    fn member_completion_rejects_non_instance() {
        let src = "module M { reg r: Bit(8) always { val x = r } }";
        let index = ModuleIndex::default();
        let r = analyze_with_index(src, &index);
        let offset = src.rfind('r').unwrap();
        assert!(api::member_completions(&r, &index, offset, "r").is_none());
    }

    // --- BitN.fsl プレリュード ------------------------------

    /// プレリュードが Bit のメンバを公開する
    #[test]
    fn prelude_exposes_bit_members() {
        let members = index::prelude().resolved_members("Bit");
        let names: Vec<_> = members.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"zero"), "{:?}", names);
        assert!(names.contains(&"one"), "{:?}", names);
        assert!(names.contains(&"allOne"), "{:?}", names);
    }

    /// `Bit(n)` を式位置で用いても未宣言シンボル扱いされない
    #[test]
    fn bit_constructor_is_resolved() {
        let src = "module M { def f(w): Bit(w) = Bit(w).zero }";
        let r = analyze(src);
        assert!(
            !errors(&r).iter().any(|m| m.contains("Bit")),
            "{:?}",
            r.diagnostics
        );
    }

    /// `Bit(n).zero` のメンバ補完がプレリュード経由で得られる
    #[test]
    fn member_completion_resolves_bit_prelude() {
        let index = ModuleIndex::default();
        let r = analyze_with_index("module M {}", &index);
        let members =
            api::member_completions(&r, &index, 0, "Bit").expect("prelude Bit members");
        let names: Vec<_> = members.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"allOne"), "{:?}", names);
    }
}
