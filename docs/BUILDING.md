# Building & testing PrivaVault

PrivaVault 2 is a Cargo workspace. **Node.js is not needed** to build or run the app (only to run the
small UI test suite and to regenerate legacy test fixtures).

```
crates/privavault-core   vault formats + crypto (library, no GUI deps)
src-tauri                desktop app (Tauri 2)
ui                       static front-end served by the app
tests/ui                 Node tests for the UI (i18n, CSP, XSS regression)
packaging/               Linux desktop/MIME/metainfo files, Arch PKGBUILD
tools/v1-fixtures        regenerates 1.x test vaults with the original JS code
```

## Prerequisites

| | Linux | Windows |
| --- | --- | --- |
| Rust | ≥ 1.90 (`pacman -S rust` or rustup) | rustup, MSVC toolchain |
| System libs | Arch: `webkit2gtk-4.1 gtk3 librsvg` (+ `patchelf` for AppImage) · Debian/Ubuntu: `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev patchelf` | Visual Studio Build Tools (C++), WebView2 |
| Tauri CLI | `cargo install tauri-cli --version "^2" --locked` (or `--root .tools` to keep it in the repo) | same |

## Run & test

```bash
cargo test --workspace            # core (31) + app (9) tests
cargo run --release -p privavault-core --example bench -- 1024   # throughput
node --test tests/ui/ui.test.js              # UI checks (Node ≥ 18)
cargo clippy --workspace --all-targets -- -D warnings

cargo run -p privavault           # debug build of the app
```

Use a throw-away settings dir while developing: `XDG_CONFIG_HOME=/tmp/pv-cfg cargo run -p privavault`.

## Release bundles

```bash
cd src-tauri
cargo tauri build --bundles deb,rpm,appimage    # Linux  → target/release/bundle/{deb,rpm,appimage}
cargo tauri build --bundles nsis                # Windows → target/release/bundle/nsis + target/release/privavault.exe
```

Arch package from the checkout: `cd packaging/arch && PRIVAVAULT_LOCAL_SRC=../.. makepkg -f`.
(A plain `cargo build --release` must add `--features tauri/custom-protocol`, as the PKGBUILD does.)

CI (`.github/workflows/build.yml`) runs tests + clippy on Ubuntu and Windows for every push/PR, and builds
bundles for tags `v*` (uploaded to a **draft** release — nothing is deleted or published automatically).

## Regenerating v1 fixtures

```bash
cd tools/v1-fixtures && npm install
node gen.js out.pvault 'password' 1000 file1 folder2 …
```
`gen.js` is the 1.4.0 `encrypt-files` handler with the UI removed; `gen_zip.js` wraps an arbitrary ZIP
(used for the malicious-names fixture).

## GUI end-to-end test (Linux)

```bash
cargo build -p privavault
python3 tests/gui/e2e.py --shots /tmp/pv-shots     # 40 checks, ~1 minute, opens a real window
```

The script starts the **debug** build with a throw-away config dir and drives the real window:
user input through DOM events over the WebKitGTK remote inspector (`tests/gui/inspector.py`, stdlib
only), "open with" through a real second instance, and native dialogs answered by
`PRIVAVAULT_E2E_OUTPUT_DIR` — a seam compiled into debug builds only (`#[cfg(debug_assertions)]`).
Screenshots are taken from the page itself, never from the desktop. Covered: first run, password rules,
encrypt, forwarding, wrong password, listing, filter, single/all/ZIP extraction, no-overwrite, lock,
1.x and malicious vaults, cancel with cleanup, theme and language switching.

Not covered automatically: the native dialogs themselves, drag & drop, notifications, Windows.

## Manual GUI checklist

Before a release, check what automation can't, on **KDE Wayland** and **Windows 11**:

1. First launch shows the welcome dialog once; theme follows the system; switch EN/TR in Settings.
2. Lock files: add files + a folder (picker and drag & drop), remove one, weak/mismatched passwords are
   refused, save dialog opens in the source folder, progress + cancel work, *Show in folder* opens the
   file manager.
3. Open the vault: wrong password message; correct password lists entries instantly; filter; *Save…* one
   file; *Extract all…* creates a new folder (second time: `name (2)`); *Save as ZIP…*.
4. Open `crates/privavault-core/tests/fixtures/v1_multi.pvault` (password `correct horse battery staple`):
   legacy note shown, extraction works.
5. Open `v1_evil.pvault` (password `evil`): `../evil.txt` and `/abs.txt` shown struck-through, not extracted;
   the `<img …>` name is shown as plain text.
6. Auto-lock (set to 2 min) locks the vault; *Lock* in the sidebar works.
7. Close the window during a long encryption → confirmation → quitting leaves no `.pvpart` file.
8. Double-click a `.pvault` in Dolphin/Explorer with the app running → it opens in the existing window.
9. Network: with the update check off, no traffic (e.g. `ss -tp | grep privavault` stays empty).
