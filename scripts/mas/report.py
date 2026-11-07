import collections, os, re, sys

res, raw, out = sys.argv[1], sys.argv[2], sys.argv[3]
home = os.path.expanduser("~")

rows = []
if os.path.exists(res):
    for line in open(res, errors="replace"):
        p = line.rstrip("\n").split("\t", 2)
        if len(p) == 3:
            rows.append(p)

feature_rules = [
    ("pty job control", re.compile(r"ttys|/dev/tty|/dev/ptmx|pty|ioctl|tiocsctty|process-setsid|process-setpgid", re.I)),
    ("launchd / autostart", re.compile(r"LaunchAgents|launchd|com\.apple\.xpc|launchctl|mach-lookup.*(launch|xpc|smd|servicemanagement)", re.I)),
    ("agent hooks (~/.claude ~/.codex ~/.gemini)", re.compile(r"\.claude|\.codex|\.gemini|\.config/", re.I)),
    ("keychain / security", re.compile(r"securityd|keychain|Security|trustd|com\.apple\.SecurityServer|ocspd", re.I)),
    ("tmux", re.compile(r"tmux", re.I)),
    ("ps / lsof / process info", re.compile(r"process-info|sysctl|proc_|lsof|/bin/ps|kernel-|forbidden-exec-sugid", re.I)),
    ("network", re.compile(r"network-|socket|nsurlsessiond|dnssd|mDNSResponder|cfprefsd.*network", re.I)),
    ("exec locations", re.compile(r"process-exec|/opt/homebrew|/usr/local|\.local/bin|/bin/|/usr/bin|/usr/sbin", re.I)),
    ("shell env (login shell)", re.compile(r"\.zsh|\.zprofile|\.zlogin|\.zshrc|\.zshenv|\.profile|\.bash|/etc/z|/etc/profile|path_helper", re.I)),
    ("state dir / files", re.compile(r"Application Support|Containers|file-(read|write)|ovs-|sbx-grant|/private/tmp|/tmp", re.I)),
]


def feature(proc, op, path):
    s = f"{op} {path}"
    for name, rx in feature_rules:
        if rx.search(s):
            return name
    return "other"


OURS = {"overseer-app", "overseer", "probe", "probe-helper", "zsh", "sh", "bash", "git", "xcrun", "tmux", "node", "ps", "lsof", "security", "launchctl", "env", "sleep", "cat", "tty", "agent", "ovs-script.sh", "ovs-dummy", "stty", "ls", "claude", "codex", "gh", "login", "grant"}
groups = collections.defaultdict(lambda: collections.Counter())
rx = re.compile(r"Sandbox: (\S+?)\((\d+)\) deny\(\d+\) (\S+)(?: (.*))?$")
total = 0
if os.path.exists(raw):
    for line in open(raw, errors="replace"):
        m = rx.search(line)
        if not m:
            continue
        proc, _, op, path = m.groups()
        if proc not in OURS and not proc.startswith(("com.apple.WebKit", "overseer")):
            continue
        path = (path or "").strip().replace(home, "~")
        path = re.sub(r"\.sb-[0-9a-f]+-\w+", ".sb-X", path)
        path = re.sub(r"/ttys\d+", "/ttysN", path)
        path = re.sub(r"\b\d{3,}\b", "N", path)
        total += 1
        groups[feature(proc, op, path)][(proc, op, path)] += 1

lines = ["## macOS App Sandbox smoke", "", "| Check | Result | Detail |", "|---|---|---|"]
for name, verdict, detail in rows:
    d = detail.replace("|", "\\|")[: (700 if name.startswith(("pipe_", "exec_claude", "exec_codex")) else 260)]
    lines.append(f"| {name} | {verdict} | {d} |")
passed = sum(1 for r in rows if r[1] == "PASS")
failed = sum(1 for r in rows if r[1] == "FAIL")
lines += ["", f"PASS {passed}, FAIL {failed}, INFO {len(rows) - passed - failed}", ""]
lines.append(f"## Sandbox denials ({total} log lines, deduplicated)")
lines.append("")
if not groups:
    lines.append("none found in the log window")
for feat in sorted(groups, key=lambda k: -sum(groups[k].values())):
    lines.append(f"### {feat}")
    lines.append("")
    for (proc, op, path), n in groups[feat].most_common(40):
        lines.append(f"- `{proc}` {op} `{path}` x{n}")
    lines.append("")
open(out, "a").write("\n".join(lines) + "\n")
print("\n".join(lines))
