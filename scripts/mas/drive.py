import base64, json, os, socket, struct, subprocess, sys, time, urllib.parse

SOCK, BASE, OUT = sys.argv[1], sys.argv[2], sys.argv[3]
REPO = f"{BASE}/proj"
OUTSIDE = sys.argv[4] if len(sys.argv) > 4 else "/tmp/sbx-grant"
rows = []
PREFIX = os.environ.get("PREFIX", "")


def rec(name, verdict, detail=""):
    detail = " ".join(str(detail).split())[:500]
    rows.append((name, verdict, detail))
    print(f"RESULT\t{PREFIX}{name}\t{verdict}\t{detail}", flush=True)
    with open(OUT, "a") as f:
        f.write(f"{PREFIX}{name}\t{verdict}\t{detail}\n")


def conn():
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(30)
    s.connect(SOCK)
    return s


def req(method, path, body=None):
    s = conn()
    b = json.dumps(body).encode() if body is not None else b""
    s.sendall(
        f"{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {len(b)}\r\n\r\n".encode() + b
    )
    data = b""
    while (c := s.recv(65536)):
        data += c
    head, _, payload = data.partition(b"\r\n\r\n")
    code = int(head.split()[1])
    try:
        return code, json.loads(payload or b"null")
    except Exception:
        return code, payload.decode(errors="replace")


class WS:
    def __init__(self, path):
        self.s = conn()
        key = base64.b64encode(os.urandom(16)).decode()
        self.s.sendall(
            f"GET {path} HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n".encode()
        )
        buf = b""
        while b"\r\n\r\n" not in buf:
            c = self.s.recv(4096)
            if not c:
                raise RuntimeError("ws closed in handshake: " + buf.decode(errors="replace"))
            buf += c
        head, _, self.buf = buf.partition(b"\r\n\r\n")
        if b" 101 " not in head.split(b"\r\n")[0] + b" ":
            raise RuntimeError("ws handshake: " + head.decode(errors="replace")[:200])
        self.s.settimeout(0.3)

    def send(self, text):
        data = text.encode()
        mask = os.urandom(4)
        n = len(data)
        hdr = bytes([0x82])
        hdr += bytes([0x80 | n]) if n < 126 else bytes([0x80 | 126]) + struct.pack(">H", n)
        self.s.sendall(hdr + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

    def _need(self, n, end):
        while len(self.buf) < n:
            if time.time() > end:
                return False
            try:
                c = self.s.recv(65536)
            except socket.timeout:
                continue
            if not c:
                return False
            self.buf += c
        return True

    def read_until(self, needle, secs=20):
        end = time.time() + secs
        got = b""
        while time.time() < end:
            if needle.encode() in got:
                return got.decode(errors="replace")
            if not self._need(2, end):
                break
            op, ln = self.buf[0] & 0x0F, self.buf[1] & 0x7F
            off = 2
            if ln == 126:
                if not self._need(4, end):
                    break
                ln = struct.unpack(">H", self.buf[2:4])[0]
                off = 4
            elif ln == 127:
                if not self._need(10, end):
                    break
                ln = struct.unpack(">Q", self.buf[2:10])[0]
                off = 10
            if not self._need(off + ln, end):
                break
            payload, self.buf = self.buf[off:off + ln], self.buf[off + ln:]
            if op in (1, 2):
                got += payload
            elif op == 8:
                break
        return got.decode(errors="replace")


def term(cmd, cwd=REPO):
    code, body = req("POST", "/v1/terms", {"cwd": cwd, "cmd": cmd, "cols": 100, "rows": 30})
    return code, body


def kill(tid):
    req("POST", f"/v1/terms/{tid}/kill", {"confirm": True})
    req("DELETE", f"/v1/terms/{tid}")


def guarded(name, fn):
    try:
        fn()
    except Exception as e:
        rec(name, "FAIL", f"{type(e).__name__}: {e}")


def t_version():
    code, body = req("GET", "/v1/version")
    rec("daemon_version_over_socket", "PASS" if code == 200 else "FAIL", f"{code} {body}")


def t_status():
    code, body = req("GET", "/v1/status")
    rec("daemon_status", "PASS" if code == 200 else "FAIL", f"{code} {str(body)[:200]}")


def t_stub():
    c = f"{BASE}"
    code, body = term(["/bin/sh", "-c", f"SB_STUB_LOG={c}/stub.log exec {c}/stubs/agent"])
    if code != 201 and code != 200:
        rec("pty_stub_agent_session", "FAIL", f"POST /v1/terms {code} {body}")
        return
    tid = body["id"]
    ws = WS(f"/v1/terms/{tid}/io")
    a = ws.read_until("agent ready")
    ws.send("hello-from-runner\r")
    b = ws.read_until("got: hello-from-runner")
    ok = "agent ready" in a and "got: hello-from-runner" in b
    rec("pty_stub_agent_session", "PASS" if ok else "FAIL", f"banner={'agent ready' in a} echo={'got: hello-from-runner' in b} tail={(a + b)[-200:]!r}")
    kill(tid)


def t_zsh():
    code, body = term(["/bin/zsh", "-f", "-i"])
    if code not in (200, 201):
        rec("pty_zsh_job_control", "FAIL", f"{code} {body}")
        return
    tid = body["id"]
    ws = WS(f"/v1/terms/{tid}/io")
    ws.read_until("%", 5)
    ws.send("echo JC$((6*7)); tty; sleep 30 & jobs; kill %1; wait; echo DONE$((1+1))\r")
    out = ws.read_until("DONE2", 20)
    bad = any(x in out for x in ("tty pgrp", "no job control", "Operation not permitted"))
    ok = "JC42" in out and "DONE2" in out and "[1]" in out and not bad
    rec("pty_zsh_job_control", "PASS" if ok else "FAIL", f"job_control_error={bad} tail={out[-250:]!r}")
    kill(tid)


def t_git_term():
    code, body = term(["/bin/sh"])
    if code not in (200, 201):
        rec("git_in_pty_inside_container", "FAIL", f"{code} {body}")
        return
    tid = body["id"]
    ws = WS(f"/v1/terms/{tid}/io")
    ws.send(f"git -C {REPO} status --short; echo GITRC$?\r")
    out = ws.read_until("GITRC0", 20)
    rec("git_in_pty_inside_container", "PASS" if "GITRC0" in out else "FAIL", out[-250:])
    ws.send(f"git -C {REPO} worktree add {BASE}/wt-term -b wt-term; echo WTRC$?\r")
    out = ws.read_until("WTRC0", 30)
    rec("git_worktree_add_in_pty_inside_container", "PASS" if "WTRC0" in out else "FAIL", out[-250:])
    ws.send(f"echo TOOLS; command -v git node tmux claude codex; echo PATH=$PATH; echo TOOLSEND\r")
    out = ws.read_until("TOOLSEND", 15)
    rec("info_pty_shell_path_and_tools", "INFO", out[-300:])
    kill(tid)


def t_worktree():
    code, body = req("POST", "/v1/worktrees", {"project": REPO, "slug": "smoke"})
    path = body.get("path") if isinstance(body, dict) else None
    ok = code == 201 and path and os.path.isdir(path)
    rec("daemon_worktree_create_endpoint", "PASS" if ok else "FAIL", f"{code} {str(body)[:300]}")


def t_project_list():
    code, body = req("GET", "/v1/projects/" + urllib.parse.quote(REPO, safe="") + "/worktrees")
    rec("daemon_lists_worktrees_of_container_repo", "PASS" if code == 200 else "FAIL", f"{code} {str(body)[:200]}")


def t_outside():
    code, body = term(["/bin/sh", "-c", "echo OUTSIDE-OK; ls"], cwd=OUTSIDE)
    if code in (200, 201):
        ws = WS(f"/v1/terms/{body['id']}/io")
        out = ws.read_until("OUTSIDE-OK", 8)
        rec("outside_folder_term_without_grant", "PASS" if "OUTSIDE-OK" not in out else "FAIL", f"want denied; term created {code}, output={out[-150:]!r}")
        kill(body["id"])
    else:
        rec("outside_folder_term_without_grant", "PASS", f"want denied; got {code} {str(body)[:150]}")
    code, body = req("GET", "/v1/projects/" + urllib.parse.quote(OUTSIDE, safe="") + "/worktrees")
    rec("outside_folder_daemon_git_without_grant", "PASS" if code != 200 else "FAIL", f"want denied; got {code} {str(body)[:150]}")


def t_files():
    seen = []
    for root, dirs, files in os.walk(BASE.rsplit("/Data", 1)[0] + "/Data"):
        depth = root.count("/") - BASE.count("/")
        if depth > 5:
            dirs[:] = []
            continue
        for f in files:
            seen.append(os.path.join(root, f))
    rec("info_files_written_in_container", "INFO", f"{len(seen)} files, e.g. " + ", ".join(p.replace(BASE, '') for p in seen[:25]))


for name, fn in [
    ("daemon_version_over_socket", t_version),
    ("daemon_status", t_status),
    ("pty_stub_agent_session", t_stub),
    ("pty_zsh_job_control", t_zsh),
    ("git_in_pty_inside_container", t_git_term),
    ("daemon_worktree_create_endpoint", t_worktree),
    ("daemon_lists_worktrees_of_container_repo", t_project_list),
    ("outside_folder_term_without_grant", t_outside),
    ("info_files_written_in_container", t_files),
]:
    guarded(name, fn)
