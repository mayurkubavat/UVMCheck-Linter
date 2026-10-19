# UVMCheck Extension for VS Code & Cursor

Real-time, zero-latency SystemVerilog and UVM static analysis in your editor powered by **UVMCheck** and **Slang**.

## Features

- **Real-Time Diagnostics**: Instant feedback on 233+ rules covering UVM methodology, OOP soundness, race conditions, TLM connectivity, and dead code.
- **Deep Semantic Analysis**: Cross-component validation of objection CFGs, Config DB type matching, virtual sequencer wiring, and RAL predictor binding.
- **Incremental Linting**: Live diagnostics as you type over Language Server Protocol (`uvmcheck --lsp`).
- **Zero Configuration**: Automatically resolves project include trees and UVM package scopes.

## Requirements

Ensure `uvmcheck` is built and available on your system `PATH`:

```bash
git clone https://github.com/mayurkubavat/UVMCheck-Linter.git
cd UVMCheck-Linter
cmake -B build -G Ninja
ninja -C build
# Optionally install globally or add to PATH:
export PATH="$(pwd)/build:$PATH"
```

## Configuration Settings

You can customize UVMCheck behavior via `.vscode/settings.json` or Cursor Settings:

| Setting | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `uvmcheck.enable` | `boolean` | `true` | Enable/disable live linting |
| `uvmcheck.executablePath` | `string` | `"uvmcheck"` | Path to the `uvmcheck` binary |
| `uvmcheck.preset` | `string` | `"all"` | Rule profile (`all`, `soundness`, `methodology`, `performance`, `style`) |
| `uvmcheck.trace.server` | `string` | `"off"` | LSP protocol trace logging (`off`, `messages`, `verbose`) |

## Commands

- **`UVMCheck: Restart Language Server`**: Restarts the background LSP daemon and reloads workspace rules.

## Installation for Development

```bash
cd editors/vscode
npm install
npm run compile
# Press F5 in VS Code to launch the Extension Development Host
```
