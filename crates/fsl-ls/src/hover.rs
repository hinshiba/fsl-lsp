//! Hover ハンドラ
//!
//! analyzer の `hover_at` を呼び出し，Markdown を `Hover` に詰めて返す．

use tower_lsp::lsp_types::{Hover, HoverContents, HoverParams, MarkupContent, MarkupKind};

use crate::on_change::span_to_range;
use crate::pos::position_to_offset;
use crate::Backend;

impl Backend {
    /// `hover` の本体
    pub async fn handle_hover(&self, params: HoverParams) -> Option<Hover> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let docs = self.docs.read().await;
        let doc = docs.get(&uri)?;
        let offset = position_to_offset(&doc.line_index, pos)?;
        let payload = fsl_analyzer::api::hover_at(&doc.analysis, offset)?;

        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: payload.markdown,
            }),
            range: Some(span_to_range(&doc.line_index, &payload.range)),
        })
    }
}
