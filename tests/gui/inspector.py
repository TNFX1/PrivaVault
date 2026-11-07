"""Minimal client for the WebKitGTK remote inspector (stdlib only).

Debug builds of PrivaVault have the inspector enabled. Starting the app with
`WEBKIT_INSPECTOR_HTTP_SERVER=127.0.0.1:<port>` exposes the page over a WebSocket that speaks the
WebKit Inspector Protocol; `Runtime.evaluate` lets the GUI test act on the real DOM exactly like a
user's input events would (WebDriver is not available on Arch's WebKitGTK package).
"""
import base64
import json
import os
import re
import socket
import struct
import time
import urllib.request


class Inspector:
    def __init__(self, port, timeout=30):
        end = time.time() + timeout
        while True:
            try:
                html = urllib.request.urlopen(f"http://127.0.0.1:{port}/", timeout=2).read().decode()
                m = re.search(r"(/socket/\d+/\d+/WebPage)", html)
                if m:
                    break
            except OSError:
                pass
            if time.time() > end:
                raise TimeoutError("inspector not reachable")
            time.sleep(0.3)
        self.sock = socket.create_connection(("127.0.0.1", port), timeout=timeout)
        key = base64.b64encode(os.urandom(16)).decode()
        self.sock.sendall(
            (f"GET {m.group(1)} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\n"
             f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n").encode())
        resp = b""
        while b"\r\n\r\n" not in resp:
            resp += self.sock.recv(4096)
        if b" 101 " not in resp.split(b"\r\n")[0]:
            raise ConnectionError(resp[:200])
        self.buf = resp.split(b"\r\n\r\n", 1)[1]
        self.next_id = 0
        self.target = None

    # -- framing ---------------------------------------------------------------------------------
    def _send(self, text):
        data = text.encode()
        mask = os.urandom(4)
        n = len(data)
        head = bytes([0x81])
        if n < 126:
            head += bytes([0x80 | n])
        elif n < 65536:
            head += bytes([0x80 | 126]) + struct.pack(">H", n)
        else:
            head += bytes([0x80 | 127]) + struct.pack(">Q", n)
        self.sock.sendall(head + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

    def _read(self, n):
        while len(self.buf) < n:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise ConnectionError("inspector closed")
            self.buf += chunk
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def _recv(self):
        msg = b""
        while True:
            b0, b1 = self._read(2)
            n = b1 & 0x7F
            if n == 126:
                n = struct.unpack(">H", self._read(2))[0]
            elif n == 127:
                n = struct.unpack(">Q", self._read(8))[0]
            payload = self._read(n)
            if b0 & 0x0F in (0x1, 0x0):
                msg += payload
                if b0 & 0x80:
                    return json.loads(msg)

    # -- protocol --------------------------------------------------------------------------------
    def _call_raw(self, method, params=None):
        self.next_id += 1
        mid = self.next_id
        self._send(json.dumps({"id": mid, "method": method, "params": params or {}}))
        while True:
            m = self._recv()
            if m.get("method") == "Target.targetCreated":
                self.target = m["params"]["targetInfo"]["targetId"]
            if m.get("id") == mid:
                return m
            if m.get("method") == "Target.dispatchMessageFromTarget":
                inner = json.loads(m["params"]["message"])
                if inner.get("id") == mid:
                    return inner

    def call(self, method, params=None):
        """Send a command, routed through the page target when WebKit uses target multiplexing."""
        if self.target is None:
            # Give WebKit a moment to announce its page target.
            self.sock.settimeout(1.0)
            try:
                while self.target is None:
                    m = self._recv()
                    if m.get("method") == "Target.targetCreated":
                        self.target = m["params"]["targetInfo"]["targetId"]
            except (socket.timeout, TimeoutError):
                pass
            finally:
                self.sock.settimeout(30)
        if self.target is None:
            return self._call_raw(method, params)
        self.next_id += 1
        mid = self.next_id
        inner = json.dumps({"id": mid, "method": method, "params": params or {}})
        self.next_id += 1
        self._send(json.dumps({"id": self.next_id, "method": "Target.sendMessageToTarget",
                               "params": {"targetId": self.target, "message": inner}}))
        while True:
            m = self._recv()
            if m.get("method") == "Target.dispatchMessageFromTarget":
                r = json.loads(m["params"]["message"])
                if r.get("id") == mid:
                    return r

    def js(self, expression):
        """Evaluate JS in the page; awaits promises; returns the JSON value."""
        # WebKit has no `awaitPromise` flag on evaluate; it has a separate Runtime.awaitPromise.
        r = self.call("Runtime.evaluate", {
            "expression": f"(async () => JSON.stringify(await (async () => {{ {expression} }})()))()"})
        res = r.get("result", {})
        if res.get("wasThrown") or "error" in r:
            raise RuntimeError(json.dumps(r)[:500])
        pid = res["result"]["objectId"]
        r = self.call("Runtime.awaitPromise", {"promiseObjectId": pid, "returnByValue": True})
        res = r.get("result", {})
        if res.get("wasThrown") or "error" in r:
            raise RuntimeError(json.dumps(r)[:500])
        v = res.get("result", {}).get("value")
        return json.loads(v) if isinstance(v, str) else v

    def wait(self, expression, timeout=20, what=None):
        end = time.time() + timeout
        while time.time() < end:
            try:
                v = self.js(f"return ({expression});")
            except RuntimeError:  # page (re)loading: the evaluation context went away
                v = None
            if v:
                return v
            time.sleep(0.25)
        raise TimeoutError(f"timed out: {what or expression}")
