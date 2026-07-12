# SysML v2 Browser IDE

A fully browser-based SysML v2 code editor with live diagnostics, completions, and hover — powered by a **standard LSP protocol server** compiled to WebAssembly via Rust.

No backend. No network requests. The entire LSP stack runs in the browser.

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

## Message Flow

A keystroke in the editor triggers this full round-trip:

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

### Request/Response Flow (completion, hover)

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

## How It Works

### LSP Protocol (tower-lsp)

The Rust backend (`src/lib.rs`) implements a fully standard [Language Server Protocol](https://microsoft.github.io/language-server-protocol/) server using `tower-lsp` in `runtime-agnostic` mode — no Tokio needed, just `wasm-bindgen-futures`.

### WASM I/O Bridge

tower-lsp expects byte-level `AsyncRead`/`AsyncWrite` streams (stdin/stdout in a native binary). Two custom structs bridge this to the browser:

| Component | Trait | Mechanism |
|---|---|---|
| **`LspReader`** | `AsyncRead` | Polls a futures `mpsc` channel fed by `handle_message_from_js()` |
| **`LspWriter`** | `AsyncWrite` | Calls the JS global `send_to_js()` on every write |

### LSP Framing

Messages use standard `Content-Length` header framing. JavaScript handles encoding/decoding — Rust only sees raw bytes:

```
Content-Length: 42\r\n\r\n{"jsonrpc":"2.0","method":"initialize",...}
```

- **JS → Rust**: JSON-RPC is framed with headers, then pushed into the `mpsc` channel
- **Rust → JS**: tower-lsp writes framed bytes → `send_to_js()` → JS deframer extracts JSON → `postMessage()` to main thread

### Request/Response Bridge

Monaco providers (hover, completion) are promise-based. The main thread maintains a `Map<requestId, resolveFn>`. When the worker posts back a response with a matching `id`, the promise resolves and the provider returns its result.

### Web Worker

All WASM runs inside a dedicated Web Worker — parsing and LSP logic never block the main thread.

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
