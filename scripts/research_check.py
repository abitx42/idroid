#!/usr/bin/env python3
"""Structural check for the research knowledge base.

Validates that the ledgers are well formed and that index.json agrees with them.
It cannot tell whether a conclusion is true -- only that the memory is coherent.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RESEARCH = ROOT / "research"

FINDING_ROW = re.compile(r"^\|\s*(F-\d{3})\s*\|\s*([a-z]+)\s*\|(.*)$")
AUDIT_ROW = re.compile(r"^\|\s*(A-\d{3})\s*\|\s*(high|medium|low)\s*\|\s*([a-z]+)\s*\|")
DECISION_ROW = re.compile(r"^\|\s*(D-\d{3})\s*\|\s*([a-z]+)\s*\|")
SESSION_ROW = re.compile(r"^##\s+(S-\d{3})\b")
GRADE = re.compile(r"\[(V|P|A|\?)\]")
ID = re.compile(r"^[FADS]-\d{3}$")

FINDING_STATUS = {"settled", "open", "disproven", "superseded"}
AUDIT_STATUS = {"open", "fixed", "wontfix", "superseded"}
DECISION_STATUS = {"accepted", "proposed", "superseded"}

errors: list[str] = []
warnings: list[str] = []


def rows(path: Path, pattern: re.Pattern[str]) -> list[re.Match[str]]:
    if not path.is_file():
        errors.append(f"missing file: {path.relative_to(ROOT)}")
        return []
    found = []
    for line in path.read_text(encoding="utf-8").splitlines():
        match = pattern.match(line)
        if match:
            found.append(match)
    return found


def unique(ids: list[str], label: str) -> set[str]:
    seen: set[str] = set()
    for value in ids:
        if value in seen:
            errors.append(f"{label}: duplicate id {value}")
        seen.add(value)
    return seen


def main() -> int:
    findings = rows(RESEARCH / "findings.md", FINDING_ROW)
    finding_ids: list[str] = []
    for match in findings:
        ident, status, rest = match.group(1), match.group(2), match.group(3)
        finding_ids.append(ident)
        if status not in FINDING_STATUS:
            errors.append(f"{ident}: invalid status '{status}'")
        if not GRADE.search(rest):
            errors.append(f"{ident}: missing evidence grade [V]/[P]/[A]/[?]")
        if "superseded" == status and "supersedes" not in rest:
            warnings.append(f"{ident}: marked superseded but names no replacement")
    finding_set = unique(finding_ids, "findings.md")

    audits = rows(RESEARCH / "audit.md", AUDIT_ROW)
    audit_ids: list[str] = []
    for match in audits:
        ident, status = match.group(1), match.group(3)
        audit_ids.append(ident)
        if status not in AUDIT_STATUS:
            errors.append(f"{ident}: invalid audit status '{status}'")
    audit_set = unique(audit_ids, "audit.md")

    decisions = rows(RESEARCH / "decisions.md", DECISION_ROW)
    decision_ids: list[str] = []
    for match in decisions:
        ident, status = match.group(1), match.group(2)
        decision_ids.append(ident)
        if status not in DECISION_STATUS:
            errors.append(f"{ident}: invalid decision status '{status}'")
    decision_set = unique(decision_ids, "decisions.md")

    sessions = rows(RESEARCH / "sessions.md", SESSION_ROW)
    session_set = unique([match.group(1) for match in sessions], "sessions.md")

    for ident in sorted(finding_set | audit_set):
        number = int(ident[2:])
        if number == 0:
            errors.append(f"{ident}: ids start at 001")

    index_path = RESEARCH / "index.json"
    if not index_path.is_file():
        errors.append("missing research/index.json")
        return report(len(finding_set), len(audit_set), len(decision_set), len(session_set))

    try:
        index = json.loads(index_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        errors.append(f"index.json is not valid JSON: {exc}")
        return report(len(finding_set), len(audit_set), len(decision_set), len(session_set))

    referenced: set[str] = set()

    def walk(node: object) -> None:
        if isinstance(node, str):
            if ID.match(node):
                referenced.add(node)
        elif isinstance(node, list):
            for item in node:
                walk(item)
        elif isinstance(node, dict):
            for value in node.values():
                walk(value)

    walk(index)

    for ident in sorted(referenced - (finding_set | audit_set | decision_set | session_set)):
        errors.append(f"index.json references unknown id {ident}")

    for topic, body in index.get("topics", {}).items():
        for ident in body.get("findings", []) + body.get("audit", []):
            if ID.match(ident) and ident not in referenced:
                warnings.append(f"topic '{topic}' lists {ident} outside the index registry")

    indexed_findings = {value for value in referenced if value.startswith("F-")}
    indexed_audits = {value for value in referenced if value.startswith("A-")}
    missing = sorted(finding_set - indexed_findings)
    if missing:
        errors.append(f"findings missing from index.json: {', '.join(missing)}")
    missing = sorted(audit_set - indexed_audits)
    if missing:
        errors.append(f"audit entries missing from index.json: {', '.join(missing)}")

    if set(index.get("decisions", [])) != decision_set:
        errors.append("index.json decisions do not match decisions.md")
    if set(index.get("sessions", [])) != session_set:
        errors.append("index.json sessions do not match sessions.md")

    return report(len(finding_set), len(audit_set), len(decision_set), len(session_set))


def report(findings: int, audits: int, decisions: int, sessions: int) -> int:
    for warning in warnings:
        print(f"warning: {warning}")
    for error in errors:
        print(f"error: {error}")
    if errors:
        print(f"\nresearch-check FAILED ({len(errors)} error(s))")
        return 1
    print(
        "research-check OK: "
        f"{findings} findings, {audits} audit entries, "
        f"{decisions} decisions, {sessions} sessions"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
