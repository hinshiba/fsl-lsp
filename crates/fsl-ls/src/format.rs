//! Formatting ハンドラ
//!
//! `fsl_fmt::format` を呼び出し，文書全体を 1 つの TextEdit で置換する．
//! パース／レキサーエラー時は None を返し，LSP 仕様で「変更なし」と解釈される．

use tower_lsp::lsp_types::{DocumentFormattingParams, Position, Range, TextEdit};

use crate::on_change::offset_to_position;
use crate::Backend;

impl Backend {
    /// `formatting` の本体．成功時は文書全体を置換する単一 TextEdit を返す
    pub async fn handle_formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> Option<Vec<TextEdit>> {
        let uri = params.text_document.uri;

        // キャッシュ済みのドキュメント本文を整形対象とする
        let docs = self.docs.read().await;
        let doc = docs.get(&uri)?;

        // 整形に失敗した場合は変更なしとして None を返す
        let new_text = fsl_fmt::format(&doc.text)?;

        // 文書全体を 1 つの TextEdit で置換する
        let end = offset_to_position(&doc.line_index, doc.text.len());
        let range = Range {
            start: Position::new(0, 0),
            end,
        };
        Some(vec![TextEdit { range, new_text }])
    }
}
