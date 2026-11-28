#!/usr/bin/env python3
"""Unit tests for scripts/check-traceability.py (standard library only)."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "check_traceability", Path(__file__).with_name("check-traceability.py")
)
assert _SPEC is not None and _SPEC.loader is not None
ct = importlib.util.module_from_spec(_SPEC)
sys.modules[_SPEC.name] = ct
_SPEC.loader.exec_module(ct)

HEADER = (
    "| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |\n"
    "|---|---|---|---|---|---|---|\n"
)


def row(req_id="REQ-LIFE-01", phase="A", method="Unit", evidence="—", need="open", state="open"):
    return f"| {req_id} | Something (Spec §1) | {phase} | {method} | {evidence} | {need} | {state} |\n"


class Repo:
    """A throwaway repository layout with one crate and the three documents."""

    def __init__(self, spec: str, rows: str, security: str = "# Security\n") -> None:
        self._dir = tempfile.TemporaryDirectory()
        self.root = Path(self._dir.name)
        crate = self.root / "crates" / "core"
        (crate / "src").mkdir(parents=True)
        (crate / "Cargo.toml").write_text('[package]\nname = "framelet-core"\n', encoding="utf-8")
        (crate / "src" / "lib.rs").write_text(
            "#[cfg(test)]\nmod tests {\n    #[test]\n    fn real_test() {}\n"
            "    fn not_a_test() {}\n    #[test]\n    #[ignore]\n    fn ignored_test() {}\n"
            "    // #[test]\n    // fn commented_test() {}\n}\n",
            encoding="utf-8",
        )
        (self.root / "docs" / "verification" / "evidence").mkdir(parents=True)
        (self.root / "docs" / "verification" / "evidence" / "run.md").write_text("x", encoding="utf-8")
        (self.root / ".github" / "workflows").mkdir(parents=True)
        (self.root / ct.CI_WORKFLOW).write_text(
            "name: CI\njobs:\n  linux:\n    steps:\n      - name: Check requirement traceability\n"
            "        run: python scripts/check-traceability.py\n",
            encoding="utf-8",
        )
        (self.root / "SPECIFICATION.md").write_text(spec, encoding="utf-8")
        (self.root / "SECURITY.md").write_text(security, encoding="utf-8")
        (self.root / ct.MATRIX).write_text("# Matrix\n\n" + HEADER + rows, encoding="utf-8")

    def errors(self) -> list[str]:
        return ct.check(self.root)

    def close(self) -> None:
        self._dir.cleanup()


SPEC_OK = "# Spec\n\n## 1. Area\n\n- Do the thing. [REQ-LIFE-01]\n"


class CheckTraceabilityTest(unittest.TestCase):
    def run_repo(self, spec: str, rows: str, security: str = "# Security\n") -> list[str]:
        repo = Repo(spec, rows, security)
        self.addCleanup(repo.close)
        return repo.errors()

    def assert_error(self, errors: list[str], fragment: str) -> None:
        self.assertTrue(any(fragment in error for error in errors), f"{fragment!r} not in {errors}")

    def test_consistent_repository_passes(self) -> None:
        self.assertEqual(self.run_repo(SPEC_OK, row()), [])

    def test_real_repository_passes(self) -> None:
        self.assertEqual(ct.check(ct.ROOT), [])

    def test_malformed_and_duplicate_ids_are_rejected(self) -> None:
        errors = self.run_repo(SPEC_OK, row() + row() + row(req_id="REQ-life-1"))
        self.assert_error(errors, "duplicate ID REQ-LIFE-01")
        self.assert_error(errors, "malformed requirement ID 'REQ-life-1'")

    def test_named_suffix_ids_are_accepted(self) -> None:
        spec = "## 1. Area\n\n- Mic mode. [REQ-MODE-MIC]\n"
        self.assertEqual(self.run_repo(spec, row(req_id="REQ-MODE-MIC")), [])

    def test_invalid_phase_and_state_are_rejected(self) -> None:
        errors = self.run_repo(SPEC_OK, row(phase="Z", need="done", state="finished"))
        self.assert_error(errors, "invalid phase 'Z'")
        self.assert_error(errors, "invalid required state 'done'")
        self.assert_error(errors, "invalid state 'finished'")

    def test_phase_vocabulary(self) -> None:
        for phase in ("A", "every phase", "B, D, H", "Foundation (verified in A)", "C (WAV), D (target format)"):
            self.assertTrue(ct.phase_is_valid(phase), phase)
        for phase in ("", "I", "A; B", "every", "phase A"):
            self.assertFalse(ct.phase_is_valid(phase), phase)

    def test_spec_id_without_row_is_rejected(self) -> None:
        spec = SPEC_OK + "- Another thing. [REQ-LIFE-02]\n"
        self.assert_error(self.run_repo(spec, row()), "REQ-LIFE-02 has no row")

    def test_row_without_spec_tag_is_rejected(self) -> None:
        errors = self.run_repo(SPEC_OK, row() + row(req_id="REQ-LIFE-02"))
        self.assert_error(errors, "REQ-LIFE-02 is not tagged")

    def test_security_tags_satisfy_rows(self) -> None:
        rows = row() + row(req_id="REQ-SEC-01")
        self.assertEqual(self.run_repo(SPEC_OK, rows, "# Security\n\nOffline. [REQ-SEC-01]\n"), [])

    def test_untagged_normative_item_and_must_line_are_rejected(self) -> None:
        spec = SPEC_OK + "- Untagged item.\n\nThe app must work.\n"
        errors = self.run_repo(spec, row())
        self.assert_error(errors, "SPECIFICATION.md:6: normative item in '1. Area'")
        self.assert_error(errors, "SPECIFICATION.md:8: normative item")

    def test_informative_sections_markers_code_and_tables_are_exempt(self) -> None:
        spec = (
            "# Spec\n\n- preamble item\n\n## 3. Recommended technology direction\n\n- candidate\n"
            "\n## 1. Area\n\n- Do the thing. [REQ-LIFE-01]\n- Not required. *(informative)*\n"
            "\n```text\n- code must not count\n```\n"
        )
        self.assertEqual(self.run_repo(spec, row()), [])

    def test_tagged_lead_line_covers_its_list_only(self) -> None:
        spec = (
            "## 1. Area\n\nFields must include: [REQ-LIFE-01]\n- width\n- height\n"
            "\nNext paragraph.\n- uncovered\n"
        )
        errors = self.run_repo(spec, row())
        self.assertEqual(len(errors), 1, errors)
        self.assert_error(errors, "SPECIFICATION.md:8:")

    def test_test_references_must_resolve_to_test_functions(self) -> None:
        ok = row(evidence="`framelet_core::tests::real_test`", need="automated-tested", state="automated-tested")
        self.assertEqual(self.run_repo(SPEC_OK, ok), [])
        errors = self.run_repo(SPEC_OK, row(evidence="`framelet_core::tests::not_a_test`"))
        self.assert_error(errors, "no #[test] fn not_a_test in crate framelet_core")
        errors = self.run_repo(SPEC_OK, row(evidence="`framelet_nope::tests::real_test`"))
        self.assert_error(errors, "unknown crate 'framelet_nope'")
        for name in ("ignored_test", "commented_test"):
            errors = self.run_repo(SPEC_OK, row(evidence=f"`framelet_core::tests::{name}`"))
            self.assert_error(errors, f"no #[test] fn {name} in crate framelet_core")

    def test_referenced_paths_and_evidence_links_must_exist(self) -> None:
        ok = row(method="`crates/core/src/lib.rs`", evidence="[run](evidence/run.md)")
        self.assertEqual(self.run_repo(SPEC_OK, ok), [])
        errors = self.run_repo(SPEC_OK, row(method="`scripts/missing.py`", evidence="[r](evidence/gone.md)"))
        self.assert_error(errors, "referenced path `scripts/missing.py` does not exist")
        self.assert_error(errors, "linked evidence 'evidence/gone.md' does not exist")

    def test_tested_rows_must_cite_a_test_or_ci_job(self) -> None:
        errors = self.run_repo(SPEC_OK, row(need="automated-tested", state="ui-integrated"))
        self.assert_error(errors, "state ui-integrated cites no test or CI job")
        for evidence in ('CI job "linux"', 'CI step "Check requirement traceability"'):
            self.assertEqual(self.run_repo(SPEC_OK, row(evidence=evidence, state="automated-tested")), [])
        link = "[ci](../../.github/workflows/ci.yml)"
        self.assertEqual(self.run_repo(SPEC_OK, row(evidence=link, state="automated-tested")), [])
        errors = self.run_repo(SPEC_OK, row(evidence="CI job", state="automated-tested"))
        self.assert_error(errors, "state automated-tested cites no test or CI job")

    def test_named_ci_job_or_step_must_exist(self) -> None:
        errors = self.run_repo(SPEC_OK, row(evidence='CI job "macos"', state="automated-tested"))
        self.assert_error(errors, "CI job 'macos' not found in .github/workflows/ci.yml")
        self.assert_error(errors, "cites no test or CI job")
        errors = self.run_repo(SPEC_OK, row(evidence='CI step "Missing step"'))
        self.assert_error(errors, "CI step 'Missing step' not found")

    def test_normative_table_rows_need_a_tag_or_tagged_lead(self) -> None:
        table = "| Setting | Default |\n|---|---|\n| Frame rate | 30 FPS |\n"
        errors = self.run_repo("## 1. Area\n\n- Item. [REQ-LIFE-01]\n\n" + table, row())
        self.assertEqual(len(errors), 1, errors)
        self.assert_error(errors, "SPECIFICATION.md:7: normative table row")
        covered = "## 1. Area\n\nDefaults apply: [REQ-LIFE-01]\n\n" + table
        self.assertEqual(self.run_repo(covered, row()), [])
        tagged = "## 1. Area\n\n" + table.replace("30 FPS |", "30 FPS [REQ-LIFE-01] |")
        self.assertEqual(self.run_repo(tagged, row()), [])
        informative = "## 3. Recommended technology direction\n\n" + table
        self.assertEqual(self.run_repo(SPEC_OK + "\n" + informative, row()), [])
        self.assertEqual(self.run_repo(SPEC_OK, row(state="implemented")), [])

    def test_malformed_spec_tag_is_rejected(self) -> None:
        spec = SPEC_OK + "- Bad tag. [REQ-lower-x]\n"
        self.assert_error(self.run_repo(spec, row()), "malformed requirement tag 'REQ-lower-x'")


if __name__ == "__main__":
    unittest.main()
