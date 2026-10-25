# UVM Methodology Guide

This guide describes how UVMCheck expects UVM testbench code to be structured so that it stays reusable and easy to change. Each guideline is a short, testable statement; most are enforced by one or more lint rules, and the rest are left to review until a rule exists.

## Structure

The guide has four levels, each pointing to the one below:

| Level | Answers | Example |
| :--- | :--- | :--- |
| Principle | Why | Law of Demeter, single responsibility, connascence |
| Chapter | Where it applies | `MG-4` Reusable verification components |
| Guideline | What to do | `MG-4.1` Every agent works in both active and passive mode |
| Rule | How it is checked | `UVM-RUS-001` AgentIgnoresIsActive |

Guideline levels use the wording of standards documents:

- **Shall**: a requirement. Departing from it needs a waiver with a reason.
- **Should**: a recommendation. Departures are expected to be rare and explained.
- **May**: advice. Rules for it are usually in the Advisory profile, off by default.

## Chapters

| Chapter | Title | Guidelines |
| :--- | :--- | :--- |
| [MG-1](MG-1-layering.md) | Layering and packages | MG-1.1, MG-1.2, MG-1.3, MG-1.4 |
| [MG-2](MG-2-coupling.md) | Coupling | MG-2.1, MG-2.2, MG-2.3, MG-2.4 |
| [MG-3](MG-3-roles.md) | Cohesion and component roles | MG-3.1, MG-3.2, MG-3.3 |
| [MG-4](MG-4-reuse.md) | Reusable verification components | MG-4.1, MG-4.2, MG-4.3, MG-4.4, MG-4.5 |
| [MG-5](MG-5-patterns.md) | Design-pattern conformance | MG-5.1, MG-5.2, MG-5.3 |
| [MG-6](MG-6-constructs.md) | Construct usage | MG-6.1, MG-6.2, MG-6.3 |
| [MG-7](MG-7-duplication.md) | Duplication | MG-7.1, MG-7.2 |
| [MG-8](MG-8-contracts.md) | Contracts and failure handling | MG-8.1, MG-8.2 |
| [MG-9](MG-9-evolvability.md) | Evolvability | MG-9.1, MG-9.2 |

## Using the guide with the linter

```bash
# Print the guide with the rules behind each guideline
uvmcheck --list-guidelines
uvmcheck --list-guidelines --json guide.json

# Run only the rules of one chapter or guideline (Advisory rules included)
uvmcheck --guideline MG-4 -f files.f
uvmcheck --guideline MG-3.1,MG-4.1 -f files.f

# Presets: reuse = MG-1 to MG-5; guide = every guideline
uvmcheck --preset reuse -f files.f

# Add a pass/fail line per guideline to any run
uvmcheck --conformance --json report.json -f files.f
```

With guideline selection or `--conformance`, the console summary ends with a pass/fail line per guideline and a tally per chapter, and the JSON report gains a `conformance` array. Every finding in the JSON report also lists the guidelines its rule enforces.

The mapping from guidelines to rules lives in `src/rules/Guidelines.cpp`. `tests/test_guidelines.py` checks that every rule named there exists and that the chapter pages list the same rules.

## Turning a principle into a rule

Every new rule goes through the same steps, in order. A rule without a bad snippet or a precision measurement does not ship.

1. **Pick a principle** from the software-engineering literature, such as the Law of Demeter or connascence of name.
2. **Write the UVM symptom**: what a violation looks like in a real testbench, as a bad and a good snippet. If you cannot write the bad snippet, there is no rule.
3. **Find a static signature**: the evidence the linter can see. That is syntax, a semantic fact from slang, a cross-file fact from the UVM model, or (later) git history.
4. **Measure precision** on uvm-core, open-source VIPs and internal code. Count false positives and define the escape hatches (exempt roles, phases, thresholds).
5. **Assign a level and a profile**: Shall and Should rules report warnings and run by default; May rules go to the Advisory profile.
6. **Encode** the rule with bad and good tests, add it to its guideline in `src/rules/Guidelines.cpp` and to the chapter page, and give the message a concrete fix.

### Scoring a candidate

Score each criterion from 1 to 3. A rule with a total of 10 or more and a precision score of 3 can run by default; anything else starts in the Advisory profile.

| Criterion | 1 | 2 | 3 |
| :--- | :--- | :--- | :--- |
| Detectability | Needs history or heuristics | Needs semantic or cross-file facts | Syntax or one semantic query |
| Precision | Over 20% false positives | 5 to 20% | Under 5% |
| Impact | Style or taste | Slows reuse or review | Wrong results, hangs, or blocks reuse |
| Fix clarity | Needs a redesign | A known refactoring | A local, mechanical change |

## Sources

- Robert C. Martin, *Agile Software Development: Principles, Patterns, and Practices* (SOLID and package principles).
- Glenford Myers, *Composite/Structured Design* (coupling and cohesion); Meilir Page-Jones, *What Every Programmer Should Know About Object-Oriented Design* (connascence).
- David Parnas, *On the Criteria To Be Used in Decomposing Systems into Modules*; Bertrand Meyer, *Object-Oriented Software Construction* (design by contract).
- Martin Fowler, *Refactoring*, 2nd ed.; Gamma, Helm, Johnson and Vlissides, *Design Patterns*.
- MISRA C:2012; Gerard Holzmann, *The Power of Ten*; SEI CERT C Coding Standard.
- Adam Tornhill, *Your Code as a Crime Scene*.
- Accellera UVM 1800.2 User's Guide and class reference.
