# Architecture (PrivaVault 2.x)

```
┌──────────────────────── ui/ (static HTML/CSS/JS, no build step) ───────────────────────┐
│ index.html · css/app.css · js/app.js · js/i18n.js (EN/TR) · js/util.js · fonts/ (Plex) │
│ Renders state, validates input, shows progress. Never builds HTML from data, never     │
│ chooses output paths, no network unless the opt-in update check is enabled.           │
└───────────────▲─────────────────────────────── invoke() / events ───────────────────────┘
                │ Tauri IPC (capabilities/default.json: events + window basics only)
┌───────────────┴──────────── src-tauri/ (app shell, Rust) ──────────────────────────────┐
│ lib.rs       commands, AppState, one-job-at-a-time, progress throttling, cancellation  │
│              native dialogs (rfd → XDG portal on KDE/GNOME, Win32 on Windows)           │
│ settings.rs  JSON settings in the platform config dir, 0600, corrupt-file recovery      │
│ platform.rs  OS detection, XDG/APPDATA dirs, Wayland/X11 detection, argv/file:// paths, │
│              WebKitGTK + NVIDIA workaround                                              │
└───────────────▲────────────────────────────────────────────────────────────────────────┘
                │ plain Rust API, no GUI types
┌───────────────┴──────────── crates/privavault-core/ (library) ─────────────────────────┐
│ lib.rs    Vault (V1|V2), create_vault, extract_entry/extract_all/export_zip, Job        │
│ v2.rs     current format: header, manifest, STREAM chunk writer/reader                  │
│ v1.rs     read-only 1.x support: GHASH verification + seekable CTR → zip crate          │
│ kdf.rs    scrypt (v2), PBKDF2 (v1), parameter bounds                                    │
│ paths.rs  entry-name validation (zip-slip), unique names                                │
│ fsutil.rs atomic owner-only output files                                                │
│ error.rs  error enum with stable codes for the UI                                       │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

**Why this split:** everything that touches keys, file formats or the filesystem lives in a
GUI-free library that is tested with `cargo test` on Linux *and* Windows CI. The Tauri layer is thin
and mostly glue; the UI is replaceable. There is no platform-specific code in the core except
`#[cfg(unix)]` permission bits and `#[cfg(windows)]` reserved-name rules.

## Data flow

**Lock files:** UI collects input paths (picker or drag & drop) → `encrypt(paths, password, name)` →
Rust opens the save dialog → `create_vault` walks inputs (no symlink following), writes
`header ‖ manifest ‖ files` as authenticated chunks into `.<name>.<rand>.pvpart` → fsync → rename.

**Open a vault:** `open_vault(path, password)` → format detection (magic `PRIVAVLT` = v2, else v1) →
key derivation → v2: key check + decrypt manifest only (instant listing) / v1: full authentication pass →
the `Vault` (holding the key) is stored in `AppState.session` until locked.
**Extract:** Rust opens the dialog → chunks are decrypted and authenticated one by one straight into
an atomic output file. Extract-all creates a new folder and never overwrites.

## Vault format v2

See the module documentation in `crates/privavault-core/src/v2.rs` for the byte layout and
`docs/SECURITY_AUDIT.md §3` for the cryptographic rationale. Summary:

| Part | Size | Notes |
| --- | --- | --- |
| Header | 80 B | magic, version, KDF id + params, cipher id, chunk size, salt, nonce prefix, lengths, key check |
| Chunks | (64 KiB + 16) each | AES-256-GCM, nonce = prefix ‖ counter ‖ last-flag, AAD = header |
| Plaintext | — | `manifest JSON ‖ file data…` (manifest: path, kind, size, mtime, exec bit) |

Compatibility rules: readers reject unknown versions/algorithms and non-zero reserved bytes (so a future
format change is an explicit version bump, never a silent downgrade). v1 files are read-only;
PrivaVault 2 never writes v1.

## Threads & state

* UI thread: never blocks. Every command that does I/O or key derivation is `async` and uses
  `spawn_blocking`.
* One operation at a time (`busy` flag + `BusyGuard`). Cancellation is a shared `AtomicBool` checked per
  chunk/MiB; cancelled outputs are removed by `AtomicFile`'s `Drop`.
* Closing the window during an operation asks for confirmation; "quit" cancels first and exits once the
  partial file is gone.

## Platform specifics

| Concern | Linux (Arch/CachyOS, Debian/Ubuntu) | Windows 10/11 |
| --- | --- | --- |
| WebView | WebKitGTK 4.1 (system) | WebView2 (preinstalled on 11; installer bootstraps on 10) |
| Settings | `$XDG_CONFIG_HOME/privavault/settings.json` (fallback `~/.config`) | `%APPDATA%\PrivaVault\settings.json` |
| Permissions | 0600 files, 0700 dirs | inherited ACL |
| Dialogs | XDG Desktop Portal (KDE/GNOME native) via `rfd` | Win32 common dialogs |
| Window identity | Wayland `app_id` = `com.privavault.app`; the desktop file is named `com.privavault.app.desktop` so KWin/Mutter find the icon | AppUserModelID via installer |
| File association | shared-mime-info XML (`application/x-privavault`, magic + `*.pvault`), `MimeType=` in desktop file | NSIS registers `.pvault` |
| Open-with / 2nd launch | `%F` args, `file://` URIs decoded, forwarded by single-instance plugin | same |
| Packages | AppImage, .deb, .rpm, Arch `PKGBUILD` | NSIS installer (per-user), portable `.exe` |

## Why Tauri (decision record)

Chosen by the project owner over upgrading Electron. Consequences:
* + Crypto moves from JS into Rust with RustCrypto crates; no Node runtime in the product.
* + ~10 MB binaries instead of ~100 MB; system WebView patched by the OS.
* + Capability model: the renderer gets no filesystem/dialog/shell permissions at all.
* − Rendering differences between WebKitGTK and WebView2 must be tested on both.
* − WebKitGTK quirks on Linux (NVIDIA + Wayland DMA-BUF); mitigated in `platform.rs`.
* − Requires WebView2 on Windows 10 machines without it (installer downloads it).
