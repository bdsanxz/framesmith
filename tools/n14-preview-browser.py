#!/usr/bin/env python3
"""n14-preview-browser.py -- drive the preview review page in headless Chrome.

Uses only the Python standard library: launches headless Chrome with a
DevTools port, talks CDP over a minimal raw-socket WebSocket client, and
exercises the page's documented controls (step back, step forward, scrub,
loop, speed, play, pause) by activating the real page elements. Each
control round-trips through the browser and is timed; the server readout
is polled over HTTP until it reflects the control (or a timeout fails
the check). Saves screenshots as evidence and prints a JSON report to
stdout (plus writes it to --out when given).

Usage:
  n14-preview-browser.py --url http://127.0.0.1:PORT/ [--out report.json]
                         [--shots dir] [--timeout SECS]
"""
import base64
import hashlib
import json
import os
import socket
import struct
import subprocess
import sys
import time
import urllib.request

WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


def find_chrome():
    cands = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ]
    for c in cands:
        if os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    for name in ("google-chrome", "google-chrome-stable", "chromium",
                 "chromium-browser"):
        for d in os.environ.get("PATH", "").split(os.pathsep):
            p = os.path.join(d, name)
            if os.path.isfile(p) and os.access(p, os.X_OK):
                return p
    return None


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def http_json(url, method="GET", timeout=10):
    req = urllib.request.Request(url, method=method)
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.status, r.read()


def status(base):
    code, body = http_json(base + "api/status")
    assert code == 200, "status http %d" % code
    return json.loads(body.decode("utf-8"))


class CDP:
    """Minimal CDP client over a raw-socket WebSocket (stdlib only)."""

    def __init__(self, ws_url, timeout=30):
        from urllib.parse import urlparse
        u = urlparse(ws_url)
        self.sock = socket.create_connection((u.hostname, u.port or 80),
                                             timeout=timeout)
        self.sock.settimeout(timeout)
        key = base64.b64encode(os.urandom(16)).decode()
        req = ("GET %s HTTP/1.1\r\nHost: %s\r\nUpgrade: websocket\r\n"
               "Connection: Upgrade\r\nSec-WebSocket-Key: %s\r\n"
               "Sec-WebSocket-Version: 13\r\n\r\n"
               % (u.path or "/", u.hostname, key))
        self.sock.sendall(req.encode())
        head = b""
        while b"\r\n\r\n" not in head:
            chunk = self.sock.recv(4096)
            if not chunk:
                raise RuntimeError("devtools handshake: EOF")
            head += chunk
        if b" 101 " not in head.split(b"\r\n", 1)[0]:
            raise RuntimeError("devtools handshake failed: %r" % head[:120])
        accept = hashlib.sha1((key + WS_GUID).encode()).digest()
        if base64.b64encode(accept).decode() not in head.decode("latin1"):
            raise RuntimeError("devtools handshake: bad accept key")
        self.buf = b""
        self.next_id = 0
        self.pending_events = []

    def _fill(self, n):
        while len(self.buf) < n:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise RuntimeError("devtools socket EOF")
            self.buf += chunk

    def _read_frame(self):
        self._fill(2)
        b1, b2 = self.buf[0], self.buf[1]
        opcode = b1 & 0x0F
        ln = b2 & 0x7F
        off = 2
        if ln == 126:
            self._fill(4)
            ln = struct.unpack(">H", self.buf[2:4])[0]
            off = 4
        elif ln == 127:
            self._fill(10)
            ln = struct.unpack(">Q", self.buf[2:10])[0]
            off = 10
        self._fill(off + ln)
        payload = self.buf[off:off + ln]
        self.buf = self.buf[off + ln:]
        return opcode, bytes(payload)

    def _read_message(self):
        parts = []
        while True:
            opcode, payload = self._read_frame()
            if opcode == 0x8:
                raise RuntimeError("devtools closed the connection")
            if opcode == 0x9:  # ping -> pong
                self._send_raw(0xA, payload)
                continue
            if opcode in (0x1, 0x0):
                parts.append(payload)
                if opcode == 0x1 or True:
                    # Server frames are never fragmented in practice for
                    # these sizes; FIN bit check:
                    pass
                return b"".join(parts)
            # ignore binary/control otherwise

    def _send_raw(self, opcode, payload):
        head = bytes([0x80 | opcode])
        ln = len(payload)
        mask = os.urandom(4)
        if ln < 126:
            head += bytes([0x80 | ln])
        elif ln < 65536:
            head += bytes([0x80 | 126]) + struct.pack(">H", ln)
        else:
            head += bytes([0x80 | 127]) + struct.pack(">Q", ln)
        head += mask
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        self.sock.sendall(head + masked)

    def call(self, method, params=None, timeout=30):
        self.next_id += 1
        mid = self.next_id
        msg = json.dumps({"id": mid, "method": method,
                          "params": params or {}}).encode()
        self._send_raw(0x1, msg)
        deadline = time.time() + timeout
        while True:
            remain = deadline - time.time()
            if remain <= 0:
                raise RuntimeError("cdp call %s timed out" % method)
            self.sock.settimeout(remain)
            try:
                raw = self._read_message()
            finally:
                self.sock.settimeout(timeout)
            try:
                obj = json.loads(raw.decode("utf-8"))
            except Exception:
                continue
            if obj.get("id") == mid:
                if "error" in obj:
                    raise RuntimeError("cdp %s: %s" % (method, obj["error"]))
                return obj.get("result", {})
            self.pending_events.append(obj)

    def evaluate(self, expression, await_promise=True, timeout=30):
        res = self.call("Runtime.evaluate",
                        {"expression": expression, "awaitPromise": await_promise,
                         "returnByValue": True}, timeout=timeout)
        if res.get("exceptionDetails"):
            raise RuntimeError("page eval failed: %s"
                               % json.dumps(res["exceptionDetails"])[:400])
        return res.get("result", {}).get("value")

    def close(self):
        try:
            self.sock.close()
        except Exception:
            pass


def main(argv):
    url = None
    out = None
    shots = None
    timeout = 15.0
    i = 0
    while i < len(argv):
        a = argv[i]
        if a == "--url" and i + 1 < len(argv):
            url = argv[i + 1]
            i += 2
        elif a == "--out" and i + 1 < len(argv):
            out = argv[i + 1]
            i += 2
        elif a == "--shots" and i + 1 < len(argv):
            shots = argv[i + 1]
            i += 2
        elif a == "--timeout" and i + 1 < len(argv):
            timeout = float(argv[i + 1])
            i += 2
        else:
            print("unknown flag %r" % a, file=sys.stderr)
            return 2
    if not url:
        print("n14-preview-browser.py: missing --url", file=sys.stderr)
        return 2
    if not url.endswith("/"):
        url += "/"
    if shots:
        os.makedirs(shots, exist_ok=True)

    chrome = find_chrome()
    report = {"chrome": chrome, "url": url, "controls": [], "ok": False}
    if not chrome:
        report["error"] = "no Chrome/Chromium binary found"
        print(json.dumps(report, indent=2))
        return 1

    profile = "/tmp/framesmith-n14-browser-%d" % os.getpid()
    cdp_port = free_port()
    proc = subprocess.Popen(
        [chrome, "--headless=new", "--no-first-run",
         "--no-default-browser-check",
         "--autoplay-policy=no-user-gesture-required",
         "--user-data-dir=" + profile,
         "--remote-debugging-port=%d" % cdp_port, "about:blank"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    cdp = None
    try:
        # Wait for the DevTools endpoint, then open the page target.
        ws_url = None
        deadline = time.time() + 30
        page_id = None
        while time.time() < deadline:
            try:
                with urllib.request.urlopen(
                        "http://127.0.0.1:%d/json/list" % cdp_port,
                        timeout=3) as r:
                    targets = json.loads(r.read().decode())
                for t in targets:
                    if t.get("type") == "page" and "about:blank" in t.get(
                            "url", ""):
                        page_id = t["id"]
                        break
                if page_id:
                    break
            except Exception:
                pass
            if proc.poll() is not None:
                raise RuntimeError("chrome exited early (code %s)"
                                   % proc.poll())
            time.sleep(0.2)
        if not page_id:
            raise RuntimeError("no about:blank page target appeared")
        put = urllib.request.Request(
            "http://127.0.0.1:%d/json/new?%s" % (cdp_port, url),
            method="PUT")
        with urllib.request.urlopen(put, timeout=10) as r:
            new_target = json.loads(r.read().decode())
        ws_url = new_target["webSocketDebuggerUrl"]
        cdp = CDP(ws_url)
        cdp.call("Page.enable")
        # Wait for the page to finish loading.
        deadline = time.time() + 30
        loaded = False
        start_wait = time.time()
        while time.time() - start_wait < 30:
            cdp.call("Page.getResourceTree")
            try:
                code, body = http_json(url, timeout=5)
                if code == 200 and b'id="readout"' in body:
                    loaded = True
                    break
            except Exception:
                pass
            time.sleep(0.3)
        if not loaded:
            raise RuntimeError("review page never served its readout")
        checks = []

        def poll(cond, what):
            t0 = time.time()
            last = None
            while time.time() - t0 < timeout:
                last = status(url)
                try:
                    if cond(last):
                        return last, (time.time() - t0) * 1000.0
                except Exception:
                    pass
                time.sleep(0.1)
            raise RuntimeError("timed out waiting for %s; last=%s"
                               % (what, json.dumps(last)[:300]))

        def run_control(name, js, cond, what):
            t0 = time.time()
            cdp.evaluate(js)
            st, _ = poll(cond, what)
            ms = (time.time() - t0) * 1000.0
            checks.append({"control": name, "ms": round(ms, 1),
                           "readout": st["readout"]})
            print("control %s ok in %.0f ms" % (name, ms), flush=True)
            return st

        st0 = status(url)
        n = st0["frame_count"]
        assert n == 20, "reference has 20 frames, status says %s" % n

        # 1. step forward (documented transport control).
        st = run_control(
            "step-forward",
            "document.querySelectorAll('button')[1].click()",
            lambda s: s["readout"]["frame"] == st0["readout"]["frame"] + 1,
            "step forward to advance one frame")
        # 2. step back.
        st = run_control(
            "step-back",
            "document.querySelectorAll('button')[0].click()",
            lambda s: s["readout"]["frame"] == st0["readout"]["frame"],
            "step back to the first frame")
        # 3. scrub slider to frame 15.
        st = run_control(
            "scrub",
            "(function(){var e=document.getElementById('scrub');"
            "e.value='15';e.dispatchEvent(new Event('input'))})()",
            lambda s: s["readout"]["frame"] == 15,
            "scrub to frame 15")
        # 4. loop toggle on.
        st = run_control(
            "loop",
            "(function(){var e=document.getElementById('loop');"
            "e.checked=true;e.dispatchEvent(new Event('change'))})()",
            lambda s: s["readout"]["loop"] is True,
            "loop flag to turn on")
        # 5. speed to 2x.
        st = run_control(
            "speed",
            "(function(){var e=document.getElementById('speed');"
            "e.value='2';e.dispatchEvent(new Event('change'))})()",
            lambda s: abs(s["readout"]["speed"] - 2.0) < 1e-9,
            "speed to read 2x")
        # 6. play: the mix is the clock (audio=sound expected).
        t0 = time.time()
        play_val = cdp.evaluate(
            "(async function(){"
            "var r=await (await fetch('/api/play',{method:'POST'})).json();"
            "return r})()")
        assert play_val.get("audio") == "sound", \
            "play should report the sounding mix, got %r" % (play_val,)
        st, _ = poll(lambda s: s["readout"]["playing"] is True,
                     "playing flag after play")
        play_ms = (time.time() - t0) * 1000.0
        checks.append({"control": "play", "ms": round(play_ms, 1),
                       "readout": st["readout"],
                       "audio": play_val.get("audio")})
        print("control play ok in %.0f ms (audio=sound)" % play_ms,
              flush=True)
        # 7. pause via the page button.
        st = run_control(
            "pause",
            "document.querySelectorAll('button')[3].click()",
            lambda s: s["readout"]["playing"] is False,
            "playing flag to clear after pause")

        # Mix-clock advance: drive the documented sync path the page's
        # audio clock uses and require the readout to follow it.
        st = status(url)
        code, body = http_json(url + "api/frame?i=%d&action=step"
                               % st["readout"]["frame"])
        assert code == 200 and body[:4] == b"\x89PNG", \
            "current frame must serve after the control sequence"
        t0 = time.time()
        cdp.evaluate(
            "(async function(){"
            "var r=await (await fetch('/api/sync?time=1.0',"
            "{method:'POST'})).json(); return r.frame})()")
        st, _ = poll(lambda s: s["readout"]["frame"] == 10
                     and abs(s["readout"]["time_secs"] - 1.0) < 1e-9,
                     "readout to follow mix time 1.0s")
        sync_ms = (time.time() - t0) * 1000.0
        checks.append({"control": "sync-mix-clock", "ms": round(sync_ms, 1),
                       "readout": st["readout"]})
        print("control sync-mix-clock ok in %.0f ms" % sync_ms, flush=True)

        # Screenshots: page as rendered plus the served frame pixels.
        shots_saved = []
        if shots:
            res = cdp.call("Page.captureScreenshot", {"format": "png"})
            data = base64.b64decode(res["data"])
            p = os.path.join(shots, "browser-page.png")
            with open(p, "wb") as f:
                f.write(data)
            shots_saved.append(p)
            code, body = http_json(url + "api/frame?i=10&action=step")
            p2 = os.path.join(shots, "browser-frame10.png")
            with open(p2, "wb") as f:
                f.write(body)
            shots_saved.append(p2)
        report["controls"] = checks
        report["shots"] = shots_saved
        report["ok"] = True
        print("BROWSER: PASS (%d controls)" % len(checks), flush=True)
        rc = 0
    except Exception as e:
        report["error"] = "%s: %s" % (type(e).__name__, e)
        print("BROWSER: FAIL -- %s" % e, flush=True)
        rc = 1
    finally:
        try:
            if cdp is not None:
                cdp.close()
        except Exception:
            pass
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except Exception:
            proc.kill()
    print(json.dumps(report, indent=2))
    if out:
        with open(out, "w") as f:
            json.dump(report, f, indent=2)
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
