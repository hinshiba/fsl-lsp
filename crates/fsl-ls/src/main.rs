use line_index::LineIndex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

mod on_change;

#[derive(Debug)]
struct Backend {
    client: Client,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        will_save: Some(true),
                        will_save_wait_until: None,
                        save: None,
                    },
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions::default()),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(Some(CompletionResponse::Array(vec![
            CompletionItem::new_simple("Hello".to_string(), "Some detail".to_string()),
            CompletionItem::new_simple("Bye".to_string(), "More detail".to_string()),
        ])))
    }

    async fn hover(&self, _: HoverParams) -> Result<Option<Hover>> {
        Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String("You're hovering!".to_string())),
            range: None,
        }))
    }

    /* ================ 変更系 ================
     * ドキュメントに対して，`on_change()`等の変更追跡が必要な種類の通知ハンドラ
     * ======================================== */

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        on_change
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let _ = params;
        warn!("Got a textDocument/didChange notification, but it is not implemented");
    }

    async fn did_save(&self, param: DidSaveTextDocumentParams) {
        self.client.log_message(MessageType::INFO, "did_save").await;
        let test_diag = Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 20,
                    character: 1,
                },
            },
            severity: Some(DiagnosticSeverity::WARNING),
            code: None, // 診断の番号 E102とかつけれる
            code_description: None,
            source: None,
            message: "すべてのコードは警戒しなければなりません".to_string(),
            related_information: None,
            tags: None,
            data: None,
        };
        self.client
            .publish_diagnostics(param.text_document.uri, vec![test_diag], None)
            .await;
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
