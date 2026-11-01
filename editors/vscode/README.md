# UVMCheck Extension for VS Code & Cursor

Real-time, zero-latency SystemVerilog and UVM static analysis in your editor powered by **UVMCheck** and **Slang**.

## Features

- **Real-Time Diagnostics**: Instant feedback on 330+ rules covering UVM methodology, OOP soundness, race conditions, TLM connectivity, and dead code.
- **Deep Semantic Analysis**: Cross-component validation of objection CFGs, Config DB type matching, virtual sequencer wiring, and RAL predictor binding.
- **Incremental Linting**: Live diagnostics as you type over Language Server Protocol (`uvmcheck --lsp`).
- **Project aware**: Compiles the project from `.uvmcheck/config.py` (sources, file lists, include dirs, defines) with your unsaved edits, and applies its waivers and baseline.
- **Quick fixes**: Apply a rule's fix-it, or waive a finding on its line with a reason.

## Requirements

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
