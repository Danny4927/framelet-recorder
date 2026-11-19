"""Guard first-party Rust safety and the core's inward-only dependency boundary."""

from pathlib import Path
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
FIRST_PARTY_CRATES = (ROOT / "apps", ROOT / "crates", ROOT / "poc")
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


def crate_root_for(manifest_path: Path) -> list[Path]:
    package = tomllib.loads(manifest_path.read_text(encoding="utf-8")).get("package", {})
    targets = []
    lib = package.get("lib")
    if lib:
        targets.append(manifest_path.parent / lib.get("path", "src/lib.rs"))
    else:
        default_lib = manifest_path.parent / "src/lib.rs"
        if default_lib.exists():
            targets.append(default_lib)

    for binary in package.get("bin", []):
        targets.append(manifest_path.parent / binary.get("path", "src/main.rs"))
    if not package.get("bin"):
        default_main = manifest_path.parent / "src/main.rs"
        if default_main.exists():
            targets.append(default_main)

    for target in ("examples", "benches", "tests"):
        targets.extend((manifest_path.parent / target).glob("*.rs"))
    return [path for path in targets if path.exists()]


def check_first_party_crates() -> None:
    checked = 0
    for root in FIRST_PARTY_CRATES:
        if not root.exists():
            continue
        for manifest_path in root.rglob("Cargo.toml"):
            if "target" in manifest_path.parts:
                continue
            crate_roots = crate_root_for(manifest_path)
            if not crate_roots:
                continue
            for source in crate_roots:
                text = source.read_text(encoding="utf-8")
                if "#![forbid(unsafe_code)]" not in text:
                    fail(
                        f"{source.relative_to(ROOT)} must declare "
                        "#![forbid(unsafe_code)] at crate root"
                    )
            checked += 1
    print(f"First-party Rust crate safety declarations verified ({checked} manifests).")


def check_core_boundary() -> None:
    manifest_path = ROOT / "crates/core/Cargo.toml"
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    dependency_sections = (
        manifest.get("dependencies", {}),
        manifest.get("build-dependencies", {}),
        manifest.get("dev-dependencies", {}),
    )
    dependencies = set().union(*(set(section) for section in dependency_sections))
    unexpected = dependencies - {"framelet-capture-api"}
    if unexpected:
        fail(f"framelet-core has forbidden direct dependencies: {sorted(unexpected)}")

    for source in (ROOT / "crates/core/src").rglob("*.rs"):
        text = source.read_text(encoding="utf-8")
        found = [name for name in FORBIDDEN_CORE_REFERENCES if name in text]
        if found:
            fail(f"{source.relative_to(ROOT)} references adapter/runtime APIs: {found}")
    print("Core inward-only dependency boundary verified.")


if __name__ == "__main__":
    check_first_party_crates()
    check_core_boundary()
