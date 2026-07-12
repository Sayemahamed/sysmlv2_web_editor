import init, { start_lsp, handle_message_from_js } from './pkg/sysmlv2_web_editor.js';

let wasmReady = false;
const messageQueue = [];

// Initialize the WebAssembly binary
init().then(() => {
    start_lsp();
    wasmReady = true;

    // Drain any messages queued while WASM was loading
    while (messageQueue.length > 0) {
        const bytes = messageQueue.shift();
        handle_message_from_js(bytes);
    }
});

// Converts a JSON payload to a framed LSP buffer containing 'Content-Length' headers
function toLspFrame(jsonPayload) {
    const jsonStr = JSON.stringify(jsonPayload);
    const blob = new TextEncoder().encode(jsonStr);
    const headers = `Content-Length: ${blob.byteLength}\r\n\r\n`;
    const headerBlob = new TextEncoder().encode(headers);

    const frame = new Uint8Array(headerBlob.byteLength + blob.byteLength);
    frame.set(headerBlob, 0);
    frame.set(blob, headerBlob.byteLength);
    return frame;
}

// Receives JSON-RPC from Monaco and transmits to Rust WASM
self.onmessage = (event) => {
    const jsonPayload = event.data;
    const bytes = toLspFrame(jsonPayload);

    if (wasmReady) {
        queueMicrotask(() => {
            handle_message_from_js(bytes);
        });
    } else {
        // Buffer messages if WASM is still compiling
        messageQueue.push(bytes);
    }
};

// Internal buffer to parse stream outputs from Rust
let buffer = new Uint8Array(0);

function handleBytesFromRust(bytes) {
    const newBuf = new Uint8Array(buffer.length + bytes.length);
    newBuf.set(buffer);
    newBuf.set(bytes, buffer.length);
    buffer = newBuf;

    while (true) {
        const str = new TextDecoder().decode(buffer);
        const headerMatch = str.match(/^Content-Length: (\d+)\r\n\r\n/);
        if (!headerMatch) break;

        const contentLength = parseInt(headerMatch[1], 10);
        const headerLength = headerMatch[0].length;

        if (buffer.length < headerLength + contentLength) {
            break; // Awaiting more chunks
        }

        const jsonBytes = buffer.slice(headerLength, headerLength + contentLength);
        const jsonStr = new TextDecoder().decode(jsonBytes);
        const json = JSON.parse(jsonStr);

        // Forward decoded JSON back to Monaco Editor on the main thread
        self.postMessage(json);

        buffer = buffer.slice(headerLength + contentLength);
    }
}

// Map the custom global function used by Rust's AsyncWrite loop
self.send_to_js = function (data) {
    const ownedBytes = data.slice();

    queueMicrotask(() => {
        handleBytesFromRust(ownedBytes);
    });
};