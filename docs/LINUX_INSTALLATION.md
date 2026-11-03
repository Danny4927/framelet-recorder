# Linux installation plan

## Target

The primary Linux package target is **Arch Linux and CachyOS on KDE Plasma Wayland**. Wayland is the supported first-class capture path; X11 remains a separate, deferred backend.

## Delivery channels

1. **AUR source package (`framelet`)** — preferred Arch/CachyOS installation. The PKGBUILD builds a tagged Framelet release with the system Rust toolchain and installs the executable, desktop entry, icons, and license metadata.
2. **GitHub release archive** — a signed, architecture-specific tarball for manual installation and testing before the AUR package is mature.
3. **Flatpak** — optional later channel if sandboxing and portal behavior are tested end-to-end. It is not the first release target.
4. **AppImage** — not the preferred channel. Bundling desktop/media libraries can conflict with the host PipeWire and portal stack and complicate troubleshooting.

An official Arch repository package can be considered after the project has stable releases, reproducible packaging, and maintainership capacity.

## KDE Plasma Wayland runtime

Framelet should use the standard XDG ScreenCast portal, not direct compositor scraping. On KDE Plasma, the portal backend is provided by `xdg-desktop-portal-kde`; the portal frontend is `xdg-desktop-portal`. PipeWire transports the selected screen stream.

A typical KDE system already has these components through its desktop/audio setup. Framelet must detect missing components and show actionable diagnostics; it must not silently install packages, enable system services, or modify portal configuration.

Useful Arch package names:

- `xdg-desktop-portal`
- `xdg-desktop-portal-kde`
- `pipewire`
- `wireplumber`

Audio packages such as `pipewire-audio` and `pipewire-pulse` are relevant to the user's broader PipeWire audio setup, but should not be pulled in solely for screen capture if the desktop already has a working audio stack.

## Packaging rules

- Install a normal native executable under `/usr/bin`.
- Install a `.desktop` entry and hicolor application icons.
- Store user configuration and recordings in standard user directories; never write into the application installation directory.
- Do not run as root and do not ship privileged helpers.
- Do not enable or start global/user services on the user's behalf.
- Declare actual linked runtime libraries accurately; do not make all of KDE or PipeWire a hard dependency if it is already supplied by the desktop or only needed by a specific backend.
- Keep portal backend packages as optional dependencies with clear descriptions, because non-KDE Wayland users may use another backend.
- Build from source in the AUR package; do not download unverified prebuilt binaries from a PKGBUILD.
- Pin and audit Cargo dependencies through the committed lockfile; verify release checksums/signatures.
- Provide an uninstall-safe package that removes only Framelet-owned files.

## Planned KDE installation experience

After a stable release and a reviewed PKGBUILD, the intended user flow is:

```bash
# Install from the AUR with the user's preferred AUR helper:
yay -S framelet
```

The exact package name and command are provisional until the package is published. The application should appear in the KDE application launcher as **Framelet**. On first screen recording, KDE presents its normal screen/window chooser and permission flow. The user chooses the target and explicitly grants access.

## Readiness gate

Do not publish an AUR package as a supported release until:

- KDE Plasma Wayland has passed portal selection, actual PipeWire frame delivery, cancel/deny, and clean teardown tests.
- 1080p30 recording has passed a sustained run with bounded memory and no unbounded queues.
- microphone and webcam backends have their own real-device test results.
- dependency/license/unsafe/FFI/RustSec review is complete.
- a release archive, desktop entry, icons, and uninstall behavior have been tested on clean Arch and CachyOS installs.
