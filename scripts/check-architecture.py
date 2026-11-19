"""Guard first-party Rust safety and the core's inward-only dependency boundary."""

from pathlib import Path
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
FIRST_PARTY_CRATES = (
    ROOT / "apps",
    ROOT / "crates",
    ROOT / "poc",
)
FORBIDDEN_CORE_REFERENCES = (
    "slint::",
    "cpal::",
    "pipewire::",
    "gstreamer::",
    "ffmpeg::",
)


def fail(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(1)


def check_first_party_crates() -> None:
    checked = 0
    for root in FIRST_PARTY_CRATES:
        if not root.exists():
            continue
        for manifest_path in root.rglob("Cargo.toml"):
            if manifest_path.parent.name == "target":
                continue
            source_root = manifest_path.parent / "src"
            if not source_root.exists():
                continue

            rust_sources = list(source_root.rglob("*.rs"))
            if not rust_sources:
                continue

            for source in rust_sources:
                text = source.read_text(encoding="utf-8")
                if "#![forbid(unsafe_code)]" not in text and "#![forbid(unsafe_code)]" not in "\n".join(
                    line for line in text.splitlines() if line.lstrip().startswith("#![")
                ):
                    fail(
                        f"{source.relative_to(ROOT)} must declare "
                        "#![forbid(unsafe_code)] at crate root"
                    )
            checked += 1

    print(f"First-party Rust crate safety declarations verified ({checked} crates).")


def check_core_boundary() -> None:
    manifest_path = ROOT / "crates/core/Cargo.toml"
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    dependency_sections = (
        manifest.get("dependencies", {}),
        manifest.get("build-dependencies", {}),
        manifest.get("dev-dependencies", {}),
    )
    dependencies = set().union(*(set(section) for section in dependency_sections))
    allowed = {"framelet-capture-api"}
    unexpected = dependencies - allowed
    if unexpected:
        fail(f"framelet-core has forbidden direct dependencies: {sorted(unexpected)}")

    for source in (ROOT / "crates/core/src").rglob("*.rs"):
        text = source.read_text(encoding="utf-8")
        found = [name for name in FORBIDDEN_CORE_REFERENCES if name in text]
        if found:
            fail(
                f"{source.relative_to(ROOT)} references adapter/runtime APIs: {found}"
            )

    print("Core inward-only dependency boundary verified.")


if __name__ == "__main__":
    check_first_party_crates()
    check_core_boundary()
