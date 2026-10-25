# MG-7 Duplication

Grounded in DRY, the rule of three and the clone taxonomy (exact, renamed and near-miss clones), with Fowler's Pull Up Method as the usual fix.

Copied code drifts: a fix lands in one copy and not the others. The third copy is the signal to extract a base class, a helper or a parameterized class.

## MG-7.1 (Should)

**Copy-paste is refactored into a shared base, helper or parameterized class by the third copy.**

Copied code drifts as fixes land in one copy only.

Enforced by: `SV-DUP-001`, `SV-RND-011`

## MG-7.2 (May)

**Near-identical agents become one parameterized agent.**

Two agents that differ only in names and widths are one parameterized agent waiting to happen.

Enforced by: review only (no rule yet).
