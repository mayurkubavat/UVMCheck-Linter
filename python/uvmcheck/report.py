"""
Work with uvmcheck JSON reports (--json): merge, diff and trend.

    python3 -m uvmcheck.report merge  a.json b.json -o all.json
    python3 -m uvmcheck.report diff   old.json new.json [--fail-on-new] [--json diff.json]
    python3 -m uvmcheck.report trend  week1.json week2.json week3.json -o trend.html [--labels w1,w2,w3]

merge  combines reports from separate runs (one per block or per job) into
       one report. A finding seen in several reports is kept once.
diff   lists the findings that are new in the second report and the ones
       that were fixed. Findings are matched by fingerprint, which survives
       edits elsewhere in the file; reports written before fingerprints
       existed are matched by rule, file and message.
trend  writes an HTML page with the findings per run, by category, by
       guideline, by rule and by block (the top folders of the file path).

Installed with the Python package, the same commands run as `uvmcheck-report`.
"""

import argparse
import html
import json
import sys
from collections import Counter
from pathlib import Path
from typing import Dict, Iterable, List, Sequence, Tuple


# ----------------------------------------------------------------- loading

def load(path: str) -> dict:
    with open(path) as f:
        data = json.load(f)
    if not isinstance(data, dict) or "violations" not in data:
        raise ValueError(f"{path}: not a uvmcheck JSON report")
    return data


def finding_key(v: dict) -> Tuple:
    """Identity of a finding across runs."""
    if v.get("fingerprint"):
        return ("fp", v["ruleId"], v["fingerprint"])
    return ("msg", v["ruleId"], v.get("file", ""), v.get("message", ""))


def exact_key(v: dict) -> Tuple:
    """Identity of a finding within one run (for merging duplicates)."""
    return (v["ruleId"], v.get("file", ""), v.get("line", 0), v.get("column", 0), v.get("message", ""))


# ------------------------------------------------------------------ merge

def merge(reports: Sequence[dict]) -> dict:
    seen = set()
    violations = []
    for r in reports:
        for v in r["violations"]:
            k = exact_key(v)
            if k in seen:
                continue
            seen.add(k)
            violations.append(v)
    violations.sort(key=lambda v: (v.get("file", ""), v.get("line", 0), v.get("column", 0), v["ruleId"]))
    return {
        "tool": "uvmcheck",
        "version": reports[0].get("version", "") if reports else "",
        "compileErrors": sum(r.get("compileErrors", 0) for r in reports),
        "waived": sum(r.get("waived", 0) for r in reports),
        "mergedFrom": len(reports),
        "violations": violations,
    }


# ------------------------------------------------------------------- diff

def diff(old: dict, new: dict) -> dict:
    """New, fixed and unchanged findings. Repeated keys are matched by count."""
    def by_key(report):
        groups: Dict[Tuple, List[dict]] = {}
        for v in report["violations"]:
            groups.setdefault(finding_key(v), []).append(v)
        return groups

    # Mixed reports (one with fingerprints, one without) compare by message.
    if any("fingerprint" in v for v in old["violations"]) != any("fingerprint" in v for v in new["violations"]):
        strip = lambda r: {**r, "violations": [{k: x for k, x in v.items() if k != "fingerprint"}
                                               for v in r["violations"]]}
        old, new = strip(old), strip(new)

    before, after = by_key(old), by_key(new)
    added, fixed, unchanged = [], [], 0
    for k in set(before) | set(after):
        b, a = before.get(k, []), after.get(k, [])
        unchanged += min(len(a), len(b))
        added += a[len(b):]
        fixed += b[len(a):]
    order = lambda v: (v.get("file", ""), v.get("line", 0), v["ruleId"])
    return {"new": sorted(added, key=order), "fixed": sorted(fixed, key=order), "unchanged": unchanged}


def describe(v: dict) -> str:
    return f"{v.get('file', '')}:{v.get('line', 0)}: {v['ruleId']} {v.get('message', '')}"


# ------------------------------------------------------------------ trend

def block_of(path: str, depth: int) -> str:
    parts = [p for p in Path(path).parts if p not in (".", "..")]
    folders = parts[:-1]
    return "/".join(folders[:depth]) if folders else "(top)"


def tallies(report: dict, depth: int) -> Dict[str, Counter]:
    t = {"category": Counter(), "guideline": Counter(), "rule": Counter(), "block": Counter(), "severity": Counter()}
    for v in report["violations"]:
        t["category"][v.get("category", "Uncategorized")] += 1
        for g in v.get("guidelines", []) or ["(none)"]:
            t["guideline"][g] += 1
        t["rule"][v["ruleId"]] += 1
        t["block"][block_of(v.get("file", ""), depth)] += 1
        t["severity"][v.get("severity", "")] += 1
    return t


CSS = """
:root { --bg:#f7f8f7; --fg:#1d2522; --muted:#5b6762; --line:#d8deda; --card:#fff; --up:#b4381f; --down:#18794e;
  --s1:#2f6db5; --s2:#c46a1b; --s3:#2e8b6d; --s4:#8a4fbf; --s5:#b33b5e; --s6:#5d7a1f; }
@media (prefers-color-scheme: dark) { :root { --bg:#141917; --fg:#e4eae7; --muted:#9ba8a2; --line:#2d3733;
  --card:#1b221f; --up:#ff8a70; --down:#5fd39b; --s1:#6aa6ee; --s2:#f0a35b; --s3:#5ccaa5; --s4:#bb8cf0;
  --s5:#f07a9c; --s6:#a8c95a; } }
body { margin:0; background:var(--bg); color:var(--fg); font:14px/1.5 system-ui, -apple-system, sans-serif; }
main { max-width:1000px; margin:0 auto; padding:28px 16px 60px; }
h1 { font-size:1.6rem; margin:0 0 4px; } h2 { font-size:1.15rem; margin:32px 0 10px; }
.muted { color:var(--muted); }
.cards { display:grid; grid-template-columns:repeat(auto-fit, minmax(150px, 1fr)); gap:10px; margin-top:16px; }
.card { background:var(--card); border:1px solid var(--line); border-radius:6px; padding:12px 14px; }
.card b { display:block; font-size:1.5rem; font-variant-numeric:tabular-nums; }
.wrap { overflow-x:auto; background:var(--card); border:1px solid var(--line); border-radius:6px; }
table { border-collapse:collapse; width:100%; font-size:0.9rem; }
th, td { padding:6px 10px; border-bottom:1px solid var(--line); text-align:right; white-space:nowrap;
  font-variant-numeric:tabular-nums; }
th:first-child, td:first-child { text-align:left; }
tr:last-child td { border-bottom:0; }
.up { color:var(--up); } .down { color:var(--down); }
svg text { fill:var(--muted); font-size:11px; }
.legend { display:flex; flex-wrap:wrap; gap:4px 14px; font-size:0.85rem; margin:8px 0 0; }
.legend span::before { content:""; display:inline-block; width:10px; height:10px; border-radius:2px;
  margin-right:5px; background:currentColor; vertical-align:-1px; }
"""

SERIES = ["var(--s1)", "var(--s2)", "var(--s3)", "var(--s4)", "var(--s5)", "var(--s6)"]


def chart(labels: List[str], series: List[Tuple[str, List[int]]]) -> str:
    """Line chart of the top series; inline SVG so the page is self-contained."""
    w, h, pad_l, pad_b, pad_t = 760, 220, 44, 26, 10
    top = max([max(vals) for _, vals in series] + [1])
    n = len(labels)
    x = lambda i: pad_l + (w - pad_l - 12) * (i / (n - 1) if n > 1 else 0.5)
    y = lambda val: pad_t + (h - pad_t - pad_b) * (1 - val / top)
    out = [f'<svg viewBox="0 0 {w} {h}" width="100%" role="img" aria-label="Findings per run">']
    for frac in (0, 0.5, 1):
        yy = y(top * frac)
        out.append(f'<line x1="{pad_l}" x2="{w - 12}" y1="{yy:.1f}" y2="{yy:.1f}" stroke="var(--line)"/>')
        out.append(f'<text x="{pad_l - 6}" y="{yy + 4:.1f}" text-anchor="end">{round(top * frac)}</text>')
    for i, label in enumerate(labels):
        out.append(f'<text x="{x(i):.1f}" y="{h - 8}" text-anchor="middle">{html.escape(label)}</text>')
    for (name, vals), color in zip(series, SERIES):
        pts = " ".join(f"{x(i):.1f},{y(v):.1f}" for i, v in enumerate(vals))
        out.append(f'<polyline points="{pts}" fill="none" stroke="{color}" stroke-width="2">'
                   f'<title>{html.escape(name)}</title></polyline>')
        for i, v in enumerate(vals):
            out.append(f'<circle cx="{x(i):.1f}" cy="{y(v):.1f}" r="3" fill="{color}">'
                       f'<title>{html.escape(name)}, {html.escape(labels[i])}: {v}</title></circle>')
    out.append("</svg>")
    legend = "".join(f'<span style="color:{c}">{html.escape(n)}</span>' for (n, _), c in zip(series, SERIES))
    return "".join(out) + f'<div class="legend">{legend}</div>'


def table(labels: List[str], rows: List[Tuple[str, List[int]]], limit: int) -> str:
    head = "".join(f"<th>{html.escape(l)}</th>" for l in labels)
    body = []
    for name, vals in rows[:limit]:
        change = vals[-1] - vals[-2] if len(vals) > 1 else 0
        cls = "up" if change > 0 else "down" if change < 0 else "muted"
        cells = "".join(f"<td>{v}</td>" for v in vals)
        body.append(f'<tr><td>{html.escape(name)}</td>{cells}<td class="{cls}">{change:+d}</td></tr>')
    more = f'<p class="muted">{len(rows) - limit} more not shown.</p>' if len(rows) > limit else ""
    return (f'<div class="wrap"><table><thead><tr><th></th>{head}<th>change</th></tr></thead>'
            f'<tbody>{"".join(body)}</tbody></table></div>{more}')


def trend_html(reports: Sequence[dict], labels: List[str], depth: int) -> str:
    per_run = [tallies(r, depth) for r in reports]
    totals = [len(r["violations"]) for r in reports]
    sections = []
    for dim, title, limit in (("category", "By category", 40), ("guideline", "By guideline", 60),
                              ("block", "By block", 60), ("rule", "By rule", 80)):
        names = set().union(*(t[dim].keys() for t in per_run))
        rows = sorted(((n, [t[dim][n] for t in per_run]) for n in names), key=lambda r: (-r[1][-1], r[0]))
        sections.append(f"<h2>{title}</h2>" + (chart(labels, rows[:6]) if dim != "rule" else "")
                        + table(labels, rows, limit))
    d = diff(reports[-2], reports[-1]) if len(reports) > 1 else None
    cards = [("findings now", totals[-1]), ("first run", totals[0])]
    if d:
        cards += [("new since previous", len(d["new"])), ("fixed since previous", len(d["fixed"]))]
    cards += [("waived now", reports[-1].get("waived", 0))]
    card_html = "".join(f'<div class="card"><b>{v}</b><span class="muted">{html.escape(k)}</span></div>'
                        for k, v in cards)
    return f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>uvmcheck trend</title><style>{CSS}</style></head>
<body><main>
<h1>Lint trend</h1>
<p class="muted">{len(reports)} runs, {html.escape(labels[0])} to {html.escape(labels[-1])}.</p>
<div class="cards">{card_html}</div>
<h2>Total findings</h2>{chart(labels, [("all findings", totals)])}
{"".join(sections)}
</main></body></html>
"""


# -------------------------------------------------------------------- CLI

def write_json(data: dict, path: str) -> None:
    text = json.dumps(data, indent=2) + "\n"
    if path == "-":
        sys.stdout.write(text)
    else:
        Path(path).write_text(text)


def main(argv: Iterable[str] = None) -> int:
    ap = argparse.ArgumentParser(prog="uvmcheck-report", description=__doc__.split("\n\n")[0].strip())
    sub = ap.add_subparsers(dest="cmd", required=True)

    m = sub.add_parser("merge", help="combine reports into one")
    m.add_argument("reports", nargs="+")
    m.add_argument("-o", "--output", default="-", help="output JSON (default stdout)")

    d = sub.add_parser("diff", help="new and fixed findings between two reports")
    d.add_argument("old")
    d.add_argument("new")
    d.add_argument("--json", help="also write the diff as JSON")
    d.add_argument("--fail-on-new", action="store_true", help="exit 1 when there are new findings")

    t = sub.add_parser("trend", help="HTML trend page over several reports (oldest first)")
    t.add_argument("reports", nargs="+")
    t.add_argument("-o", "--output", required=True, help="output HTML file")
    t.add_argument("--labels", help="comma-separated run labels (default: file names)")
    t.add_argument("--block-depth", type=int, default=2, help="folders that name a block (default 2)")

    args = ap.parse_args(list(argv) if argv is not None else None)
    try:
        if args.cmd == "merge":
            out = merge([load(p) for p in args.reports])
            write_json(out, args.output)
            if args.output != "-":
                print(f"Merged {len(args.reports)} report(s): {len(out['violations'])} finding(s) -> {args.output}")
            return 0

        if args.cmd == "diff":
            result = diff(load(args.old), load(args.new))
            for v in result["new"]:
                print(f"+ {describe(v)}")
            for v in result["fixed"]:
                print(f"- {describe(v)}")
            print(f"{len(result['new'])} new, {len(result['fixed'])} fixed, {result['unchanged']} unchanged")
            if args.json:
                write_json(result, args.json)
            return 1 if args.fail_on_new and result["new"] else 0

        reports = [load(p) for p in args.reports]
        labels = args.labels.split(",") if args.labels else [Path(p).stem for p in args.reports]
        if len(labels) != len(reports):
            ap.error(f"{len(labels)} label(s) for {len(reports)} report(s)")
        Path(args.output).write_text(trend_html(reports, labels, args.block_depth))
        print(f"Wrote trend over {len(reports)} run(s) to {args.output}")
        return 0
    except (OSError, ValueError, json.JSONDecodeError) as e:
        print(f"Error: {e}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
