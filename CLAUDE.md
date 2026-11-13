# PrivaVault

Offline file encryption for Linux and Windows. Built with Tauri 2 and Rust (MIT license). Current release: 2.3.1.

## Layout

- `crates/privavault-core/`: crypto and file formats.
  - v2 vault format (default for new vaults): scrypt, then HKDF, then AES-256-GCM in STREAM mode with 64 KiB chunks. The header is authenticated and there is a key check.
  - v1 vaults (from the Electron 1.x app) can only be read; they are never written.
  - Tests include real 1.4.0 fixtures.
- `src-tauri/`: Tauri commands. Output paths come only from dialogs opened on the Rust side.
- `ui/`: static HTML, CSS and JS. Strict CSP; no `innerHTML`. English and Turkish.
- `packaging/`: desktop file (`StartupWMClass=com.privavault.app`), MIME type, metainfo, Arch PKGBUILD, AUR package, Flatpak manifest.
- `tools/release/`: `make_manifest.py` (updater `latest.json`), `aur.sh`, `flatpak.sh`
- `docs/`: architecture, security audit, building guide, changelog
- Logo: `assets/logo.svg` is the source. Regenerate the app icons with `.tools/bin/cargo-tauri icon <1024px png>`, then copy back only the files already tracked in `src-tauri/icons/`.

## Commands

- `cargo test --workspace`: Rust tests
- `node --test tests/ui/ui.test.js`: UI tests. Pass the file itself; a directory argument fails on Node 22, which CI uses.
- `python3 tests/gui/e2e.py`: end-to-end GUI checks over the WebKitGTK remote inspector
- `cargo build -p privavault && python3 tests/gui/screenshots.py`: retake the README screenshots. Files are named `<name>-<version>.png` and the script rewrites the README.
- The Tauri CLI and cargo-audit live in `.tools/bin/` (gitignored).
- AppImage: `cargo tauri build --bundles appimage --config ../packaging/linux/appimage.tauri.json`

## Releases

- Pushing a `v*` tag runs CI: tests, bundles (deb, rpm, AppImage, Arch package, Windows setup and portable), signing, `latest.json`, and the final publish step (`gh release create`). That step fails if a release with the same tag already exists.
- Release assets are installable files only: no `SHA256SUMS*.txt` and no `-debug` packages.
- Updater signing key backup: `~/.config/privavault-signing/`. Losing it means no more in-app updates.
- Check CI through the workflow badge or `releases.atom`. Unauthenticated REST API calls are limited to 60 per hour.
- The in-app update check is opt-in and off by default. It is hidden when running as a Flatpak.

## Conventions

- No Claude or AI attribution in commits or PRs (no `Co-Authored-By: Claude`, no "Generated with Claude Code").
- Commit only when asked, and push only when asked.
- Do not prepare Flathub submissions: Flathub's generative-AI policy forbids them.
- Take screenshots from the page through the inspector (`Page.snapshotRect`), never from the whole screen.

## Open items

- Native dialogs, drag and drop, and notifications are not covered by automated tests.
- The Windows build has never been run by hand (CI only), and the installer is unsigned.
- There has been no independent crypto review.
