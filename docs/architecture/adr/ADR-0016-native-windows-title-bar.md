# ADR-0016: Native dark Windows title bar

- **Status:** Accepted for the Windows desktop shell
- **Date:** 17 November 2026
- **Deciders:** Framelet maintainers

## Context

The Slint client area uses Framelet's dark palette, but the default native Windows title bar remains light on systems configured for a light application theme. The previous custom, frameless title bar was removed because it introduced visual-regression and native-window-behavior risks. The standard OS frame must remain responsible for caption buttons, move, resize, maximize, and accessibility behavior.

The application also has a binding safe-Rust policy. First-party unsafe Win32 calls or FFI are therefore not an acceptable solution.

## Decision

Keep native window decorations and use Slint 1.18.1's unstable-winit-030 accessor to reach the existing Winit window on the UI event-loop thread. On Windows, use Winit's safe WindowExtWindows methods to set:

- title-bar background to Framelet's dark surface color (RGB 16, 20, 28);
- title text to the light foreground color (RGB 243, 244, 246);
- native window border to the same dark surface color.

Apply the styling through a one-shot Slint timer after the event loop has created the native window. Do not remove native decorations or implement custom caption buttons/hit-testing. Non-Windows platforms do not execute the Windows styling code.

Winit documents title background/text/border tinting as supported starting with Windows 11 build 22000. On older Windows versions, the native frame may retain the system appearance; this is a documented compatibility fallback, not a reason to introduce undocumented APIs or unsafe code.

## Alternatives considered

1. **Custom frameless title bar:** rejected because it replaces native caption behavior and previously caused visual-regression issues.
2. **Direct DwmSetWindowAttribute FFI:** rejected because it requires unsafe Win32 calls in first-party code, conflicting with the binding safe-Rust constraint.
3. **Third-party DWM wrapper:** rejected for this narrow task because Winit already exposes safe native title-bar color methods through the Slint backend.
4. **Leave the mismatch unchanged:** rejected because the native title bar visibly conflicts with the dark application surface on supported Windows 11 systems.

## Consequences

- Native minimize, maximize/restore, close, move, resize, and OS accessibility behavior remain owned by Windows.
- No custom title-bar controls, window subclassing, or first-party unsafe code are introduced.
- The Slint Winit accessor is an explicitly unstable integration API; the exact Slint pin limits upgrade drift, and a Slint upgrade must revalidate this integration.
- Windows 11 22000+ receives explicit Framelet title-bar colors. Older Windows versions retain their native system appearance.

## Validation

- CI must compile and lint the desktop workspace on Linux and Windows.
- Real-device validation must confirm the title bar colors and native caption behavior on Windows 11.
- Windows 10 behavior must be recorded separately; CI compilation is not visual validation.
