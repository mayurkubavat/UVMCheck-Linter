//! `--help` and `--version`: what the command line takes, in the order a
//! new user needs it. A test checks that every option listed here is one the
//! program accepts, so the text cannot drift from the parser.

/// The name the program was started under (`uvmcheck` or `dv-lint`).
pub fn program() -> String {
    std::env::args()
        .next()
        .and_then(|a| std::path::Path::new(&a).file_stem().map(|s| s.to_string_lossy().into_owned()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "uvmcheck".into())
}

/// `uvmcheck 0.1.0`.
pub fn version() -> String {
    format!("{} {}", program(), env!("CARGO_PKG_VERSION"))
}

/// The `--help` text.
pub fn help() -> String {
    format!("{}\n\nUsage: {} [options] <compile options> FILE...\n{OPTIONS}", SUMMARY, program())
}

/// The line that follows an argument error.
pub fn hint() -> String {
    format!("run '{} --help' for the options", program())
}

const SUMMARY: &str = "Static analysis for SystemVerilog and UVM verification code.";

/// The options, as `--help` prints them.
pub const OPTIONS: &str = "
With no sources, the project's .uvmcheck/config.py (or config.json) names
them. Start a project with --init.

Compile options (as a simulator takes them):
  FILE...                  Source files, compiled in order
  -f FILE, -F FILE         A file list (-F: paths relative to the list)
  +incdir+DIR, -I DIR      Include directory
  +define+NAME[=VALUE], -D NAME[=VALUE]
                           Define a macro
  -top NAME                Top module; elaboration starts from the tops
  -v FILE, -y DIR          Library file or directory, searched for modules
                           the sources instantiate (+libext+.sv+.v)
  --single-unit            Compile all files as one compilation unit
  --skip-libraries         Report nothing in -v and -y library files
  --report-library         Report in the UVM library too; beside your own
                           files its findings are left out
  --in-context             Lint a header inside the file that includes it

Rules:
  --preset NAME            default, soundness, methodology, performance,
                           security, style, smells, advisory, reuse, guide,
                           uvm-1800.2 or all. The default is soundness,
                           methodology, performance and security, plus any
                           rule the project config configures
  --rule ID                Run only this rule (repeatable)
  --experimental           Also run experimental rules
  --python-rules PATH      Also run rules written with the Python SDK
  --history                Let the git history rules (SV-EVO-*) read the log
  --list-rules             List every rule with its category, severity and
                           description, and exit (--json [FILE]: the catalog)
  --list-guidelines        List the methodology guidelines and the rules that
                           enforce each, and exit (--json [FILE])
  --list                   List rule ids and names only, and exit
  --explain RULE           Describe a rule, with its example, and exit

Project:
  --init                   Write a starter .uvmcheck/config.py and exit
  --config FILE            Use this project config
  --no-config              Ignore the project config
  --waivers FILE           Waiver file (default: .uvmcheck/waivers.json)
  --baseline FILE          Report only findings not in this baseline
  --update-baseline FILE   Write the current findings as the baseline
  --changed-since REF      Report only findings on lines changed since REF
  --strict-new-files       With --changed-since: fail on any finding in a
                           file added since REF

Reports:
  --json FILE              JSON report ('-' for standard output)
  --sarif FILE             SARIF 2.1.0 report, for code scanning
  --html FILE              Self-contained HTML report
  --map FILE               The testbench map (components and connections)
  --frames, --no-frames    Show the source line under each finding
                           (default: on a terminal)
  --time                   Print where the time went

Fixes:
  --fix                    Apply the safe fixes
  --fix-dry-run            Print the fixes as a diff, change nothing
  --unsafe-fixes           Also apply fixes that can change behaviour
  --fix-rule LIST          Limit fixes to these rules (comma separated)

Waiver hygiene:
  --waiver-report          List waivers and baseline entries that match
                           nothing, expired waivers and counts per rule
  --report-unused-waivers  Report those as UVM-WV-001 warnings
  --strict-waivers         Exit 4 when any are found

Cache:
  --no-cache               Always analyze
  --cache-dir DIR          Where the result cache lives
                           (default: .uvmcheck_cache)
  --clear-cache            Delete the cache, then run

  -h, --help               Print this help and exit
  --lsp                    Serve an editor: run the language server (dv-lsp)
  --version                Print the version and exit

Exit status: 0 no ERROR finding remains, 1 an ERROR finding remains,
2 a load or configuration error, 4 --strict-waivers found stale waivers.

Environment: DV_LINT_JOBS (threads; default every core), UVMCHECK_PYTHON
(the interpreter for config.py and Python rules).

Guide and rule catalog: https://github.com/mayurkubavat/svlang
";
