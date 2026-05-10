//! Goto Definition ハンドラ
//!
//! `Position` を byte offset に変換し，analyzer の `definition_at` を呼ぶ．
//! 結果の Span を LSP `Location` に整形して返す．

use tower_lsp::lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Location};

use crate::on_change::span_to_range;
use crate::pos::position_to_offset;
use crate::Backend;

impl Backend {
    /// `goto_definition` の本体
    pub async fn handle_goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Option<GotoDefinitionResponse> {
        let uri = params.text_document_position_params.text_document.uri.clone();
        let pos = params.text_document_position_params.position;

        let docs = self.docs.read().await;
        let doc = docs.get(&uri)?;
        let offset = position_to_offset(&doc.line_index, pos)?;
        let span = fsl_analyzer::api::definition_at(&doc.analysis, offset)?;

        let range = span_to_range(&doc.line_index, &span);
        Some(GotoDefinitionResponse::Scalar(Location { uri, range }))
    }
}
