# UVMCheck Extension for VS Code, Cursor & Antigravity

<img src="images/uvmcheck.png" width="96" alt="UVMCheck logo">

Real-time, zero-latency SystemVerilog and UVM static analysis in your editor powered by **UVMCheck** and **Slang**.

## Features

- **Real-Time Diagnostics**: Instant feedback on 330+ rules covering UVM methodology, OOP soundness, race conditions, TLM connectivity, and dead code.
- **Deep Semantic Analysis**: Cross-component validation of objection CFGs, Config DB type matching, virtual sequencer wiring, and RAL predictor binding.
- **Incremental Linting**: Live diagnostics as you type over Language Server Protocol (`uvmcheck --lsp`).
- **Project aware**: Compiles the project from `.uvmcheck/config.py` (sources, file lists, include dirs, defines) with your unsaved edits, and applies its waivers and baseline.
- **Quick fixes**: Apply a rule's fix-it, or waive a finding on its line with a reason.
- **Code browsing** (`dv-lsp`, the Rust native front end): outline, workspace symbols (Ctrl/Cmd+T), go to definition / declaration / type definition (including `extern` prototype and out-of-block body pairs, macros and base classes), find references, highlights, and hover with the declaration and its doc comment.
- **Completion**: names in scope with inherited class members, `obj.` and `Class::`/`pkg::` members, `type_id::create`, `` ` `` macros (workspace and UVM) and `$` system tasks.
- **Syntax highlighting**: Built-in SystemVerilog/Verilog grammar with UVM classes, macros, phases and verbosity constants; no other SV extension needed.
- **UVM snippets**: `uvm_component`, `uvm_object`, `uvm_sequence_item`, `uvm_sequence`, `uvm_driver`, `uvm_monitor`, `uvm_test`, `uvm_info`, `uvm_error`, `cfgget`, `always_ff`, `module`.

## Requirements

The extension runs two servers: `uvmcheck --lsp` (lint diagnostics and quick fixes) and `dv-lsp` (code browsing and completion). Either works without the other; a missing one is reported once with a link to its setting. Build `dv-lsp` with `cargo build --release -p dv-lsp` (from the repository root) and put `target/release` on `PATH`, or run `npm run bundle:local` before `npm run package` to ship both binaries inside the `.vsix`.

Release builds of the extension (`.vsix` per platform, attached to each GitHub release) bundle the `uvmcheck` binary for Linux x64 and macOS arm64; nothing else is needed. On other platforms, or to use your own build, put `uvmcheck` on `PATH` or set `uvmcheck.executablePath`:

```bash
git clone https://github.com/mayurkubavat/UVMCheck-Linter.git
cd UVMCheck-Linter
cmake --preset default && cmake --build build
export PATH="$(pwd)/build:$PATH"
```

## Configuration Settings

You can customize UVMCheck behavior via `.vscode/settings.json` or Cursor Settings:

| Setting | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `uvmcheck.enable` | `boolean` | `true` | Enable/disable live linting |
| `uvmcheck.executablePath` | `string` | `"uvmcheck"` | Path to the `uvmcheck` binary |
| `uvmcheck.preset` | `string` | `"all"` | Rule profile (`all`, `soundness`, `methodology`, `performance`, `style`) |
| `uvmcheck.navigation.enable` | `boolean` | `true` | Run `dv-lsp` for outline, definitions, references, hover and completion |
| `uvmcheck.navigation.serverPath` | `string` | `"dv-lsp"` | Path to the `dv-lsp` binary |
| `uvmcheck.navigation.libraryPaths` | `string[]` | `[]` | Extra trees to index (UVM, VIPs); `$UVM_HOME/src` is added when set |
| `uvmcheck.navigation.exclude` | `string[]` | `[]` | Directory names to skip when indexing |
| `uvmcheck.trace.server` | `string` | `"off"` | LSP protocol trace logging (`off`, `messages`, `verbose`) |

## Commands

- **`UVMCheck: Restart Language Servers`**: Restarts both servers, reloading the project config and the index.

## Installing

The extension is one `.vsix` file that works in VS Code and in its forks (Cursor, Antigravity, VSCodium, Windsurf).

```bash
cd editors/vscode
npm install
npm run package          # writes uvmcheck-vscode-<version>.vsix
code --install-extension uvmcheck-vscode-*.vsix      # VS Code
cursor --install-extension uvmcheck-vscode-*.vsix    # Cursor
```

In any of these editors you can also use **Extensions > ... > Install from VSIX...**. Cursor and Antigravity read the Open VSX registry rather than the Microsoft marketplace; `npm run publish:openvsx` (with `OVSX_PAT` set) publishes there when you want it listed.

## Installation for Development

```bash
cd editors/vscode
npm install
npm run compile
# Press F5 in VS Code to launch the Extension Development Host
```
