#!/usr/bin/env python3
"""
Integration test for the UVMCheck language server (uvmcheck --lsp).

1. Standalone: a buffer with no project config is linted on its own
   (multi-line text, real ranges, diagnostics cleared on close).
2. Project: a .uvmcheck/config.json names the sources and include dirs; an
   include-only file open in the editor is linted inside the full compile, so
   a rule that needs the UVM base classes declared elsewhere fires. The
   "waive" code action's edit, applied to the buffer, makes the finding go.
"""

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def encode(obj):
    content = json.dumps(obj).encode("utf-8")
    return f"Content-Length: {len(content)}\r\n\r\n".encode("utf-8") + content


class Client:
    def __init__(self, binary, cwd):
        self.proc = subprocess.Popen([binary, "--lsp"], stdin=subprocess.PIPE,
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd=cwd)
        self.next_id = 1

    def send(self, obj):
        self.proc.stdin.write(encode(obj))
        self.proc.stdin.flush()

    def read(self):
        length = None
        while True:
            line = self.proc.stdout.readline()
            if not line:
                return None
            line = line.decode("utf-8").strip()
            if not line:
                break
            if line.lower().startswith("content-length:"):
                length = int(line.split(":")[1])
        return json.loads(self.proc.stdout.read(length).decode("utf-8"))

    def request(self, method, params=None):
        rid = self.next_id
        self.next_id += 1
        self.send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params or {}})
        while True:
            msg = self.read()
            assert msg is not None, f"no response to {method}"
            if msg.get("id") == rid:
                return msg

    def notify(self, method, params):
        self.send({"jsonrpc": "2.0", "method": method, "params": params})

    def diagnostics_for(self, uri):
        """Reads publishDiagnostics notifications until one arrives for `uri`."""
        while True:
            msg = self.read()
            if msg is None:
                err = self.proc.stderr.read().decode("utf-8", "replace") if self.proc.poll() is not None else ""
                raise AssertionError(f"no diagnostics for {uri}; server stderr:\n{err[-2000:]}")
            if msg.get("method") == "textDocument/publishDiagnostics" and msg["params"]["uri"] == uri:
                return msg["params"]["diagnostics"]

    def close(self):
        resp = self.request("shutdown")
        assert "result" in resp
        self.notify("exit", {})
        assert self.proc.wait(timeout=5) == 0, "server did not exit cleanly"


def open_doc(client, uri, text, version=1):
    client.notify("textDocument/didOpen", {"textDocument": {
        "uri": uri, "languageId": "systemverilog", "version": version, "text": text}})


def apply_edit(text, edit):
    lines = text.split("\n")
    start, end = edit["range"]["start"], edit["range"]["end"]
    def offset(pos):
        return sum(len(l) + 1 for l in lines[:pos["line"]]) + pos["character"]
    return text[:offset(start)] + edit["newText"] + text[offset(end):]


def test_standalone(binary, workdir):
    client = Client(binary, workdir)
    try:
        init = client.request("initialize", {"processId": os.getpid(), "rootUri": None, "capabilities": {}})
        caps = init["result"]["capabilities"]
        assert caps["textDocumentSync"]["change"] == 1, "expected full text sync"
        print("  [PASS] initialize handshake")

        text = "module bad_timescale;\n  logic clk;\n  initial #10 clk = 1;\nendmodule\n"
        uri = Path(workdir, "test.sv").as_uri()
        open_doc(client, uri, text)
        diags = client.diagnostics_for(uri)
        tim = [d for d in diags if d["code"] == "SV-TIM-002"]
        assert tim, f"expected SV-TIM-002, got {[d['code'] for d in diags]}"
        # Multi-line text must arrive intact: the delay is on the third line.
        assert tim[0]["range"]["start"]["line"] == 2, tim[0]
        for d in diags:
            r = d["range"]
            assert r["end"]["character"] > r["start"]["character"], f"empty range: {d}"
            assert d["source"] == "uvmcheck"
        print(f"  [PASS] didOpen: {len(diags)} diagnostics, multi-line text, real ranges")

        client.notify("textDocument/didClose", {"textDocument": {"uri": uri}})
        assert client.diagnostics_for(uri) == [], "expected empty diagnostics on didClose"
        print("  [PASS] diagnostics cleared on didClose")
        client.close()
        print("  [PASS] shutdown / exit")
    finally:
        if client.proc.poll() is None:
            client.proc.kill()


ITEMS = """class packet_item extends uvm_sequence_item;
  `uvm_object_utils(packet_item)
  function new(string name = "packet_item"); super.new(name); endfunction
endclass

class bad_packet_item extends packet_item;
  `uvm_object_utils(bad_packet_item)
  function new(string name = "bad_packet_item"); super.new(name); endfunction
endclass
"""

DRIVER = """class my_driver extends uvm_driver#(packet_item);
  `uvm_component_utils(my_driver)
  function new(string name, uvm_component parent); super.new(name, parent); endfunction

  virtual function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    set_type_override("packet_item", "bad_packet_item");
  endfunction
endclass
"""

PKG = '`include "uvm_mock.svh"\n`include "items.svh"\n`include "driver.svh"\n'


def test_project(binary, workdir):
    proj = Path(workdir, "proj")
    (proj / ".uvmcheck").mkdir(parents=True)
    (proj / "items.svh").write_text(ITEMS)
    (proj / "driver.svh").write_text(DRIVER)
    (proj / "tb_pkg.sv").write_text(PKG)
    (proj / ".uvmcheck" / "config.json").write_text(json.dumps({
        "sources": ["tb_pkg.sv"],
        "include_dirs": [".", str(ROOT / "tests" / "common")],
    }))

    # Start outside the project: the config is found from the workspace root.
    elsewhere = Path(workdir, "elsewhere")
    elsewhere.mkdir()
    client = Client(binary, str(elsewhere))
    try:
        client.request("initialize", {"processId": os.getpid(), "rootUri": proj.as_uri(), "capabilities": {}})

        uri = (proj / "driver.svh").as_uri()
        open_doc(client, uri, DRIVER)
        diags = client.diagnostics_for(uri)
        fac = [d for d in diags if d["code"] == "UVM-FAC-005"]
        assert fac, f"expected UVM-FAC-005 from the project compile, got {[d['code'] for d in diags]}"
        assert fac[0]["range"]["start"]["line"] == 6, fac[0]
        print("  [PASS] include-only file linted inside the project compile (UVM-FAC-005)")

        # Relinting the same text gives the same findings (no state kept between runs).
        client.notify("textDocument/didChange", {"textDocument": {"uri": uri, "version": 2},
                                                 "contentChanges": [{"text": DRIVER}]})
        again = client.diagnostics_for(uri)
        assert again == diags, "relint of unchanged text changed the findings"
        print("  [PASS] relint is stable")

        # Unsaved edits are what gets linted, not the file on disk.
        edited = DRIVER.replace('    set_type_override("packet_item", "bad_packet_item");\n', "")
        client.notify("textDocument/didChange", {"textDocument": {"uri": uri, "version": 2},
                                                 "contentChanges": [{"text": edited}]})
        assert not [d for d in client.diagnostics_for(uri) if d["code"] == "UVM-FAC-005"]
        client.notify("textDocument/didChange", {"textDocument": {"uri": uri, "version": 3},
                                                 "contentChanges": [{"text": DRIVER}]})
        assert [d for d in client.diagnostics_for(uri) if d["code"] == "UVM-FAC-005"]
        print("  [PASS] didChange lints the editor buffer")

        resp = client.request("textDocument/codeAction", {
            "textDocument": {"uri": uri},
            "range": fac[0]["range"],
            "context": {"diagnostics": [fac[0]]}})
        actions = resp["result"]
        waive = [a for a in actions if a["title"].startswith("Waive UVM-FAC-005")]
        assert waive, f"expected a waive action, got {[a['title'] for a in actions]}"
        edit = waive[0]["edit"]["changes"][uri][0]
        assert "uvmcheck:waive-next-line UVM-FAC-005" in edit["newText"]
        assert edit["newText"].startswith("    //"), "waiver comment should keep the line's indent"
        waived = apply_edit(DRIVER, edit)
        client.notify("textDocument/didChange", {"textDocument": {"uri": uri, "version": 4},
                                                 "contentChanges": [{"text": waived}]})
        assert not [d for d in client.diagnostics_for(uri) if d["code"] == "UVM-FAC-005"], \
            "finding should be waived after applying the code action"
        print("  [PASS] waive code action removes the finding")
        client.close()
    finally:
        if client.proc.poll() is None:
            client.proc.kill()


def main():
    binary = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else str(ROOT / "build" / "uvmcheck"))
    if not os.path.isfile(binary):
        print(f"Error: binary {binary} not found.")
        sys.exit(1)
    print("[LSP TEST] uvmcheck --lsp")
    with tempfile.TemporaryDirectory() as tmp:
        standalone = Path(tmp, "standalone")
        standalone.mkdir()
        test_standalone(binary, str(standalone))
        test_project(binary, tmp)
    print("All LSP checks passed.")


if __name__ == "__main__":
    main()
