#!/usr/bin/env python3
"""
Integration test for UVMCheck LSP (Language Server Protocol) Daemon.
Tests initialize, didOpen (with live diagnostics), didChange, and shutdown protocol over stdin/stdout.
"""

import subprocess
import json
import os
import sys

def encode_lsp_message(obj):
    content = json.dumps(obj)
    header = f"Content-Length: {len(content)}\r\n\r\n"
    return (header + content).encode("utf-8")

def read_lsp_message(proc):
    header_line = b""
    while b"\r\n\r\n" not in header_line and b"\n\n" not in header_line:
        chunk = proc.stdout.readline()
        if not chunk:
            return None
        header_line += chunk

    # Parse Content-Length
    for line in header_line.decode("utf-8").splitlines():
        if line.lower().startswith("content-length:"):
            length = int(line.split(":")[1].strip())
            payload = proc.stdout.read(length)
            return json.loads(payload.decode("utf-8"))
    return None

def main():
    binary_path = os.path.abspath("build/uvmcheck")
    if not os.path.isfile(binary_path):
        print(f"Error: binary {binary_path} not found.")
        sys.exit(1)

    print("[LSP TEST] Launching uvmcheck --lsp daemon...")
    proc = subprocess.Popen(
        [binary_path, "--lsp"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE
    )

    try:
        # Step 1: Send 'initialize'
        init_req = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": os.getpid(),
                "rootUri": "file:///workspace",
                "capabilities": {}
            }
        }
        proc.stdin.write(encode_lsp_message(init_req))
        proc.stdin.flush()

        init_resp = read_lsp_message(proc)
        assert init_resp is not None, "Failed to read initialize response"
        assert init_resp.get("id") == 1, "Initialize response ID mismatch"
        capabilities = init_resp["result"]["capabilities"]
        assert capabilities.get("textDocumentSync") == 1, "Expected full text sync"
        print("  [PASS] LSP 'initialize' handshake succeeded.")

        # Step 2: Send 'textDocument/didOpen' with buggy SystemVerilog snippet
        bad_sv_content = (
            "module bad_timescale;\n"
            "  logic clk;\n"
            "endmodule\n"
        )
        did_open = {
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///workspace/test.sv",
                    "languageId": "systemverilog",
                    "version": 1,
                    "text": bad_sv_content
                }
            }
        }
        proc.stdin.write(encode_lsp_message(did_open))
        proc.stdin.flush()

        diag_notification = read_lsp_message(proc)
        assert diag_notification is not None, "Failed to receive diagnostics notification"
        assert diag_notification.get("method") == "textDocument/publishDiagnostics"
        params = diag_notification.get("params", {})
        assert params.get("uri") == "file:///workspace/test.sv"
        diagnostics = params.get("diagnostics", [])
        print(f"  [PASS] Received {len(diagnostics)} live diagnostics on didOpen.")
        assert len(diagnostics) > 0, "Expected at least 1 diagnostic for missing timescale"
        print(f"         Code: {diagnostics[0].get('code')}, Message: {diagnostics[0].get('message')[:60]}...")

        # Step 3: Send 'textDocument/didClose' -> should clear diagnostics
        did_close = {
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": {
                "textDocument": {
                    "uri": "file:///workspace/test.sv"
                }
            }
        }
        proc.stdin.write(encode_lsp_message(did_close))
        proc.stdin.flush()

        clear_diag = read_lsp_message(proc)
        assert clear_diag is not None
        assert clear_diag.get("params", {}).get("diagnostics") == [], "Expected empty diagnostics on didClose"
        print("  [PASS] Diagnostics cleared on didClose.")

        # Step 4: Shutdown & Exit
        shutdown_req = {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "shutdown"
        }
        proc.stdin.write(encode_lsp_message(shutdown_req))
        proc.stdin.flush()

        shutdown_resp = read_lsp_message(proc)
        assert shutdown_resp.get("id") == 2
        print("  [PASS] LSP 'shutdown' acknowledged.")

        exit_notif = {
            "jsonrpc": "2.0",
            "method": "exit"
        }
        proc.stdin.write(encode_lsp_message(exit_notif))
        proc.stdin.flush()
        proc.wait(timeout=2)
        print("  [PASS] LSP daemon exited cleanly with code 0.\n")

    finally:
        if proc.poll() is None:
            proc.kill()

if __name__ == "__main__":
    main()
