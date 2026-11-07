# Changelog

## 2.0.0 — 2026-11-07

A rework of PrivaVault as a cross-platform (Linux + Windows) app with a new, safer vault format.
See `docs/PROJECT_AUDIT.md` and `docs/SECURITY_AUDIT.md` for the reasoning.

### Security
- Fixed: vault entry names could inject HTML/script into the app (stored XSS).
- Fixed: the app loaded a script from a CDN at runtime; all assets are now bundled and a strict CSP applies.
- Fixed: decrypted archives were written to the temp directory; nothing is written to temp anymore.
- Fixed: plaintext was released before integrity was verified; every byte is now authenticated first.
- Fixed: a crafted vault header could freeze the app (unbounded PBKDF2 iterations).
- Fixed: extraction could follow unsafe paths if the ZIP library missed them; own path validation added.
- Changed: the GitHub update check is now opt-in and off by default (was: every launch, undisclosed).
- New vault format v2: scrypt (128–512 MiB), HKDF key separation, AES-256-GCM in authenticated 64 KiB
  chunks (STREAM), authenticated header. v1 vaults remain readable; v2 is not readable by 1.x.

### Platforms
- Linux is a first-class target: Wayland/KDE Plasma integration, XDG paths, native portal dialogs,
  `.pvault` MIME type, AppImage/.deb/.rpm and an Arch PKGBUILD.
- Windows: per-user NSIS installer and portable exe (built in CI).
- Electron replaced by Tauri 2; the crypto engine is now a Rust library with 31 tests.

### App
- New interface: sidebar navigation, light/dark/system theme, welcome screen, vault browser with filter,
  per-file extraction without decrypting the whole vault, accurate error messages, progress with speed
  and time left, cancel.
- Settings are saved (theme, language, protection strength, extension, notifications, auto-lock).
- Open vaults auto-lock after inactivity (default 10 minutes).
- Extraction never overwrites existing files; failed or cancelled operations leave no partial files.
- Closing the window quits instead of hiding to the tray.
- Removed: confetti.

### Quality
- 40 Rust tests (RFC 7914 KDF vectors, GCM cross-check, real 1.4.0 vaults, tampering, traversal,
  cancellation, permissions), 8 UI tests, and a 40-check end-to-end test that drives the real window.
- Throughput ≈ 630 MiB/s encrypt / 660 MiB/s decrypt (release build, NVMe, incl. fsync).

## 1.4.0
- English localisation, ZIP extraction support (Electron version).
