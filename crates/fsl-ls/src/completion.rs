//! Completion ハンドラ
//!
//! analyzer の `completions_at` を呼び，スコープ内シンボル・キーワード・
//! 組込み型・ビルトイン関数を一括で `CompletionItem` 化して返す．

use fsl_analyzer::SymbolKind;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse,
};

use crate::pos::position_to_offset;
use crate::Backend;

impl Backend {
    /// `completion` の本体
    pub async fn handle_completion(&self, params: CompletionParams) -> Option<CompletionResponse> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;

        let docs = self.docs.read().await;
        let doc = docs.get(&uri)?;
        let offset = position_to_offset(&doc.line_index, pos)?;
        let list = fsl_analyzer::api::completions_at(&doc.analysis, offset);

        let mut items: Vec<CompletionItem> = Vec::new();

        // スコープ内シンボル
        for s in &list.symbols {
            items.push(CompletionItem {
                label: s.name.clone(),
                kind: Some(symbol_kind_to_completion(s.kind)),
                detail: s.ty.as_ref().map(|t| t.to_string()),
                ..Default::default()
            });
        }

        // キーワード
        for kw in list.keywords {
            items.push(CompletionItem {
                label: (*kw).to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            });
        }

        // 組込み型
        for ty in list.builtin_types {
            items.push(CompletionItem {
                label: (*ty).to_string(),
                kind: Some(CompletionItemKind::CLASS),
                ..Default::default()
            });
        }

        // ビルトイン関数
        for b in list.builtins {
            items.push(CompletionItem {
                label: (*b).to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                ..Default::default()
            });
        }

        Some(CompletionResponse::Array(items))
    }
}

/// `SymbolKind` を LSP `CompletionItemKind` に写す
fn symbol_kind_to_completion(k: SymbolKind) -> CompletionItemKind {
    match k {
        SymbolKind::Module => CompletionItemKind::MODULE,
        SymbolKind::Trait => CompletionItemKind::INTERFACE,
        SymbolKind::Reg | SymbolKind::Mem | SymbolKind::Output => CompletionItemKind::VARIABLE,
        SymbolKind::Input => CompletionItemKind::VARIABLE,
        SymbolKind::OutputFn | SymbolKind::Fn => CompletionItemKind::FUNCTION,
        SymbolKind::Instance => CompletionItemKind::FIELD,
        SymbolKind::Stage | SymbolKind::State => CompletionItemKind::CLASS,
        SymbolKind::Val => CompletionItemKind::VARIABLE,
        SymbolKind::Param => CompletionItemKind::VARIABLE,
        SymbolKind::Composite => CompletionItemKind::STRUCT,
    }
}
