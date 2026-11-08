# Security policy

## Supported versions

Only the latest release and the `main` branch get security fixes.

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub:
**Security > Report a vulnerability** on
[the repository](https://github.com/mayurkubavat/UVMCheck-Linter/security/advisories/new).
Do not open a public issue.

Include the version or commit, the command line, and a minimal input that
reproduces the problem. You can expect an acknowledgement within a week and a
fix or a plan within 30 days.

## Scope

UVMCheck reads untrusted SystemVerilog sources, file lists and project
configuration. In scope:

- crashes, hangs or unbounded memory use on crafted input (`dv-lint`,
  `dv-lsp`, `uvmcheck`);
- reading or writing files outside the paths a run was given, other than the
  report and cache paths it was told to write;
- the language server or VS Code extension running commands it was not
  configured to run.

Out of scope: `.uvmcheck/config.py` and Python rule plugins are code by
design and run with the user's privileges, as a build script would. Only
lint projects whose configuration you trust.
