use futures::channel::mpsc;
use futures::io::{AsyncRead, AsyncWrite};
use futures::Stream;
use std::collections::HashSet;
use std::pin::Pin;
use std::sync::{LazyLock, Mutex};
use std::task::{Context, Poll};

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[wasm_bindgen]
extern "C" {
    fn send_to_js(data: &js_sys::Uint8Array);
}

static TX: LazyLock<Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>> =
    LazyLock::new(|| Mutex::new(None));

pub struct LspReader {
    rx: mpsc::UnboundedReceiver<Vec<u8>>,
    current_buf: Vec<u8>,
    cursor: usize,
}

impl AsyncRead for LspReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.cursor < self.current_buf.len() {
            let to_read = std::cmp::min(buf.len(), self.current_buf.len() - self.cursor);
            buf[..to_read].copy_from_slice(&self.current_buf[self.cursor..self.cursor + to_read]);
            self.cursor += to_read;
            return Poll::Ready(Ok(to_read));
        }

        match Pin::new(&mut self.rx).poll_next(cx) {
            Poll::Ready(Some(data)) => {
                if data.is_empty() {
                    return Poll::Ready(Ok(0));
                }
                self.current_buf = data;
                self.cursor = 0;
                let to_read = std::cmp::min(buf.len(), self.current_buf.len());
                buf[..to_read].copy_from_slice(&self.current_buf[..to_read]);
                self.cursor = to_read;
                Poll::Ready(Ok(to_read))
            }
            Poll::Ready(None) => Poll::Ready(Ok(0)),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct LspWriter;

impl AsyncWrite for LspWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let array = js_sys::Uint8Array::from(buf);
        send_to_js(&array);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[wasm_bindgen]
pub fn handle_message_from_js(msg: &[u8]) {
    if let Some(tx) = TX.lock().unwrap().as_ref() {
        let _ = tx.unbounded_send(msg.to_vec());
    }
}

#[wasm_bindgen]
pub fn start_lsp() {
    console_error_panic_hook::set_once();

    let (tx, rx) = mpsc::unbounded::<Vec<u8>>();
    *TX.lock().unwrap() = Some(tx);

    let reader = LspReader {
        rx,
        current_buf: Vec::new(),
        cursor: 0,
    };
    let writer = LspWriter;

    let (service, socket) = LspService::new(|client| Backend {
        client,
        document_text: Mutex::new(String::new()),
    });

    spawn_local(async move {
        Server::new(reader, writer, socket).serve(service).await;
    });
}

struct Backend {
    client: Client,
    document_text: Mutex<String>,
}

// Converts a raw byte offset from sysml-v2-parser diagnostics into
// the LSP-standard line/character format.
fn offset_to_position(text: &str, offset: usize) -> Position {
    let mut line = 0;
    let mut character = 0;
    for (i, ch) in text.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }
    Position { line, character }
}

// Extract alphanumeric identifiers dynamically from active text to suggest as types or properties.
fn extract_identifiers(text: &str) -> HashSet<String> {
    let mut identifiers = HashSet::new();
    let keywords: HashSet<&str> = [
        "package",
        "import",
        "part",
        "attribute",
        "port",
        "action",
        "state",
        "connection",
        "item",
        "def",
        "ref",
        "doc",
        "metadata",
    ]
    .iter()
    .cloned()
    .collect();

    for word in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if !word.is_empty()
            && !keywords.contains(word)
            && word.chars().next().unwrap_or(' ').is_alphabetic()
        {
            identifiers.insert(word.to_string());
        }
    }
    identifiers
}

impl Backend {
    async fn validate_sysml(&self, uri: Url, text: String) {
        let mut diagnostics = Vec::new();

        // Perform error-resilient parsing designed for IDEs
        let parse_result = sysml_v2_parser::parse_for_editor(&text);

        for err in parse_result.errors {
            // Safely unwrap the optional offset and length, casting to usize
            let start_offset = err.offset.unwrap_or(0) as usize;
            let length = err.length.unwrap_or(1) as usize;
            let end_offset = start_offset + length;

            let start_pos = offset_to_position(&text, start_offset);
            let end_pos = offset_to_position(&text, end_offset);

            diagnostics.push(Diagnostic {
                range: Range {
                    start: start_pos,
                    end: end_pos,
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("SysML-v2-Parser".to_string()),
                message: err.message.to_string(), // Safely convert error message to String
                related_information: None,
                tags: None,
                data: None,
            });
        }

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![
                        ":".to_string(),
                        ";".to_string(),
                        ".".to_string(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {}

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let text = params.text_document.text;
        *self.document_text.lock().unwrap() = text.clone();
        self.validate_sysml(params.text_document.uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.first() {
            let text = change.text.clone();
            *self.document_text.lock().unwrap() = text.clone();
            self.validate_sysml(params.text_document.uri, text).await;
        }
    }

    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        let text = self.document_text.lock().unwrap().clone();

        let keywords = vec![
            "package",
            "import",
            "part",
            "attribute",
            "port",
            "action",
            "state",
            "connection",
            "item",
            "def",
            "ref",
            "doc",
            "metadata",
            "individual",
            "occurrence",
            "structure",
            "behavior",
            "constraint",
            "requirement",
            "calculation",
            "analysis",
            "view",
            "viewpoint",
            "rendering",
            "exhibit",
            "perform",
            "flow",
            "interface",
            "transition",
        ];

        let mut completions = Vec::new();

        // 1. Static Keywords
        for kw in keywords {
            completions.push(CompletionItem {
                label: kw.to_string(),
                detail: Some("SysML v2 Keyword".to_string()),
                kind: Some(CompletionItemKind::KEYWORD),
                insert_text: Some(kw.to_string()),
                ..Default::default()
            });
        }

        // 2. Extracted Local Types & Definitions
        let local_identifiers = extract_identifiers(&text);
        for id in local_identifiers {
            completions.push(CompletionItem {
                label: id.clone(),
                detail: Some("Defined Model Element".to_string()),
                kind: Some(CompletionItemKind::CLASS),
                insert_text: Some(id),
                ..Default::default()
            });
        }

        Ok(Some(CompletionResponse::Array(completions)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = params.text_document_position_params.position;
        let dynamic_message = format!(
            "### SysML v2 Language Server\n\nHover position:\n* **Line**: {}\n* **Column**: {}\n\n*Diagnostics and parsing compiled natively to WebAssembly!*",
            position.line + 1,
            position.character + 1
        );

        Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(dynamic_message)),
            range: None,
        }))
    }
}
