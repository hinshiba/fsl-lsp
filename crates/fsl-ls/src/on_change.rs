//! ドキュメント変更時の処理

use fsl_analyzer::{Severity, Span, analyze};
use line_index::{LineIndex, TextSize};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range, Url};

use crate::Backend;

impl Backend {
    /// 変更を処理するエントリポイント
    /// 解析結果の診断を LSP に push する
    pub async fn on_change(&self, uri: Url, text: &str) {
        let line_index = LineIndex::new(text);
        let result = analyze(text);

        let diagnostics: Vec<Diagnostic> = result
            .diagnostics
            .into_iter()
            .map(|d| Diagnostic {
                range: span_to_range(&line_index, &d.span),
                severity: Some(severity_to_lsp(d.severity)),
                code: None,
                code_description: None,
                source: Some("fsl".to_string()),
                message: d.message,
                related_information: None,
                tags: None,
                data: None,
            })
            .collect();

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

/// アナライザの severity を LSP の severity に写す
fn severity_to_lsp(s: Severity) -> DiagnosticSeverity {
    match s {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Information => DiagnosticSeverity::INFORMATION,
        Severity::Hint => DiagnosticSeverity::HINT,
    }
}

/// バイト範囲の Span を LSP の Range に写す
fn span_to_range(line_index: &LineIndex, span: &Span) -> Range {
    Range {
        start: offset_to_position(line_index, span.start),
        end: offset_to_position(line_index, span.end),
    }
}

/// バイトオフセットを LSP の Position に写す
/// UTF-8 桁を character として用いる簡易実装
fn offset_to_position(line_index: &LineIndex, offset: usize) -> Position {
    let off = TextSize::try_from(offset).unwrap_or_else(|_| TextSize::from(0));
    match line_index.try_line_col(off) {
        Some(lc) => Position {
            line: lc.line,
            character: lc.col,
        },
        None => Position {
            line: 0,
            character: 0,
        },
    }
}
