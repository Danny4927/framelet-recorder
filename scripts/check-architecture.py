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


def crate_roots_for(manifest_path: Path) -> list[Path]:
    """Return conventional and explicitly configured Rust compilation roots."""
    package = tomllib.loads(manifest_path.read_text(encoding="utf-8")).get("package", {})
    package_dir = manifest_path.parent
    targets: set[Path] = set()

    # Cargo build scripts are independent Rust compilation units.
    build_script = package_dir / package.get("build", "build.rs")
    if package.get("build") is not False and build_script.is_file():
        targets.add(build_script)

    lib = package.get("lib")
    if lib:
        targets.add(package_dir / lib.get("path", "src/lib.rs"))
    else:
        targets.add(package_dir / "src/lib.rs")

    # Cargo's automatic targets remain enabled alongside explicit targets.
    targets.add(package_dir / "src/main.rs")
    targets.update((package_dir / "src/bin").rglob("*.rs"))

    for target_name in ("bin", "example", "test", "bench"):
        for target in package.get(target_name, []):
            if "path" in target:
                targets.add(package_dir / target["path"])

    for directory in ("tests", "examples", "benches"):
        targets.update((package_dir / directory).rglob("*.rs"))

    return sorted(path for path in targets if path.is_file())


def check_first_party_crates() -> None:
    checked_manifests = 0
    checked_roots = 0
    for root in FIRST_PARTY_CRATES:
        if not root.exists():
            continue
        for manifest_path in root.rglob("Cargo.toml"):
            if "target" in manifest_path.parts:
                continue
            crate_roots = crate_roots_for(manifest_path)
            if not crate_roots:
                continue
            for source in crate_roots:
                text = source.read_text(encoding="utf-8")
                if "#![forbid(unsafe_code)]" not in text:
                    fail(
                        f"{source.relative_to(ROOT)} must declare "
                        "#![forbid(unsafe_code)] at crate root"
                    )
                checked_roots += 1
            checked_manifests += 1
    print(
        "First-party Rust safety declarations verified "
        f"({checked_roots} compilation roots in {checked_manifests} manifests)."
    )


def dependency_names(manifest: dict) -> set[str]:
    """Collect direct dependencies, including target-specific dependency tables."""
    names: set[str] = set()
    for section in ("dependencies", "dev-dependencies", "build-dependencies"):
        names.update(manifest.get(section, {}))
    for target in manifest.get("target", {}).values():
        if isinstance(target, dict):
            names.update(dependency_names(target))
    return names


def check_core_boundary() -> None:
    manifest_path = ROOT / "crates/core/Cargo.toml"
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    unexpected = dependency_names(manifest) - {"framelet-capture-api"}
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
