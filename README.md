# SysML v2 Browser IDE

**Live Demo: [sysmlv2.thesayem.pro.bd](https://sysmlv2.thesayem.pro.bd/)**

A client-side SysML v2 editor featuring live diagnostics, autocompletion, and hover tooltips. The editor is powered by a standard Language Server Protocol (LSP) implementation, written in Rust and compiled to WebAssembly (WASM) to run entirely within the browser.

By executing the language server in a Web Worker, the IDE processes code analysis and syntax diagnostics locally, eliminating the need for a backend server or network-based LSP communication.

---

## Key Features

- **Local Execution:** No compilation backends or remote servers required.
- **Background Processing:** Parsing and analysis run in a dedicated Web Worker to prevent UI lag.
- **Monaco Editor Integration:** Supports standard editor behaviors including real-time diagnostics (squigglies), autocomplete suggestions, and hover documentation.
- **Standard LSP Compliance:** Uses a standard Rust LSP implementation (`tower-lsp`) adapted for WebAssembly.

---

## Architecture Overview

```
┌──── MAIN THREAD ───────────────────────┐  ┌──── WEB WORKER ───────────────────────────────┐
│                                        │  │                                               │
│  ┌─────────────────────────────────┐   │  │  ┌────────────────────────────────────────┐   │
│  │        Monaco Editor            │   │  │  │          lsp-worker.js                 │   │
│  │  ┌───────────────────────────┐  │   │  │  │                                        │   │
│  │  │ Monarch Tokenizer         │  │   │  │  │  ┌──────────┐    ┌──────────────────┐  │   │
│  │  │ (syntax highlighting)     │  │   │  │  │  │  Frame   │    │    Deframe       │  │   │
│  │  └───────────────────────────┘  │   │  │  │  │ Encoder  │    │     Parser       │  │   │
│  │  ┌───────────────────────────┐  │   │  │  │  │          │    │                  │  │   │
│  │  │ LSP Providers             │  │   │  │  │  │ JSON  →  │    │  Content-Length  │  │   │
│  │  │ · HoverProvider           │  │   │  │  │  │ Content- │    │  header  → JSON  │  │   │
│  │  │ · CompletionProvider      │  │   │  │  │  │ Length   │    │                  │  │   │
│  │  │ · Diagnostics (markers)   │  │   │  │  │  │ header   │    │                  │  │   │
│  │  └───────────────────────────┘  │   │  │  │  └────┬─────┘    └────────┬─────────┘  │   │
│  └──────────┬──────────────────────┘   │  │  │       │                   │            │   │
│             │                          │  │  │       ▼                   ▲            │   │
│             │  JSON-RPC 2.0            │  │  │  ┌─────────────────────────────┐       │   │
│             │  postMessage()           │  │  │  │     WASM (Rust)             │       │   │
│             │                          │  │  │  │                             │       │   │
│             │  ┌── request/response ─┐ │  │  │  │  send_to_js()  ◄─channel──  │       │   │
│             │  │     bridge          │ │  │  │  │      ▲                      │       │   │
│             │  │  (Promise map by    │ │  │  │  │      │                      │       │   │
│             │  │   JSON-RPC id)      │ │  │  │  │  ┌───┴──────────────────┐   │       │   │
│             │  └─────────────────────┘ │  │  │  │  │   tower-lsp Server   │   │       │   │
│             │                          │  │  │  │  │   LspService::new()  │   │       │   │
│             ▼                          │  │  │  │  │                      │   │       │   │
│  ┌──────────────────────┐              │  │  │  │  │  ┌────────────────┐  │   │       │   │
│  │  publishDiagnostics  │              │  │  │  │  │  │    Backend     │  │   │       │   │
│  │  → Monaco markers    │              │  │  │  │  │  │                │  │   │       │   │
│  └──────────────────────┘              │  │  │  │  │  │ · initialize   │  │   │       │   │
│                                        │  │  │  │  │  │ · did_open     │  │   │       │   │
│                                        │  │  │  │  │  │ · did_change   │  │   │       │   │
│                                        │  │  │  │  │  │ · completion   │  │   │       │   │
│                                        │  │  │  │  │  │ · hover        │  │   │       │   │
│                                        │  │  │  │  │  └───────┬────────┘  │   │       │   │
│                                        │  │  │  │  │          │           │   │       │   │
│                                        │  │  │  │  │  ┌───────▼────────┐  │   │       │   │
│                                        │  │  │  │  │  │ sysml-v2-parser│  │   │       │   │
│                                        │  │  │  │  │  │ parse_for_edit │  │   │       │   │
│                                        │  │  │  │  │  └────────────────┘  │   │       │   │
│                                        │  │  │  │  └──────────────────────┘   │       │   │
│                                        │  │  │  └─────────────────────────────┘       │   │
│                                        │  │  └────────────────────────────────────────┘   │
└────────────────────────────────────────┘  └───────────────────────────────────────────────┘
```

---

## Message Flow

### Document Updates & Diagnostics (Notification Flow)

When a user edits the document, the change is transmitted asynchronously down to the WebAssembly parser, which returns validation markers:

```
  Monaco                Web Worker              Rust/WASM (tower-lsp)
  ──────                ──────────              ─────────────────────
     │                      │                           │
     │  didChange (JSON)    │                           │
     │─────────────────────►│                           │
     │                      │                           │
     │                      │  frame + Content-Length   │
     │                      │  header bytes             │
     │                      │──────────────────────────►│
     │                      │                           │
     │                      │                    ┌──────┴──────┐
     │                      │                    │ LspReader   │
     │                      │                    │ poll_read() │
     │                      │                    │ reads frame │
     │                      │                    └──────┬──────┘
     │                      │                           │
     │                      │                    ┌──────┴──────┐
     │                      │                    │ Backend     │
     │                      │                    │ did_change()│
     │                      │                    │   → parse   │
     │                      │                    │   → publish │
     │                      │                    │ diagnostics │
     │                      │                    └──────┬──────┘
     │                      │                           │
     │                      │                    ┌──────┴──────┐
     │                      │                    │ LspWriter   │
     │                      │                    │ poll_write()│
     │                      │                    │ → send_to_js│
     │                      │                    └──────┬──────┘
     │                      │                           │
     │                      │  framed bytes             │
     │                      │◄──────────────────────────│
     │                      │                           │
     │                      │  deframe (parse header)   │
     │                      │                           │
     │ publishDiagnostics   │                           │
     │◄─────────────────────│                           │
     │                      │                           │
  ┌──┴──────────┐           │                           │
  │ setModel    │           │                           │
  │ Markers()   │           │                           │
  └─────────────┘           │                           │
```

### Request/Response Flow (Completions & Hover)

For features requiring direct replies, a promise-based transaction registry tracks requests by their JSON-RPC transaction ID:

```
  Monaco                Web Worker              Rust/WASM
  ──────                ──────────              ─────────
     │                      │                      │
     │  completion (JSON)   │                      │
     │  id: 3 ─────────────►│                      │
     │                      │  framed bytes ──────►│
     │                      │                      │
     │              Promise │                      │
     │              pending │  ◄── framed bytes ───│
     │                      │  (id: 3, result)     │
     │  resolve(id: 3) ◄────│                      │
     │                      │                      │
  ┌──┴──────────────────┐   │                      │
  │ suggestions show up │   │                      │
  │ in editor dropdown  │   │                      │
  └─────────────────────┘   │                      │
```

---

## Technical Details

### 1. The LSP Server
The core server is implemented in `src/lib.rs` using `tower-lsp`. Because compilation targets the browser environment (`wasm32-unknown-unknown`), standard asynchronous runtimes (like Tokio) are not compatible. The server is configured in `runtime-agnostic` mode, relying on `wasm-bindgen-futures` for execution.

### 2. WASM I/O Bridging
Standard LSP frameworks expect system-level I/O streams (typically standard input/output). To accommodate this in a browser environment, two custom structures bridge JavaScript message passing with Rust's asynchronous streams:

*   **`LspReader` (implements `AsyncRead`):** Polls an internal futures `mpsc` channel. This channel is populated by external JavaScript calls to the exported `handle_message_from_js()` function.
*   **`LspWriter` (implements `AsyncWrite`):** Converts output stream bytes into calls to a global JavaScript handler, `send_to_js()`.

### 3. Protocol Framing
LSP messages require structural framing based on the `Content-Length` header standard. To keep the compiled WASM binary smaller, framing/deframing operations are split:
- **JS-side (Worker):** Receives raw JSON-RPC messages from the main thread, appends the appropriate `Content-Length` headers, and converts them to byte arrays before invoking the WASM bridge. It also decodes stream bytes emitted from Rust back into JSON structures.
- **Rust-side:** Directly processes standard bytes streams, allowing `tower-lsp` to parse protocol headers naturally without writing custom serialization logic.

---

## Project Structure

```
├── Cargo.toml                  # Rust dependency and WASM build settings
├── src/
│   └── lib.rs                  # LSP Server implementation and WASM bindings
├── resources/
│   └── index.html              # Frontend source containing Monaco Editor configuration
├── dist/
│   ├── index.html              # Served frontend template
│   ├── lsp-worker.js           # Worker file managing framing and WASM imports
│   └── pkg/                    # wasm-pack build outputs
│       ├── sysmlv2_web_editor.js      # Generated JS bindings
│       ├── sysmlv2_web_editor_bg.wasm # Compiled WASM payload
│       └── ...
└── server.js                   # Minimal Bun static asset server
```

---

## Installation & Running

### Prerequisites

To build and run the project locally, ensure you have the following tools installed:

*   [Rust](https://rustup.rs) (with `wasm32-unknown-unknown` target added via `rustup target add wasm32-unknown-unknown`)
*   [wasm-pack](https://rustwasm.github.io/wasm-pack/)
*   [Bun](https://bun.sh) (or Node.js)

### Build Steps

1. **Compile the Rust codebase to WASM:**
   ```bash
   wasm-pack build --target web --out-dir dist/pkg
   ```

2. **Prepare the static web assets:**
   ```bash
   mkdir -p dist
   cp resources/index.html dist/
   ```

### Execution

Start the development server using Bun:

```bash
bun run server.js
```

Once started, navigate to `http://localhost:3000` in your browser.

---

## Feature Matrix & Implementation Status

The table below describes the current implementation coverage of typical LSP features:

| Feature | Standard LSP Method | Status | Notes |
| :--- | :--- | :--- | :--- |
| **Open Validation** | `textDocument/didOpen` | ✅ | Validates AST upon document load |
| **Live Diagnostics** | `textDocument/didChange` | ✅ | Re-evaluates diagnostics during document edits |
| **Diagnostics Publishing**| `textDocument/publishDiagnostics`| ✅ | Sends syntax errors to Monaco markers |
| **Completions** | `textDocument/completion` | ✅ | Provides keyword and context completions |
| **Hover Tooltips** | `textDocument/hover` | ✅ | Displays documentation on type/keyword hover |
| **Go To Definition** | `textDocument/definition` | ❌ | Planned |
| **Document Symbols** | `textDocument/documentSymbol` | ❌ | Planned |
| **Formatting** | `textDocument/formatting` | ❌ | Planned |