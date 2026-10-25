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

Both rules are advisory and read the git history, so they run only when `--history` is given; a plain lint never starts git and its results do not depend on the checkout. They look at the last 1000 non-merge commits (option `max_commits`).

SV-EVO-001 scores each file as its number of commits times the summed cyclomatic complexity of its routines, and reports the five highest (options `top_files`, `min_commits`, default 3, and `min_complexity`, default 15) at the file's most complex routine. These hotspots are where refactoring pays first.

SV-EVO-002 reports two files in different packages that changed together in at least three commits (`min_shared_commits`) and in at least half of the commits touching either (`min_degree`, in percent). Commits touching more than 30 files (`max_commit_files`), such as reformatting or licence updates, are left out. Packages that keep changing together share knowledge their imports do not show.

```
uvmcheck --history --guideline MG-9.2 -f tb.f
```

Enforced by: `SV-EVO-001`, `SV-EVO-002`
