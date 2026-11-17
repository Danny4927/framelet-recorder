"""Guard the core's inward-only dependency boundary."""

from pathlib import Path
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
manifest = tomllib.loads((ROOT / "crates/core/Cargo.toml").read_text(encoding="utf-8"))
dependencies = set(manifest.get("dependencies", {}))
allowed = {"framelet-capture-api"}
unexpected = dependencies - allowed

if unexpected:
    print(f"framelet-core has forbidden direct dependencies: {sorted(unexpected)}", file=sys.stderr)
    raise SystemExit(1)

for source in (ROOT / "crates/core/src").rglob("*.rs"):
    text = source.read_text(encoding="utf-8")
    forbidden = ("slint::", "cpal::", "pipewire::", "gstreamer::", "ffmpeg::")
    found = [name for name in forbidden if name in text]
    if found:
        print(f"{source.relative_to(ROOT)} imports adapter/runtime APIs: {found}", file=sys.stderr)
        raise SystemExit(1)

print("Core dependency boundary is valid.")
