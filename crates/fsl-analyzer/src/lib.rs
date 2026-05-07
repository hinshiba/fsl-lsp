//! FSL のアナライザ
//!
//! 現状は意味解析を素通りする実装である．
//! 字句解析・構文解析の結果をそのまま `AnalysisResult` に束ねるのみで，
//! 名前解決や型検査などの本格的な処理は行わない．

use fsl_parser::{CompilationUnit, parse};

pub use fsl_parser::Span;

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

#[derive(Debug, Default, Clone)]
pub struct AnalysisResult {
    /// 構文木
    pub unit: CompilationUnit,
    /// 字句・構文・意味解析の診断をまとめたもの
    pub diagnostics: Vec<Diagnostic>,
}

// ============================================================
// 解析エントリポイント
// ============================================================

/// ソース文字列を直接受け取るエントリポイント．
/// 意味解析は素通り
pub fn analyze(src: &str) -> AnalysisResult {
    let (parsed, lex_errs) = parse(src);
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    for span in lex_errs {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: "lexical error".to_string(),
            span,
        });
    }

    for e in parsed.errors {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: e.message,
            span: e.span,
        });
    }

    AnalysisResult {
        unit: parsed.unit,
        diagnostics,
    }
}

// ============================================================
// テスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

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
}
