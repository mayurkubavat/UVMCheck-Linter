# MG-9 Evolvability

Grounded in code metrics (cognitive and cyclomatic complexity, size) and Tornhill's hotspot analysis.

Code is read and changed far more often than it is written. Size and complexity budgets keep routines reviewable; dead and commented-out code is removed because version control keeps it.

## MG-9.1 (Should)

**Routines and classes stay within complexity and size budgets, and dead or commented-out code is removed.**

Long, deeply nested routines are hard to review and test. Thresholds are configurable per rule in the project config.

Enforced by: `SV-MET-001`, `SV-MET-002`, `SV-MET-003`, `SV-MET-004`, `SV-MET-005`, `SV-SML-004`, `SV-STY-009`, `SV-RDB-002`, `SV-RDB-003`, `SV-RDB-005`, `SV-API-001`, `SV-DED-001`, `SV-DED-005`, `SV-DED-006`, `SV-DED-011`, `SV-DED-012`, `SV-DED-013`

## MG-9.2 (May)

**Complex code that changes often is refactored first.**

Complexity that never changes costs little; complexity in a file that changes every week costs every week.

Enforced by: review only (no rule yet).
