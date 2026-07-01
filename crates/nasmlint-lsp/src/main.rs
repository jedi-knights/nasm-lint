//! `nasmlint-lsp` — a Language Server that surfaces nasm-lint diagnostics inline
//! in any LSP-capable editor (Neovim, VS Code, ...).
//!
//! The server is deliberately thin: it owns no analysis logic. On open and on
//! change it hands the document text to `nasmlint_core::analyze` (via the
//! `convert` module) and publishes the results — the exact same rules the CLI
//! runs, so a finding looks identical in the editor and in CI.
//!
//! Documents use full-text sync, so every change carries the whole buffer and the
//! server needs no document store of its own.

mod convert;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

struct Backend {
    client: Client,
}

impl Backend {
    /// Analyze `text` for `uri` and publish the diagnostics to the client.
    async fn publish(&self, uri: Url, text: &str, version: Option<i32>) {
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| std::path::PathBuf::from(uri.path()));
        let diagnostics = convert::analyze_text(path, text);
        self.client
            .publish_diagnostics(uri, diagnostics, version)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "nasm-lint".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "nasm-lint language server ready")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        self.publish(doc.uri, &doc.text, Some(doc.version)).await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        // Full-text sync: the final change event carries the entire document.
        if let Some(change) = params.content_changes.pop() {
            self.publish(
                params.text_document.uri,
                &change.text,
                Some(params.text_document.version),
            )
            .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // Clear this file's diagnostics once it is no longer open.
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
