//! FSL Language Server エントリポイント
//!
//! tower-lsp 上にハンドラを登録する．
//! 文書状態は `Backend.docs` にキャッシュし，
//! goto_definition / hover / completion はキャッシュを参照する．

use std::collections::HashMap;
use std::sync::Arc;

use fsl_analyzer::{AnalysisResult, ModuleIndex};
use line_index::LineIndex;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

mod completion;
mod definition;
mod format;
mod hover;
mod on_change;
mod pos;
mod workspace;

use workspace::{build_index, scan_fsl_files, workspace_roots};

/// 文書ごとの解析キャッシュ
pub struct DocumentState {
    pub text: String,
    pub line_index: LineIndex,
    pub analysis: AnalysisResult,
}

/// LSP サーバ本体
pub struct Backend {
    pub client: Client,
    /// URI ごとの最新解析状態
    pub docs: Arc<RwLock<HashMap<Url, DocumentState>>>,
    /// ワークスペース内の全 `.fsl` ソース  開いていないファイルも含む
    pub sources: Arc<RwLock<HashMap<Url, String>>>,
    /// `sources` から構築したモジュール横断索引
    pub index: Arc<RwLock<ModuleIndex>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // ワークスペース配下の .fsl を走査してモジュール索引を初期化する
        {
            let mut sources = self.sources.write().await;
            for root in workspace_roots(&params) {
                for path in scan_fsl_files(&root) {
                    if let (Ok(text), Ok(url)) =
                        (std::fs::read_to_string(&path), Url::from_file_path(&path))
                    {
                        sources.insert(url, text);
                    }
                }
            }
            *self.index.write().await = build_index(&sources);
        }

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
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".into()]),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
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

    /* ================ クエリ系 ================ */

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(self.handle_completion(params).await)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        Ok(self.handle_hover(params).await)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        Ok(self.handle_goto_definition(params).await)
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        Ok(self.handle_formatting(params).await)
    }

    /* ================ 変更系 ================
     * ドキュメントに対して，`on_change()`等の変更追跡が必要な種類の通知ハンドラ
     * ======================================== */

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.on_change(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // FULL sync の前提で末尾のフルテキストを採用する
        if let Some(change) = params.content_changes.into_iter().last() {
            self.on_change(params.text_document.uri, &change.text).await;
        }
    }

    async fn did_save(&self, _: DidSaveTextDocumentParams) {
        // 保存時の追加処理は現状なし
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        docs: Arc::new(RwLock::new(HashMap::new())),
        sources: Arc::new(RwLock::new(HashMap::new())),
        index: Arc::new(RwLock::new(ModuleIndex::default())),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
