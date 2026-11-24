# The research knowledge base

**Purpose:** accumulate every conclusion this project has reached, so a future
agent (or human) opening this folder does not repeat the same research.

**Read `AGENTS.md` first.** This file is the maintenance protocol.

## Layout

| File | Contents |
| --- | --- |
| `findings.md` | Concluded questions. Stable IDs `F-###`, evidence grades, sources, status. |
| `audit.md` | Code defects. IDs `A-###`, severity, location, status. |
| `decisions.md` | Decisions taken or proposed. IDs `D-###`, rationale, consequences. |
| `sessions.md` | Append-only log of what each session did. IDs `S-###`. |
| `index.json` | Machine-readable topic → ID map, used by `make research-check`. |

## Evidence grades

Every finding carries one. Do not upgrade a grade without new evidence.

| Grade | Meaning |
| --- | --- |
| **[V]** | Stated in vendor documentation, a specification, or a peer-reviewed paper |
| **[P]** | Demonstrated by a working project, first-hand report, or shipping product |
| **[A]** | Architectural inference — follows from how the system works, but is not guaranteed |
| **[?]** | Untested; a spike is required |

A grade describes the *evidence*, not confidence in the product decision. `[V]`
Apple documentation can still be about a feature we cannot use.

## Statuses

| Status | Meaning |
| --- | --- |
| `settled` | Answered. Do not re-research; cite the ID. |
| `open` | Known gap; a spike or measurement is required. |
| `disproven` | Tested or proven false. Do not try it again. |
| `superseded` | Replaced by a newer finding; see `supersedes`. |

`audit.md` uses `open`, `fixed`, `wontfix`, `superseded` instead.

## How to add a finding

1. Take the next free ID in `findings.md` (they are never reused).
2. Append one row: `| F-0xx | status | conclusion in one or two sentences | grade | source |`.
   Keep the conclusion falsifiable — "X is impossible because Y" beats "X is hard".
3. If it replaces an earlier finding, add `supersedes F-0yy` in the conclusion and
   change `F-0yy` to `superseded`.
4. Add the ID to the matching topic in `index.json`.
5. Add a `S-###` entry to `sessions.md`.
6. Run `make research-check`.

## How to add an audit entry

1. Next free `A-###`.
2. Record: severity (`high`/`medium`/`low`), file and symbol, what is wrong, why it
   matters, and the fix if known.
3. If fixed, change status to `fixed` and note the verification command in the
   entry. Never delete it — the history is the point.

## Rules

- **Append, don't rewrite.** Corrections are new entries.
- **One claim per row.** If a row needs "and", it is probably two findings.
- **No unsourced numbers.** Put the source or the measurement.
- **Do not record secrets, tokens, pairing codes, or personal data.**
- **Keep the index complete.** A finding missing from `index.json` fails the check.

## Checking

```sh
make research-check
```

Validates: unique IDs, valid statuses, every `index.json` reference resolving, and
that every session entry lists at least one artefact. It is a structural check,
not a truth check — it cannot tell you a conclusion is wrong.
