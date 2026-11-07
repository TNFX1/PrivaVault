"""Tiny AT-SPI helper for driving the real PrivaVault window on a Linux desktop (no WebDriver needed).

WebKitGTK exposes the page through the accessibility bus, so the test finds controls by their
accessible role and name — the same information screen readers use.
"""
import os
import subprocess
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402


def wait(pred, timeout=15.0, step=0.2, what="condition"):
    end = time.time() + timeout
    while time.time() < end:
        try:
            v = pred()
        except gi.repository.GLib.Error:
            v = None
        if v:
            return v
        time.sleep(step)
    raise TimeoutError(f"timed out waiting for {what}")


def app_root(name="privavault"):
    d = Atspi.get_desktop(0)
    for i in range(d.get_child_count()):
        a = d.get_child_at_index(i)
        if a is not None and (a.get_name() or "") == name:
            return a
    return None


def walk(obj, depth=0, limit=4000):
    """Yield (obj, depth) depth-first."""
    stack = [(obj, 0)]
    seen = 0
    while stack and seen < limit:
        o, d = stack.pop()
        if o is None:
            continue
        seen += 1
        yield o, d
        try:
            n = o.get_child_count()
        except gi.repository.GLib.Error:
            continue
        for i in reversed(range(n)):
            stack.append((o.get_child_at_index(i), d + 1))


def showing(o):
    try:
        return o.get_state_set().contains(Atspi.StateType.SHOWING)
    except gi.repository.GLib.Error:
        return False


def find(root, role=None, name=None, contains=None, visible=True):
    for o, _ in walk(root):
        try:
            r, n = o.get_role_name(), o.get_name() or ""
        except gi.repository.GLib.Error:
            continue
        if role and r != role:
            continue
        if name is not None and n != name:
            continue
        if contains is not None and contains not in n:
            continue
        if visible and not showing(o):
            continue
        return o
    return None


def find_all(root, role=None, visible=True):
    out = []
    for o, _ in walk(root):
        try:
            if (role is None or o.get_role_name() == role) and (not visible or showing(o)):
                out.append(o)
        except gi.repository.GLib.Error:
            pass
    return out


def text_of(o):
    try:
        t = o.get_text_iface()
        if t:
            return Atspi.Text.get_text(t, 0, Atspi.Text.get_character_count(t))
    except Exception:
        pass
    return o.get_name() or ""


def all_text(root):
    return "\n".join(t for t in (text_of(o) for o, _ in walk(root)) if t)


def type_into(o, text):
    Atspi.EditableText.set_text_contents(o.get_editable_text_iface(), text)


def type_text(field, text, clear=True):
    """Focus a field and type like a user (WebKitGTK exposes no editable-text interface)."""
    Atspi.Component.grab_focus(field.get_component_iface())
    time.sleep(0.2)
    if clear:  # select all + delete
        Atspi.generate_keyboard_event(0xFFE3, None, Atspi.KeySynthType.PRESS)  # Control_L
        Atspi.generate_keyboard_event(ord("a"), None, Atspi.KeySynthType.SYM)
        Atspi.generate_keyboard_event(0xFFE3, None, Atspi.KeySynthType.RELEASE)
        Atspi.generate_keyboard_event(0xFFFF, None, Atspi.KeySynthType.SYM)  # Delete
    Atspi.generate_keyboard_event(0, text, Atspi.KeySynthType.STRING)
    time.sleep(0.3)


def field_len(field):
    t = field.get_text_iface()
    return Atspi.Text.get_character_count(t) if t else -1


def press(o):
    Atspi.Action.do_action(o.get_action_iface(), 0)


def screenshot(path):
    subprocess.run(["spectacle", "-b", "-n", "-a", "-o", path], check=False,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def launch(binary, args, config_home):
    # Keyboard synthesis (XTest) only reaches X11 clients, so run the app on XWayland for the test.
    env = dict(os.environ, XDG_CONFIG_HOME=config_home, GDK_BACKEND="x11")
    p = subprocess.Popen([binary, *args], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    root = wait(app_root, 20, what="PrivaVault on the accessibility bus")
    wait(lambda: find(root, role="document web"), 20, what="web content")
    return p, root
