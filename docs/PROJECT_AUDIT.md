# PrivaVault — Project Audit (baseline: v1.4.0, commit `a5a470a`)

This audit describes the project **as it was before the 2.0 rework**. Security
findings are expanded in [`SECURITY_AUDIT.md`](SECURITY_AUDIT.md); the new design
is described in [`ARCHITECTURE.md`](ARCHITECTURE.md).

## 1. Inventory

| File | Lines | Role |
| --- | ---: | --- |
| `main.js` | 578 | Electron main process: window, tray, update check, all crypto + archive logic, IPC handlers |
| `preload.js` | 15 | `contextBridge` exposing 11 IPC calls as `window.electronAPI` |
| `index.html` | 1178 | Entire UI: markup, ~470 lines of CSS, ~560 lines of inline JS, EN/TR strings |
| `package.json` | — | Electron 28, electron-builder 24, `archiver` 5, `node-stream-zip` 1.16 |
| `.github/workflows/build.yml` | — | Windows-only build; deletes and recreates a GitHub release on **every push to main** |
| `assets/` | — | `logo.png` (688×674, not square), `logo.ico`, `preview.png` |

There are no tests, no linting, no modules, and no documentation beyond the README.

## 2. Architecture (as found)

```
index.html (renderer, inline JS)  ──ipc──►  preload.js  ──►  main.js
   UI state, i18n, validation                                 ├─ dialogs, tray, notifications
   loads canvas-confetti from jsDelivr                        ├─ checkForUpdates() → api.github.com
                                                              └─ crypto: PBKDF2 + AES-256-GCM over a ZIP stream
```

### Vault format "v1" (reverse-engineered from `main.js`)

```
offset 0   salt        16 bytes  (random)
offset 16  iv          12 bytes  (random)
offset 28  iterations   4 bytes  (uint32 big-endian, PBKDF2-HMAC-SHA256)
offset 32  ciphertext   N bytes  AES-256-GCM over a ZIP archive (archiver, zlib level 0)
offset -16 auth tag    16 bytes
```

* Encryption: `archiver('zip') → cipher → file`; tag appended afterwards with `appendFileSync`.
* "Fast password check": decrypts the first ≤1 KiB **without authentication** and checks for the `PK` ZIP magic.
* Inspect / extract: decrypts the **whole** archive to `%TEMP%/pvault_dec_<ts>.zip` (plaintext on disk),
  then opens it with `node-stream-zip`, then `unlink`s it.

## 3. Strengths

* Uses standard primitives (AES-256-GCM, PBKDF2-SHA256) from Node's crypto, random salt and IV per vault.
* `contextIsolation: true`, `nodeIntegration: false`.
* Streaming encryption: memory use stays flat for large inputs.
* Derived keys are zero-filled after use (best effort).
* Simple, approachable UI with EN/TR localisation and a password strength hint.

## 4. Weaknesses and bugs

### Security (details and severity in SECURITY_AUDIT.md)

1. **Stored XSS from vault contents.** Entry names from inside a vault are interpolated into `innerHTML`
   and an inline `onclick='…'`. A crafted vault (sent with its password) runs script in the renderer,
   with access to the typed password and the `electronAPI` bridge.
2. **Remote code at runtime.** `canvas-confetti` is loaded from jsDelivr without SRI or CSP: a network
   call on every launch, and a supply-chain path into the renderer.
3. **Undisclosed network traffic.** `checkForUpdates()` calls `api.github.com` on every launch. The
   result is never shown (no renderer listener). This contradicts the README's "no network calls".
4. **Plaintext temp files.** Every inspect/extract writes the full decrypted ZIP to the system temp
   directory. It is unlinked afterwards (not overwritten) and survives crashes.
5. **Unauthenticated plaintext is released.** GCM's tag is only checked at the end of the stream, after the
   plaintext has been written to disk.
6. **Header not authenticated; iteration count attacker-controlled.** A tampered header with
   `iterations = 0xFFFFFFFF` makes `pbkdf2Sync` freeze the main process for hours (DoS).
7. **No CSP, no `sandbox`, no navigation / `window.open` restrictions** on the BrowserWindow.
8. **Electron 28 is end-of-life** (unpatched Chromium/V8 CVEs).
9. **Weak defaults.** 100k PBKDF2 iterations is below the OWASP 2023 guidance (600k); the setting is
   not persisted, so every launch resets to 100k.
10. **Zip-slip exposure depends on the library.** `zip.extract(null, folder)` relies entirely on
    `node-stream-zip`'s path checks; there is no own validation.

### Functional bugs

* `pbkdf2Sync` runs on the Electron main thread, so the whole UI freezes during key derivation (twice per
  operation: once for the "fast check", once for the real decrypt).
* Every failure (disk full, permission denied, missing file) is reported as "Incorrect password".
* Partial output files are left behind when encryption fails midway.
* Closing the window **hides** it to the tray. If the tray cannot be created (common on some Linux desktops),
  the app keeps running invisibly with no way to quit except killing it.
* Settings (KDF rounds, extension) are not persisted; "Save" just closes the modal.
* Progress for encryption counts ZIP output bytes against input size, so the percentage is wrong.
* Inspect = full decrypt; a 20 GB vault needs 20 GB of free temp space and minutes just to list it.
* "Extract" for a single entry decrypts the entire vault again.
* Entry names containing `'` break the inline `onclick` handler.
* Folders containing symlinks are followed by `archiver` (loops / unexpected content).
* No single-instance handling; "open with" works only on Windows (`process.argv[1]`).
* `File.path` used for drag & drop was removed in newer Electron.
* CI deletes and recreates the release on every push to `main`.
* `getTotalSize` is synchronous and recursive (blocks the main thread on large trees).

### Cross-platform blockers

* File-association handling gated on `process.platform === 'win32'`.
* No Linux targets in `electron-builder` config; non-square PNG icon (Linux icon themes expect square).
* Tray-hide behaviour is unreliable on Linux.
* Paths are split with `'\\'` and `'/'` ad hoc in the renderer.
* README/CI documented and built Windows only.

## 5. Decisions taken (confirmed with the project owner)

| Topic | Decision | Rationale |
| --- | --- | --- |
| Framework | **Migrate Electron → Tauri 2 (Rust)** | Crypto moves into Rust with audited RustCrypto crates; small native binaries; strict capability model; system WebView (WebKitGTK on Linux, WebView2 on Windows) |
| Vault format | **New v2 format for new vaults; v1 stays readable** | Chunk-authenticated streaming AEAD removes temp files and unauthenticated plaintext, and enables instant listing and single-file extraction |
| Update check | **Opt-in setting, off by default** | No network traffic without consent |

## 6. Improvement plan (Phase 2)

1. **Core library** (`crates/privavault-core`), with no GUI dependencies:
   * v2 writer/reader: scrypt KDF, HKDF key separation, AES-256-GCM STREAM (RustCrypto `aead::stream`),
     an authenticated header, an encrypted manifest, and random-access chunk decryption.
   * v1 reader: verify the GCM tag over the entire ciphertext **first** (GHASH only, no plaintext), then
     decrypt via a seekable CTR reader fed straight into a ZIP parser. No temp files.
   * Strict path sanitisation for every extracted entry; never follow or create symlinks.
   * Atomic output (`*.partial` + rename), never overwriting existing files, cancellation, progress callbacks.
2. **Tauri app** (`src-tauri`): thin command layer, background jobs, settings in the platform config dir with
   owner-only permissions, single-instance + "open with", native dialogs (XDG portal on KDE/Wayland),
   native notifications.
3. **UI** (`ui/`): static HTML/CSS/JS with no runtime network and a strict CSP. Sidebar navigation, light/dark
   themes, first-run screen, security status, accessible controls, EN/TR.
4. **Tests**: core unit and integration tests (round-trips, tampering, wrong password, truncation, path
   traversal, real v1 fixtures produced by the original JS code), plus app-level settings/path tests.
5. **Packaging/CI**: Linux (AppImage, deb, rpm, Arch PKGBUILD) and Windows (NSIS + portable exe) on GitHub
   Actions, with tests on both OSes. Releases only on tags.
6. **Docs**: architecture, security audit + threat model, setup per OS, building, changelog.
