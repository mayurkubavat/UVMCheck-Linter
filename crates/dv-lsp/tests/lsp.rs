//! Drives the dv-lsp binary over stdio on a small UVM workspace.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const PKG: &str = r#"package my_pkg;
  import uvm_pkg::*;
  // A bus transaction.
  class my_item extends uvm_sequence_item;
    rand bit [7:0] addr; // target address
    function void show(); endfunction
    extern task send(int n);
  endclass
  task my_item::send(int n); endtask
  class my_drv extends uvm_driver #(my_item);
    my_item item;
    task run_phase(uvm_phase phase);
      my_item local_it;
      item.show();
      local_it.addr = 1;
      this.item.send(2);
      `my_macro(x)
      item.
    endtask
  endclass
endpackage
"#;

const BASE: &str = r#"package uvm_pkg;
class uvm_object;
  // The instance name.
  function string get_name(); endfunction
endclass
class uvm_sequence_item extends uvm_object;
endclass
class uvm_driver #(type REQ = int) extends uvm_object;
  REQ req;
endclass
endpackage
"#;

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u32,
}

impl Client {
    fn start(root: &Path) -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_dv-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut c = Client { child, stdin, stdout, next: 1 };
        let root_uri = format!("file://{}", root.display());
        c.request("initialize", &format!(r#"{{"rootUri":"{}","capabilities":{{}}}}"#, root_uri));
        c.notify("initialized", "{}");
        c
    }

    fn send(&mut self, body: &str) {
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str, params: &str) {
        self.send(&format!(r#"{{"jsonrpc":"2.0","method":"{}","params":{}}}"#, method, params));
    }

    fn read(&mut self) -> String {
        let mut len = 0;
        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line).unwrap();
            let l = line.trim();
            if l.is_empty() {
                break;
            }
            if let Some(v) = l.strip_prefix("Content-Length:") {
                len = v.trim().parse().unwrap();
            }
        }
        let mut buf = vec![0; len];
        self.stdout.read_exact(&mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    /// Sends a request and returns its response, skipping notifications.
    fn request(&mut self, method: &str, params: &str) -> String {
        let id = self.next;
        self.next += 1;
        self.send(&format!(r#"{{"jsonrpc":"2.0","id":{},"method":"{}","params":{}}}"#, id, method, params));
        loop {
            let m = self.read();
            if m.contains(&format!(r#""id":{}"#, id)) {
                return m;
            }
        }
    }

    fn at(&mut self, method: &str, file: &Path, line: u32, col: u32) -> String {
        self.request(
            method,
            &format!(
                r#"{{"textDocument":{{"uri":"file://{}"}},"position":{{"line":{},"character":{}}}}}"#,
                file.display(),
                line,
                col
            ),
        )
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn workspace() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dv-lsp-test-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("pkg.sv"), PKG).unwrap();
    std::fs::write(dir.join("lib/uvm_base.sv"), BASE).unwrap();
    std::fs::write(dir.join("macros.svh"), "// Prints its argument.\n`define my_macro(a) $display(a)\n").unwrap();
    dir
}

/// Line and column of the `nth` occurrence of `needle` (plus `delta` columns).
fn pos(text: &str, needle: &str, nth: usize, delta: u32) -> (u32, u32) {
    let off = text.match_indices(needle).nth(nth).unwrap_or_else(|| panic!("{needle}")).0;
    let line = text[..off].matches('\n').count() as u32;
    let col = (off - text[..off].rfind('\n').map(|i| i + 1).unwrap_or(0)) as u32;
    (line, col + delta)
}

fn line_of(resp: &str) -> Vec<u32> {
    // "range":{"start":{"line":N
    resp.match_indices(r#""range":{"start":{"line":"#)
        .map(|(i, m)| resp[i + m.len()..].split(|c: char| !c.is_ascii_digit()).next().unwrap().parse().unwrap())
        .collect()
}

#[test]
fn navigation_and_completion() {
    let dir = workspace();
    let pkg = dir.join("pkg.sv");
    let mut c = Client::start(&dir);

    // item.show() -> show's declaration
    let (l, k) = pos(PKG, "item.show", 0, 6);
    let r = c.at("textDocument/definition", &pkg, l, k);
    assert_eq!(line_of(&r), vec![pos(PKG, "void show", 0, 0).0], "{r}");

    // local_it.addr -> addr in my_item
    let (l, k) = pos(PKG, "local_it.addr", 0, 10);
    let r = c.at("textDocument/definition", &pkg, l, k);
    assert_eq!(line_of(&r), vec![4], "{r}");

    // this.item.send -> prototype and out-of-block body
    let (l, k) = pos(PKG, "item.send", 0, 6);
    let r = c.at("textDocument/definition", &pkg, l, k);
    assert_eq!(line_of(&r), vec![6, 8], "{r}");

    // A macro in another file
    let (l, k) = pos(PKG, "`my_macro", 0, 2);
    let r = c.at("textDocument/definition", &pkg, l, k);
    assert!(r.contains("macros.svh"), "{r}");

    // A base class in a library file
    let (l, k) = pos(PKG, "uvm_sequence_item", 0, 3);
    let r = c.at("textDocument/definition", &pkg, l, k);
    assert!(r.contains("uvm_base.sv"), "{r}");

    // Hover shows the declaration and its trailing comment
    let (l, k) = pos(PKG, "local_it.addr", 0, 10);
    let r = c.at("textDocument/hover", &pkg, l, k);
    assert!(r.contains("target address") && r.contains("rand bit [7:0] addr;"), "{r}");

    // Completion after `item.`: own and inherited members
    let (l, k) = pos(PKG, "item.\n", 0, 5);
    let r = c.at("textDocument/completion", &pkg, l, k);
    for m in ["\"show\"", "\"addr\"", "\"send\"", "\"get_name\""] {
        assert!(r.contains(m), "{m} missing: {r}");
    }
    assert!(!r.contains("\"run_phase\""), "{r}");

    // Completion of plain names sees locals and members
    let (l, k) = pos(PKG, "local_it.addr", 0, 0);
    let r = c.at("textDocument/completion", &pkg, l, k);
    assert!(r.contains("\"local_it\"") && r.contains("\"item\"") && r.contains("\"get_name\""), "{r}");
    let (l, k) = pos(PKG, "local_it.addr", 0, 3);
    let r = c.at("textDocument/completion", &pkg, l, k);
    assert!(r.contains("\"local_it\"") && !r.contains("\"item\""), "{r}");

    // Backtick completion lists workspace macros
    let (l, k) = pos(PKG, "`my_macro", 0, 1);
    let r = c.at("textDocument/completion", &pkg, l, k);
    assert!(r.contains("\"my_macro\"") && r.contains("\"uvm_info\""), "{r}");

    // References to my_item: class, uses, scope prefix
    let (l, k) = pos(PKG, "class my_item", 0, 7);
    let r = c.at("textDocument/references", &pkg, l, k);
    assert_eq!(line_of(&r).len(), 5, "{r}");

    // Outline: package > classes > members
    let r = c
        .request("textDocument/documentSymbol", &format!(r#"{{"textDocument":{{"uri":"file://{}"}}}}"#, pkg.display()));
    assert!(
        r.starts_with(r#"{"jsonrpc":"2.0","id""#)
            && r.contains(r#""name":"my_pkg""#)
            && r.contains(r#""name":"my_item::send""#),
        "{r}"
    );
    assert!(!r.contains("local_it"), "{r}");

    // Workspace symbols
    let r = c.request("workspace/symbol", r#"{"query":"drv"}"#);
    assert!(r.contains(r#""name":"my_drv""#), "{r}");

    // Edits are seen without saving
    let edited = PKG.replace("function void show();", "function void show2();");
    let body = format!(
        r#"{{"textDocument":{{"uri":"file://{}","version":2}},"contentChanges":[{{"text":{}}}]}}"#,
        pkg.display(),
        json_str(&edited)
    );
    c.notify("textDocument/didChange", &body);
    let r = c.request("workspace/symbol", r#"{"query":"show2"}"#);
    assert!(r.contains(r#""name":"show2""#), "{r}");

    let r = c.request("shutdown", "null");
    assert!(r.contains(r#""result":null"#));
    c.notify("exit", "null");
    let status = c.child.wait().unwrap();
    assert!(status.success());
    let _ = std::fs::remove_dir_all(&dir);
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}
