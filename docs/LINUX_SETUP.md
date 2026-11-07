# PrivaVault on Linux

Primary targets: **Arch Linux / CachyOS** (KDE Plasma, Wayland). Also built for Debian/Ubuntu (.deb),
Fedora/openSUSE (.rpm) and any distro via AppImage.

## Install

### Arch Linux / CachyOS (recommended: native package)
```bash
git clone https://github.com/TNFX1/PrivaVault.git
cd PrivaVault/packaging/arch
PRIVAVAULT_LOCAL_SRC=../.. makepkg -si      # builds from this checkout and installs
```
`makepkg` pulls `webkit2gtk-4.1`, `gtk3` etc. from the official repos and needs `cargo` to build.
For native KDE file dialogs install the portal (usually already present on Plasma):
`sudo pacman -S --needed xdg-desktop-portal-kde`.

### Debian / Ubuntu (22.04+)
```bash
sudo apt install ./PrivaVault_2.0.0_amd64.deb
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
| Logs, caches, history | none are written |
| Temp files | none; in-progress outputs are hidden `.<name>.<random>.pvpart` files next to the destination, removed on failure |
| Binary (packages) | `/usr/bin/privavault` |
| Desktop entry | `/usr/share/applications/PrivaVault.desktop` |
| MIME type | `/usr/share/mime/packages/privavault.xml` (`application/x-privavault`; detected by the `PRIVAVLT` magic or `*.pvault`) |

To reset settings: `rm ~/.config/privavault/settings.json`.

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
