# SysML v2 Browser IDE

A fully browser-based SysML v2 code editor with live diagnostics, completions, and hover — powered by a **standard LSP protocol server** compiled to WebAssembly via Rust.

No backend. No network requests. The entire LSP stack runs in the browser.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│ Main Thread                                                      │
│ ┌──────────────────┐      postMessage      ┌──────────────────┐ │
│ │  Monaco Editor   │ ◄──────────────────► │  Web Worker       │ │
│ │  (sysml lang)    │   (JSON-RPC 2.0)      │  (lsp-worker.js)  │ │
│ └──────────────────┘                       └────────┬─────────┘ │
│                                                     │             │
└─────────────────────────────────────────────────────┼─────────────┘
                                                      │
                                          ┌───────────▼───────────┐
                                          │  WASM (Rust compiled)  │
                                          │  ┌───────────────────┐ │
                                          │  │  tower-lsp Server │ │
                                          │  │  (std LSP impl)   │ │
                                          │  └────────┬──────────┘ │
                                          │           │             │
                                          │  ┌────────▼──────────┐ │
                                          │  │ sysml-v2-parser   │ │
                                          │  │ (Rust crate)      │ │
                                          │  └───────────────────┘ │
                                          └────────────────────────┘
```

## How It Works

### 1. Standard LSP Protocol (tower-lsp)

The Rust backend (`src/lib.rs`) implements a fully standard [Language Server Protocol](https://microsoft.github.io/language-server-protocol/) server using the `tower-lsp` crate. It supports:

- **`textDocument/didOpen`** — validates a file on open
- **`textDocument/didChange`** — re-validates on every keystroke (full sync)
- **`textDocument/completion`** — keyword + identifier completions
- **`textDocument/hover`** — hover information at any position
- **`textDocument/publishDiagnostics`** — parse errors from `sysml-v2-parser`

The server uses `runtime-agnostic` mode, meaning it doesn't need Tokio or any async runtime beyond what `wasm-bindgen-futures` provides in the browser.

### 2. WebAssembly I/O Bridge

tower-lsp needs byte-level `AsyncRead`/`AsyncWrite` streams to read/write LSP frames. In a native Rust binary this would be stdin/stdout. In the browser, two custom structs provide this:

- **`LspReader`** — implements `AsyncRead` over a futures `mpsc` channel. Bytes arrive from JavaScript via `handle_message_from_js()` → channel → polled by tower-lsp.
- **`LspWriter`** — implements `AsyncWrite`, calling a `send_to_js` JavaScript function for every write. This function is exposed from Rust with `#[wasm_bindgen]` and implemented in the Web Worker.

### 3. LSP Framing (Content-Length Headers)

The standard LSP transport uses `Content-Length` headers to delimit messages:

```
Content-Length: 42\r\n\r\n{"jsonrpc":"2.0","method":"initialize",...}
```

This framing happens in JavaScript (`lsp-worker.js`), not in Rust:

- **JS → Rust**: Each JSON-RPC message from Monaco is prefixed with `Content-Length: N\r\n\r\n` before being pushed into the Rust reader.
- **Rust → JS**: The Rust writer outputs framed bytes. The JS side parses the headers, extracts the JSON payload, and forwards it via `postMessage` to the main thread.

### 4. Web Worker Transport

All LSP traffic runs inside a **Web Worker** (`dist/lsp-worker.js`). This keeps the WASM compilation and parser execution off the main thread, preventing UI jank. Communication between Monaco (main thread) and the worker uses `postMessage` with JSON-RPC 2.0 payloads.

### 5. Monaco Editor Integration

On the main thread (`resources/index.html`):

- A custom `sysml` language is registered with Monarch tokenizer rules for syntax highlighting.
- Three LSP features are wired to Monaco providers:
  - **Diagnostics** — `textDocument/publishDiagnostics` results become Monaco markers
  - **Hover** — `textDocument/hover` responses display as Monaco hover widgets
  - **Completions** — `textDocument/completion` results populate the suggestion list, triggered on `:`, `;`, `.`

A request/response bridge maps JSON-RPC IDs to pending promises, allowing Monaco providers (which return promises) to await LSP responses from the worker.

## Project Structure

```
├── Cargo.toml              # Rust project manifest
├── src/
│   └── lib.rs              # LSP server implementation (Backend)
├── resources/
│   └── index.html          # Monaco Editor + provider registrations
├── dist/
│   ├── index.html          # Built frontend (served by Bun)
│   ├── lsp-worker.js       # Web Worker: WASM bridge + LSP framing
│   └── pkg/                # wasm-pack build output
│       ├── sysmlv2_web_editor.js      # JS glue (wasm-bindgen)
│       ├── sysmlv2_web_editor_bg.wasm # compiled WASM binary
│       └── ...
└── server.js               # Bun static file server (port 3000)
```

## Building & Running

### Prerequisites

- [Rust](https://rustup.rs) with the `wasm32-unknown-unknown` target
- [wasm-pack](https://rustwasm.github.io/wasm-pack/)
- [Bun](https://bun.sh) (for the dev server)

### Build

```bash
# Compile Rust to WebAssembly
wasm-pack build --target web --out-dir dist/pkg

# Copy the frontend resources
cp resources/index.html dist/

# (lsp-worker.js is maintained directly in dist/)
```

### Run

```bash
bun run server.js
# Open http://localhost:3000
```

## LSP Capability Matrix

| Feature | Standard LSP Method | Implemented |
|---|---|---|
| Document open validation | `textDocument/didOpen` | ✅ |
| Live diagnostics on edit | `textDocument/didChange` | ✅ |
| Publish diagnostics | `textDocument/publishDiagnostics` | ✅ |
| Code completions | `textDocument/completion` | ✅ |
| Hover information | `textDocument/hover` | ✅ |
| Go to definition | `textDocument/definition` | ❌ |
| Document symbols | `textDocument/documentSymbol` | ❌ |
| Formatting | `textDocument/formatting` | ❌ |
