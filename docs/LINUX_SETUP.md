# PrivaVault on Linux

Primary targets: **Arch Linux / CachyOS** (KDE Plasma, Wayland). Also built for Debian/Ubuntu (.deb),
Fedora/openSUSE (.rpm) and any distro via AppImage.

## Install

### Arch Linux / CachyOS (recommended: native package)
Download `privavault-<version>-1-x86_64.pkg.tar.zst` from
[Releases](https://github.com/TNFX1/PrivaVault/releases) and install it:
```bash
sudo pacman -U ~/Downloads/privavault-*-x86_64.pkg.tar.zst
```
pacman installs the needed libraries (`webkit2gtk-4.1`, `gtk3`, …) from the official repos.
Uninstall with `sudo pacman -R privavault`. Updates: install the newer file the same way.

To build the package yourself instead (needs `cargo`, takes a few minutes):
```bash
git clone https://github.com/TNFX1/PrivaVault.git
cd PrivaVault/packaging/arch && PRIVAVAULT_LOCAL_SRC=../.. makepkg -si
```
For native KDE file dialogs install the portal (usually already present on Plasma):
`sudo pacman -S --needed xdg-desktop-portal-kde`.

### Debian / Ubuntu (22.04+)
```bash
sudo apt install ./PrivaVault_2.0.0_amd64.deb      # package name: priva-vault (set by the Tauri bundler)
```

### Fedora / openSUSE
```bash
sudo dnf install ./PrivaVault-2.0.0-1.x86_64.rpm
```

### AppImage (any distro)
```bash
chmod +x PrivaVault_2.0.0_amd64.AppImage && ./PrivaVault_2.0.0_amd64.AppImage
```
The AppImage does not register the `.pvault` file type or a menu entry by itself.

## Where things are

| What | Path |
| --- | --- |
| Settings | `$XDG_CONFIG_HOME/privavault/settings.json` (default `~/.config/privavault/`), mode 0600 |
| Web engine data | `~/.local/share/com.privavault.app/` — a few KB created by WebKitGTK itself (engine cache/storage). Holds no vault contents, file names or passwords. |
| Logs, history, recent files | none are written |
| Temp files | none; in-progress outputs are hidden `.<name>.<random>.pvpart` files next to the destination, removed on failure |
| Binary (packages) | `/usr/bin/privavault` |
| Desktop entry | `/usr/share/applications/com.privavault.app.desktop` (named after the Wayland app_id) |
| MIME type | `/usr/share/mime/packages/privavault.xml` (`application/x-privavault`; detected by the `PRIVAVLT` magic or `*.pvault`) |

To reset settings: `rm ~/.config/privavault/settings.json`.

## Uninstall

Uninstalling removes the program only. **Your vaults (`.pvault` files) and anything you extracted are
never touched** — they are ordinary files in your folders. Keep the passwords: a vault can be opened
again after reinstalling, on any computer, but not without its password.

### 1. Remove the program

| Installed with | Remove with |
| --- | --- |
| Arch / CachyOS package (`pacman -U` or `makepkg -si`) | `sudo pacman -R privavault` |
| Debian / Ubuntu `.deb` | `sudo apt remove priva-vault` |
| Fedora / openSUSE `.rpm` | `sudo dnf remove priva-vault` (openSUSE: `sudo zypper remove priva-vault`) |
| AppImage | delete the `.AppImage` file; if you added it to your menu with a tool such as AppImageLauncher or Gear Lever, remove it there |

The package manager also removes the menu entry, icons and the `.pvault` file type. Libraries that were
installed as dependencies (WebKitGTK…) stay, because other apps may use them; on Arch you can remove
unused ones with `sudo pacman -Rns privavault` instead.

### 2. Remove your settings (optional)

These stay in your home folder so a reinstall keeps your preferences. To remove them too:
```bash
rm -rf ~/.config/privavault ~/.local/share/com.privavault.app
```

### 3. Check nothing is left (optional)

```bash
command -v privavault                       # prints nothing when the program is gone
ls ~/.config/privavault ~/.local/share/com.privavault.app 2>/dev/null   # prints nothing when settings are gone
```

If KDE still shows an old menu entry for a moment, it disappears after the next login (or run
`kbuildsycoca6`).

## Desktop integration

* **Wayland-native.** The window's Wayland `app_id` is `com.privavault.app`; the desktop file sets
  `StartupWMClass=com.privavault.app`, so Plasma shows the right icon in the task manager.
* **Dialogs** go through the XDG Desktop Portal (Dolphin-style dialogs on KDE).
* **Open with:** double-clicking a `.pvault` file in Dolphin opens it in PrivaVault. If the app is already
  running, the file is handed to the existing window (single instance).
* **Notifications** use the freedesktop notification service (Plasma notifications), only when an
  operation finishes while the window is in the background, and only if enabled in Settings.
* **Closing** the window quits the app (no hidden tray process). If an operation is running you are asked
  first; quitting cancels it and removes the unfinished file.

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| Blank/white window or `Error 71 (Protocol error)` on NVIDIA + Wayland | PrivaVault sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` automatically when the NVIDIA driver is loaded under Wayland. On other GPUs try it manually: `WEBKIT_DISABLE_DMABUF_RENDERER=1 privavault` |
| Plain GTK dialogs instead of KDE ones | Install `xdg-desktop-portal-kde` and log out/in |
| `.pvault` files not recognised by Dolphin after install | `update-mime-database ~/.local/share/mime` is not needed for packages; run `sudo update-mime-database /usr/share/mime` if your package manager didn't |
| Fonts look wrong | The UI bundles IBM Plex Sans; nothing to install |
| `Failed to load module "appmenu-gtk-module"` in the terminal | Harmless GTK message |

## Security notes specific to Linux

* Files PrivaVault creates are `0600` (owner read/write only); extraction folders `0700`.
* Btrfs (CachyOS default) and SSDs keep old blocks: deleting an original file after locking it does not
  securely erase it. Use full-disk encryption (LUKS) for that.
* CachyOS uses zram for swap by default, so memory is not paged to an unencrypted disk.
