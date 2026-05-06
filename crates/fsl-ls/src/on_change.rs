//! ドキュメントが変更された場合の処理
//!

use line_index::LineIndex;
use tower_lsp::lsp_types::Url;
use fsl_analyzer::{self, analyze, Span};

use crate::Backend;

impl Backend {
    /// 変更を処理するエントリポイント
    pub async fn on_change(uri: Url, text: &str) {
        let line_index = LineIndex::new(text);
        let analyzer_result = analyze(unit)
    }
}
