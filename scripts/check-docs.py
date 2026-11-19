"""Validate required architecture documentation and repository-local Markdown links."""

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
ARC42 = ROOT / "docs/architecture/arc42.md"
REQUIRED_ARC42_SECTIONS = (
    "Introduction and Goals",
    "Architecture Constraints",
    "Context and Scope",
    "Solution Strategy",
    "Building Block View",
    "Runtime View",
    "Deployment View",
    "Cross-cutting Concepts",
    "Architecture Decisions",
    "Quality Requirements",
    "Risks and Technical Debt",
    "Glossary",
)
LINK_PATTERN = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")


def fail(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(1)


def check_arc42() -> None:
    content = ARC42.read_text(encoding="utf-8")
    missing = [
        f"## {index}. {title}"
        for index, title in enumerate(REQUIRED_ARC42_SECTIONS, start=1)
        if f"## {index}. {title}" not in content
    ]
    if missing:
        fail(f"arc42 is missing required sections: {missing}")
    print("arc42 contains all 12 required sections.")


def check_markdown_links() -> None:
    markdown_files = [
        path for path in ROOT.rglob("*.md")
        if ".git" not in path.parts and "target" not in path.parts
    ]
    errors = []
    for markdown_file in markdown_files:
        content = markdown_file.read_text(encoding="utf-8")
        for target in LINK_PATTERN.findall(content):
            target = target.strip()
            if not target or target.startswith(("http://", "https://", "mailto:", "#")):
                continue
            target = target.split("#", maxsplit=1)[0].split("?", maxsplit=1)[0]
            if not target:
                continue
            candidate = (markdown_file.parent / target).resolve()
            try:
                candidate.relative_to(ROOT.resolve())
            except ValueError:
                errors.append(f"{markdown_file.relative_to(ROOT)}: link escapes repository: {target}")
                continue
            if not candidate.exists():
                errors.append(f"{markdown_file.relative_to(ROOT)}: missing local link: {target}")

    if errors:
        fail("Broken Markdown links:\n" + "\n".join(errors))
    print(f"Local Markdown links verified across {len(markdown_files)} files.")


if __name__ == "__main__":
    check_arc42()
    check_markdown_links()
