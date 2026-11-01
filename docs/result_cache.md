# Incremental result cache

The cache lives in `.uvmcheck_cache/` (gitignored) and is **on by default**. A second run with the same inputs prints `N compilation units served from cache` and does not parse or run rules. `--no-cache` skips the cache entirely. `--cache-dir <path>` moves it. `--clear-cache` deletes it and then runs.

The cache is on by default because a hit is taken only when every recorded dependency still has the same bytes. A missing file, a version or format mismatch, or a JSON entry that does not parse falls back to a full run. The process does not abort on a bad cache.

## What a unit is

A compilation unit is one primary file, a file named on the command line or in a file list, not a file pulled in by `` `include ``. Its key is a hash of:

- the tool version and the cache format version
- the registered rule set (id, severity, and whether the rule is whole-program)
- preset, `--rule`, `--guideline`, `--experimental`, and `--engine`
- defines, include directories, and the bytes of the project config, waiver file, baseline, command files, and Python rule sources
- the bytes of the primary file, every file it transitively includes, and every file of every package it imports, including packages those packages import

Includes come from slang's source manager (`getIncludedFrom` / the syntax tree's buffers). Imports come from the elaborated import symbols, and the imported package's files come from the locations of that package's members. Nothing is detected by scanning source text for `` `include `` or `import`.

Files that contribute declarations to `$unit` (anything that is not itself a package) also depend on the other `$unit` files in the same run, because those files can see each other without an import.

## Whole-program rules

Some rules compare the whole design. A finding they report in one file can change because a different file changed, even when the first file does not include or import the second. Those findings are stored separately and are reused only when the hash of every input is unchanged. Otherwise they are recomputed.

| Rule | Why it is whole-program |
| :--- | :--- |
| `SV-DUP-001` … `SV-DUP-005` | Clone detection across classes and packages |
| `SV-PKG-004`, `SV-PKG-005` | The package dependency graph |
| `SV-EVO-001`, `SV-EVO-002` | Git history across the tree (`--history` does not use the cache) |
| `UVM-ARC-001`, `UVM-ARC-003` | Layer order, and one file included into several packages |
| `UVM-TLM-001`, `UVM-TLM-004`, `UVM-VSEQ-001`, `UVM-SEQ-013`, `UVM-SEQ-014`, `UVM-SEQ-015` | Ports, sequencers, and sequence items wired in another file |
| `UVM-CFG-005`, `UVM-CFG-010`, `UVM-CFG-011`, `UVM-PHS-010` | A config or phase access whose other end is another component |
| `RAL-WIRE-001`, `RAL-MAP-001`, `RAL-MAP-002`, `RAL-COL-001` | Register maps, address ranges, and adapter wiring across the model |

Every other rule is cached with its compilation unit. Its result for a file is reused when that file, its includes, and its imported packages are unchanged.

## When the cache is not used

`--fix` and `--fix-dry-run` need source ranges the cache does not store. `--history`, `--changed-since`, `--map`, `--update-baseline`, and the waiver-hygiene flags (`--waiver-report`, `--strict-waivers`, `--report-unused-waivers`) depend on git state, a new baseline snapshot, or a fresh scan of which waivers matched. `--init` writes a new config. Those modes always analyze from scratch.

## On disk

`manifest.json` records the environment key, each unit's dependency hashes, and the whole-program key. Findings, including fix text and fix safety, are JSON blobs next to it (`format` 1). A blob whose `format` or tool version does not match is ignored.
