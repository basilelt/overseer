# Toolkit patch: keep held stubs across reboots

Not applied. For the user to review and apply to `~/.local/bin/tmux-snapshot` and
`~/.local/bin/tmux-restore` (diff generated on 2026-10-28 against copies of the VM files).

Without it a held line stays held after a reboot (the held pane has no Claude process, so
`tmux-snapshot` records it as a plain shell and `tmux-restore` does not relaunch Claude), but the
pane comes back as a shell instead of the "held — press Enter to resume" stub. `overseer doctor`
reports whether the patch is present.

With it, `tmux-snapshot` records `@sb_held`, and `tmux-restore` starts the stub in that pane and
sets `@sb_held` again. Replace `overseer` with the absolute path of `bin/overseer` if it is
not on the PATH of the systemd unit.

```diff
--- a/tmux-snapshot
+++ b/tmux-snapshot
@@ -17,12 +17,12 @@
 SEP = "|:|"  # printable: tmux mangles control chars in -F output
 FIELDS = ["session", "window", "window_name", "layout", "window_active", "pane", "pane_active",
           "cwd", "pane_pid", "command", "claude_session", "session_created", "cmux_slot", "cmux_created",
-          "cmux_title", "cmux_title_tab"]
+          "cmux_title", "cmux_title_tab", "sb_held"]
 FMT = SEP.join(["#{session_name}", "#{window_index}", "#{window_name}", "#{window_layout}",
                 "#{window_active}", "#{pane_index}", "#{pane_active}", "#{pane_current_path}",
                 "#{pane_pid}", "#{pane_current_command}", "#{@claude_session}",
                 "#{session_created}", "#{@cmux_slot}", "#{@cmux_created}",
-                "#{@cmux_title}", "#{@cmux_title_tab}"])  # tab name, kept by cmux-tab-titles-sync (Mac)
+                "#{@cmux_title}", "#{@cmux_title_tab}", "#{@sb_held}"])  # tab name, kept by cmux-tab-titles-sync (Mac)
 
 
 def tmux(*args):
--- a/tmux-restore
+++ b/tmux-restore
@@ -25,6 +25,9 @@
 
 
 def pane_cmd(p, snap_dir, saved_at):
+    if p.get("sb_held"):
+        stub = f"trap : INT; overseer stub {shlex.quote(p['sb_held'])}; exec zsh -l"
+        return f"zsh -lic {shlex.quote(stub)}"
     if p.get("claude_args") is not None and p.get("claude_session"):
         claude = " ".join(shlex.quote(a) for a in [CLAUDE, *p["claude_args"], "--resume", p["claude_session"]])
         inner = f"{claude}; exec zsh -l"
@@ -68,6 +71,8 @@
             for p in wpanes:
                 if p.get("claude_session"):
                     tmux("set", "-p", "-t", f"={name}:{widx}.{p['pane']}", "@claude_session", p["claude_session"])
+                if p.get("sb_held"):
+                    tmux("set", "-p", "-t", f"={name}:{widx}.{p['pane']}", "@sb_held", p["sb_held"])
                 if p["pane_active"] == "1":
                     tmux("select-pane", "-t", f"={name}:{widx}.{p['pane']}")
             if w0["window_active"] == "1":
```

Apply, after a backup:

```
cp ~/.local/bin/tmux-snapshot ~/.local/bin/tmux-snapshot.bak-$(date +%Y%m%d%H%M)
cp ~/.local/bin/tmux-restore ~/.local/bin/tmux-restore.bak-$(date +%Y%m%d%H%M)
cd ~/.local/bin && patch -p1 < /path/to/the-diff-above.patch
```
