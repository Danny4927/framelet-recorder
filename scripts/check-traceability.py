#!/usr/bin/env python3
"""Check requirement traceability between SPECIFICATION.md and the matrix.

Roadmap criterion A-10. Standard library only. Checks:

* requirement IDs have the form ``REQ-<AREA>-<NN>`` (or a named suffix such
  as ``REQ-MODE-MIC``) and each matrix ID is unique;
* phase and state values use the vocabulary of docs/verification;
* every ID tagged in SPECIFICATION.md or SECURITY.md has a matrix row, and
  every matrix row's ID is tagged in one of those documents;
* every normative list item, and every paragraph line that says "must",
  outside the explicitly informative sections carries an inline tag;
* backticked test references resolve to a ``#[test] fn <name>`` in the named
  crate, and backticked repository paths exist;
* linked evidence files exist;
* rows at ``automated-tested`` or higher cite at least one test, a link to a
  workflow file, or a CI job/step (``CI job "linux"``, ``CI step "<name>"``)
  that exists in .github/workflows/ci.yml.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = "SPECIFICATION.md"
SECURITY = "SECURITY.md"
MATRIX = "docs/verification/traceability.md"
CI_WORKFLOW = ".github/workflows/ci.yml"

# Sections of SPECIFICATION.md whose own text (not their subsections) is
# informative: rationale, candidate lists, cross-references, or summaries of
# requirements tagged elsewhere. "" is the preamble before the first section.
INFORMATIVE_SECTIONS = frozenset(
    {
        "",
        "3. Recommended technology direction",
        "11. Implementation sequence",
        "12. Definition of Done",
    }
)

# Marker for a single list item that is informative inside a normative section.
INFORMATIVE_MARKER = "*(informative)*"

STATES = (
    "open",
    "implemented",
    "automated-tested",
    "ui-integrated",
    "real-device-validated",
)
PHASES = frozenset({"Foundation", "A", "B", "C", "D", "E", "F", "G", "H"})
EVERY_PHASE = "every phase"
CI_REF_PATTERN = re.compile(r'CI (job|step) "([^"]+)"')
JOB_PATTERN = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
STEP_PATTERN = re.compile(r"^\s*-\s+name:\s*(.+?)\s*$")
TABLE_SEPARATOR_PATTERN = re.compile(r"^\|[\s:|-]+\|$")
COMMENT_PATTERN = re.compile(r"/\*.*?\*/|//[^\n]*", re.DOTALL)

ID_PATTERN = re.compile(r"^REQ-[A-Z]+-(?:\d{2}|[A-Z][A-Z0-9]*)$")
TAG_PATTERN = re.compile(r"\[(REQ-[A-Za-z0-9-]+)\]")
HEADING_PATTERN = re.compile(r"^(#{1,6})\s+(.*?)\s*$")
ITEM_PATTERN = re.compile(r"^\s*(?:[-*+]|\d+\.)\s+")
MUST_PATTERN = re.compile(r"\bmust\b", re.IGNORECASE)
BACKTICK_PATTERN = re.compile(r"`([^`]+)`")
LINK_PATTERN = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
TEST_REF_PATTERN = re.compile(r"^([a-z][a-z0-9_]*)(?:::[A-Za-z0-9_]+)*::([a-z0-9_]+)$")
PACKAGE_NAME_PATTERN = re.compile(r'^\s*name\s*=\s*"([^"]+)"', re.MULTILINE)


@dataclass(frozen=True)
class Row:
    """One requirement row of the traceability matrix."""

    line: int
    req_id: str
    phase: str
    method: str
    evidence: str
    required_state: str
    state: str


def parse_matrix(text: str) -> tuple[list[Row], list[str]]:
    """Return the requirement rows of the matrix and structural errors."""
    rows: list[Row] = []
    errors: list[str] = []
    for number, line in enumerate(text.splitlines(), start=1):
        if not line.startswith("| REQ-"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) != 7:
            errors.append(f"{MATRIX}:{number}: expected 7 columns, found {len(cells)}")
            continue
        req_id, _requirement, phase, method, evidence, required, state = cells
        rows.append(Row(number, req_id, phase, method, evidence, required, state))
    return rows, errors


def check_rows(rows: list[Row]) -> list[str]:
    """Check ID format, uniqueness, phase, and state vocabulary."""
    errors: list[str] = []
    seen: dict[str, int] = {}
    for row in rows:
        where = f"{MATRIX}:{row.line}"
        if not ID_PATTERN.match(row.req_id):
            errors.append(f"{where}: malformed requirement ID {row.req_id!r}")
        if row.req_id in seen:
            errors.append(f"{where}: duplicate ID {row.req_id} (first on line {seen[row.req_id]})")
        else:
            seen[row.req_id] = row.line
        if not phase_is_valid(row.phase):
            errors.append(f"{where}: {row.req_id} has invalid phase {row.phase!r}")
        for label, value in (("required state", row.required_state), ("state", row.state)):
            if value not in STATES:
                errors.append(f"{where}: {row.req_id} has invalid {label} {value!r}")
    return errors


def phase_is_valid(phase: str) -> bool:
    """Accept ``every phase`` or a comma list of phases with optional notes."""
    if phase == EVERY_PHASE:
        return True
    stripped = re.sub(r"\([^)]*\)", "", phase)
    tokens = [token.strip() for token in stripped.split(",")]
    return bool(tokens) and all(token in PHASES for token in tokens)


def spec_tags(text: str, source: str) -> tuple[dict[str, str], list[str]]:
    """Return tagged IDs with their first location, plus malformed tags."""
    tags: dict[str, str] = {}
    errors: list[str] = []
    for number, line in enumerate(text.splitlines(), start=1):
        for req_id in TAG_PATTERN.findall(line):
            if not ID_PATTERN.match(req_id):
                errors.append(f"{source}:{number}: malformed requirement tag {req_id!r}")
                continue
            tags.setdefault(req_id, f"{source}:{number}")
    return tags, errors


def untagged_normative_lines(
    text: str, source: str, informative: frozenset[str] = INFORMATIVE_SECTIONS
) -> list[str]:
    """Report normative list items and "must" lines that carry no tag.

    A tagged lead line ending in ``:`` covers the list or table directly below
    it. Table header and separator rows are never requirements.
    """
    errors: list[str] = []
    section = ""
    in_code = False
    covered = False
    table_covered = False
    previous_was_table = False
    for number, line in enumerate(text.splitlines(), start=1):
        if line.lstrip().startswith("```"):
            in_code = not in_code
            continue
        heading = HEADING_PATTERN.match(line)
        if in_code or heading:
            if heading:
                section = "" if len(heading.group(1)) == 1 else heading.group(2)
                covered = False
            continue
        stripped = line.strip()
        is_table = stripped.startswith("|")
        is_table_header = is_table and not previous_was_table
        previous_was_table = is_table
        if not stripped:
            continue
        has_tag = bool(TAG_PATTERN.search(line))
        is_item = bool(ITEM_PATTERN.match(line))
        if is_table:
            if section in informative or table_covered or has_tag:
                continue
            if is_table_header or TABLE_SEPARATOR_PATTERN.match(stripped):
                continue
            errors.append(f"{source}:{number}: normative table row in '{section}' has no REQ tag")
            continue
        if not is_item:
            lead = TAG_PATTERN.sub("", stripped).rstrip()
            covered = has_tag and lead.endswith(":")
            table_covered = covered
        if section in informative or has_tag:
            continue
        if is_item and (covered or INFORMATIVE_MARKER in line):
            continue
        if is_item or MUST_PATTERN.search(line):
            errors.append(f"{source}:{number}: normative item in '{section}' has no REQ tag")
    return errors


def crate_directories(root: Path) -> dict[str, Path]:
    """Map Rust crate names (underscored) to their directories."""
    crates: dict[str, Path] = {}
    for manifest in sorted(root.glob("*/*/Cargo.toml")):
        if manifest.parts[-3] not in {"crates", "apps"}:
            continue
        match = PACKAGE_NAME_PATTERN.search(manifest.read_text(encoding="utf-8"))
        if match:
            crates[match.group(1).replace("-", "_")] = manifest.parent
    return crates


def test_exists(crate_dir: Path, name: str) -> bool:
    """Return whether an active (not ``#[ignore]``) ``#[test] fn name`` exists.

    Comments are stripped first so commented-out tests do not count.
    """
    pattern = re.compile(
        r"((?:#\[[^\]]*\]\s*)+)(?:pub(?:\([^)]*\))?\s+)?fn\s+" + re.escape(name) + r"\s*\("
    )
    for path in sorted(crate_dir.rglob("*.rs")):
        if "target" in path.relative_to(crate_dir).parts:
            continue
        source = COMMENT_PATTERN.sub("", path.read_text(encoding="utf-8"))
        for match in pattern.finditer(source):
            attributes = re.findall(r"#\[\s*([A-Za-z_]+)", match.group(1))
            if "test" in attributes and "ignore" not in attributes:
                return True
    return False


def ci_names(root: Path) -> tuple[set[str], set[str]]:
    """Return the job IDs and step names declared in the CI workflow."""
    path = root / CI_WORKFLOW
    if not path.exists():
        return set(), set()
    jobs: set[str] = set()
    steps: set[str] = set()
    in_jobs = False
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("jobs:"):
            in_jobs = True
            continue
        if in_jobs and line and not line.startswith(" "):
            in_jobs = False
        job = JOB_PATTERN.match(line)
        if in_jobs and job:
            jobs.add(job.group(1))
        step = STEP_PATTERN.match(line)
        if step:
            steps.add(step.group(1).strip("\"'"))
    return jobs, steps


def check_references(rows: list[Row], root: Path, matrix_dir: Path) -> list[str]:
    """Check test references, repository paths, links, and test citations."""
    errors: list[str] = []
    crates = crate_directories(root)
    jobs, steps = ci_names(root)
    tested_rank = STATES.index("automated-tested")
    for row in rows:
        where = f"{MATRIX}:{row.line}: {row.req_id}"
        cited = False
        for kind, name in CI_REF_PATTERN.findall(row.evidence):
            if name in (jobs if kind == "job" else steps):
                cited = True
            else:
                errors.append(f"{where}: CI {kind} {name!r} not found in {CI_WORKFLOW}")
        for label, cell in (("method", row.method), ("evidence", row.evidence)):
            for token in BACKTICK_PATTERN.findall(cell):
                test_ref = TEST_REF_PATTERN.match(token)
                if test_ref and "::" in token:
                    crate, name = test_ref.group(1), test_ref.group(2)
                    if crate not in crates:
                        errors.append(f"{where}: unknown crate {crate!r} in `{token}`")
                    elif not test_exists(crates[crate], name):
                        errors.append(f"{where}: no #[test] fn {name} in crate {crate}")
                    elif label == "evidence":
                        cited = True
                elif "/" in token and not (root / token).exists():
                    errors.append(f"{where}: referenced path `{token}` does not exist")
            for target in LINK_PATTERN.findall(cell):
                path = target.split("#", 1)[0]
                if not path or "://" in path or path.startswith("mailto:"):
                    continue
                resolved = matrix_dir / path
                if not resolved.exists():
                    errors.append(f"{where}: linked evidence {target!r} does not exist")
                elif ".github/workflows" in resolved.resolve().as_posix():
                    cited = True
        if row.state in STATES and STATES.index(row.state) >= tested_rank and not cited:
            errors.append(f"{where}: state {row.state} cites no test or CI job")
    return errors


def check_coverage(rows: list[Row], tags: dict[str, str]) -> list[str]:
    """Check that tagged IDs and matrix rows correspond one to one."""
    errors: list[str] = []
    row_ids = {row.req_id for row in rows}
    for req_id, where in sorted(tags.items()):
        if req_id not in row_ids:
            errors.append(f"{where}: {req_id} has no row in {MATRIX}")
    for row in rows:
        if row.req_id not in tags:
            errors.append(
                f"{MATRIX}:{row.line}: {row.req_id} is not tagged in {SPEC} or {SECURITY}"
            )
    return errors


def check(root: Path) -> list[str]:
    """Run every traceability check against a repository root."""
    matrix_path = root / MATRIX
    rows, errors = parse_matrix(matrix_path.read_text(encoding="utf-8"))
    errors += check_rows(rows)
    tags: dict[str, str] = {}
    for source in (SPEC, SECURITY):
        path = root / source
        if not path.exists():
            errors.append(f"{source}: missing")
            continue
        text = path.read_text(encoding="utf-8")
        found, tag_errors = spec_tags(text, source)
        errors += tag_errors
        for req_id, where in found.items():
            tags.setdefault(req_id, where)
        if source == SPEC:
            errors += untagged_normative_lines(text, source)
    errors += check_coverage(rows, tags)
    errors += check_references(rows, root, matrix_path.parent)
    return errors


def main() -> int:
    """Print every finding and return a non-zero status if any exist."""
    errors = check(ROOT)
    for error in errors:
        print(f"error: {error}", file=sys.stderr)
    if errors:
        print(f"traceability check failed with {len(errors)} error(s)", file=sys.stderr)
        return 1
    print("traceability check passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
