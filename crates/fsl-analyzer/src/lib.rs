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
pub mod resolver;
pub mod scope;
pub mod span;
pub mod symbol;
pub mod symbols;
pub mod ty;

pub use fsl_parser::{CompilationUnit, Span};
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
/// パース → シンボル収集 → 名前解決 → Check 群の順に実行する
pub fn analyze(src: &str) -> AnalysisResult {
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
    resolver::resolve_references(&parsed.unit, &mut symbols, builtin::builtins());

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
}
