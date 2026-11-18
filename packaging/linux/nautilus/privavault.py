# Nautilus (GNOME Files) right-click menu for PrivaVault: "Lock with PrivaVault" on files and
# folders, "Open with PrivaVault" on vaults. Needs nautilus-python; installed to
# /usr/share/nautilus-python/extensions/. It only starts `privavault <paths>`, which opens the
# app (or the running window) with those paths, the same as a file manager's "Open with".
import locale
import re
import subprocess

from gi.repository import GObject, Nautilus

TURKISH = (locale.getlocale()[0] or "").startswith("tr")
LOCK = "PrivaVault ile kilitle" if TURKISH else "Lock with PrivaVault"
OPEN = "PrivaVault ile aç" if TURKISH else "Open with PrivaVault"
VAULT = re.compile(r"\.pvault(\.\d{3})?$", re.IGNORECASE)  # a vault or one part of a split vault


def _launch(_item, paths):
    subprocess.Popen(["privavault", *paths], start_new_session=True,
                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


class PrivaVaultMenu(GObject.GObject, Nautilus.MenuProvider):
    # Nautilus 43+ passes (files); older versions pass (window, files).
    def get_file_items(self, *args):
        files = args[-1]
        paths = [f.get_location().get_path() for f in files]
        if not paths or None in paths:  # remote or virtual locations
            return []
        vaults = [p for p in paths if VAULT.search(p)]
        if vaults and len(vaults) == len(paths):
            item = Nautilus.MenuItem(name="PrivaVault::open", label=OPEN, icon="privavault")
            item.connect("activate", _launch, vaults[:1])
        else:
            item = Nautilus.MenuItem(name="PrivaVault::lock", label=LOCK, icon="privavault")
            item.connect("activate", _launch, [p for p in paths if p not in vaults])
        return [item]
