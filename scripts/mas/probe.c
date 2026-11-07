#define _DARWIN_C_SOURCE
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <netdb.h>
#include <netinet/in.h>
#include <poll.h>
#include <signal.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>
#include <util.h>

extern char *sandbox_extension_issue_file(const char *ext_class, const char *path, uint32_t flags);
extern int64_t sandbox_extension_consume(const char *token);

#define CLASS_RW "com.apple.app-sandbox.read-write"

static void result(const char *name, const char *verdict, const char *fmt, ...) {
  char d[600];
  va_list ap;
  va_start(ap, fmt);
  vsnprintf(d, sizeof d, fmt, ap);
  va_end(ap);
  for (char *p = d; *p; p++)
    if (*p == '\n' || *p == '\t' || *p == '\r') *p = ' ';
  printf("RESULT\t%s\t%s\t%s\n", name, verdict, d);
  fflush(stdout);
}

static void check(const char *name, int want_ok, int got_ok, const char *detail) {
  result(name, want_ok == got_ok ? "PASS" : "FAIL", "want %s got %s %s", want_ok ? "allowed" : "denied",
         got_ok ? "allowed" : "denied", detail);
}

static int run(char *const argv[], int tmo, char *out, size_t n) {
  int p[2];
  if (pipe(p)) return -1;
  pid_t pid = fork();
  if (pid < 0) return -1;
  if (pid == 0) {
    dup2(p[1], 1);
    dup2(p[1], 2);
    close(p[0]);
    close(p[1]);
    int z = open("/dev/null", O_RDONLY);
    if (z >= 0) dup2(z, 0);
    execv(argv[0], argv);
    dprintf(2, "execv %s: %s", argv[0], strerror(errno));
    _exit(127);
  }
  close(p[1]);
  size_t len = 0;
  time_t end = time(NULL) + tmo;
  int status = 0;
  for (;;) {
    struct pollfd pf = {p[0], POLLIN, 0};
    int r = poll(&pf, 1, 200);
    if (r > 0) {
      char b[512];
      ssize_t k = read(p[0], b, sizeof b);
      if (k > 0) {
        if (out && len + k < n) memcpy(out + len, b, k), len += k;
      } else {
        break;
      }
    }
    if (time(NULL) > end) {
      kill(pid, SIGKILL);
      waitpid(pid, &status, 0);
      close(p[0]);
      if (out) out[len] = 0;
      return -2;
    }
  }
  close(p[0]);
  waitpid(pid, &status, 0);
  if (out) out[len] = 0;
  return WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
}

static int try_write(const char *path) {
  int fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
  if (fd < 0) return 0;
  int ok = write(fd, "x\n", 2) == 2;
  close(fd);
  return ok;
}

static int try_read(const char *path) {
  char b[16];
  int fd = open(path, O_RDONLY);
  if (fd < 0) return 0;
  int ok = read(fd, b, sizeof b) >= 0;
  close(fd);
  return ok;
}

static int file_has(const char *path) {
  struct stat st;
  return stat(path, &st) == 0;
}

static void spawn_check(const char *name, int want_ok, char *const argv[], const char *expect_file) {
  char out[300] = "";
  if (expect_file) unlink(expect_file);
  int rc = run(argv, 20, out, sizeof out - 1);
  int ok = rc == 0 && (!expect_file || file_has(expect_file));
  char d[400];
  snprintf(d, sizeof d, "rc=%d out=%.200s", rc, out);
  check(name, want_ok, ok, d);
}

static int cmd_issue(const char *path) {
  char *t = sandbox_extension_issue_file(CLASS_RW, path, 0);
  if (!t) {
    fprintf(stderr, "issue failed: %s\n", strerror(errno));
    return 1;
  }
  puts(t);
  return 0;
}

static void write_file(const char *path, const char *s) {
  FILE *f = fopen(path, "w");
  if (f) fputs(s, f), fclose(f);
}

static char *read_token(const char *path) {
  static char t[8192];
  FILE *f = fopen(path, "r");
  if (!f) return NULL;
  size_t n = fread(t, 1, sizeof t - 1, f);
  fclose(f);
  while (n && (t[n - 1] == '\n' || t[n - 1] == ' ')) n--;
  t[n] = 0;
  return t;
}

static int cmd_suite(const char *dir, const char *stub, const char *tokfile, const char *helper, const char *home_probe) {
  char p[1024], q[1024];
  snprintf(p, sizeof p, "%s/before.txt", dir);
  check("ext_baseline_write_denied_before_consume", 0, try_write(p), p);
  snprintf(p, sizeof p, "%s/seed.txt", dir);
  check("ext_baseline_read_denied_before_consume", 0, try_read(p), p);
  if (home_probe) {
    snprintf(p, sizeof p, "%s/.zshrc", home_probe);
    check("realhome_read_denied", 0, try_read(p), p);
  }

  snprintf(p, sizeof p, "%s/early.txt", dir);
  unlink(p);
  char sh[1400];
  snprintf(sh, sizeof sh, "sleep 4; echo x > '%s'", p);
  pid_t early = fork();
  if (early == 0) {
    int z = open("/dev/null", O_RDWR);
    dup2(z, 1);
    dup2(z, 2);
    execl("/bin/sh", "sh", "-c", sh, (char *)NULL);
    _exit(127);
  }

  char *tok = read_token(tokfile);
  if (!tok || !*tok) {
    result("ext_consume", "FAIL", "no token in %s", tokfile);
    return 1;
  }
  int64_t h = sandbox_extension_consume(tok);
  check("ext_consume", 1, h > 0, h > 0 ? "handle ok" : strerror(errno));

  snprintf(p, sizeof p, "%s/parent.txt", dir);
  check("ext_parent_write_after_consume", 1, try_write(p), p);
  snprintf(p, sizeof p, "%s/seed.txt", dir);
  check("ext_parent_read_after_consume", 1, try_read(p), p);

  snprintf(p, sizeof p, "%s/child-sh.txt", dir);
  snprintf(sh, sizeof sh, "echo child > '%s'", p);
  char *a1[] = {"/bin/sh", "-c", sh, NULL};
  spawn_check("ext_child_sh_write_after_consume", 1, a1, p);

  char *a2[] = {"/usr/bin/git", "-C", (char *)dir, "status", "--porcelain", NULL};
  spawn_check("ext_child_git_status_after_consume", 1, a2, NULL);

  snprintf(p, sizeof p, "%s/stub.txt", dir);
  char *a3[] = {"/bin/sh", (char *)stub, (char *)dir, NULL};
  spawn_check("ext_child_stub_agent_write_after_consume", 1, a3, p);

  snprintf(p, sizeof p, "%s/grandchild.txt", dir);
  snprintf(sh, sizeof sh, "/bin/sh -c \"/bin/sh -c 'echo gc > %s'\"", p);
  char *a4[] = {"/bin/sh", "-c", sh, NULL};
  spawn_check("ext_grandchild_write_after_consume", 1, a4, p);

  if (helper) {
    snprintf(p, sizeof p, "%s/helper.txt", dir);
    char *a5[] = {(char *)helper, "write", p, NULL};
    spawn_check("ext_signed_inherit_helper_child_write_after_consume", 1, a5, p);
  }

  sleep(5);
  snprintf(p, sizeof p, "%s/early.txt", dir);
  result("info_child_spawned_before_consume_gained_access", file_has(p) ? "INFO" : "INFO", "%s",
         file_has(p) ? "yes (extension visible to earlier child)" : "no (earlier child stays denied)");
  waitpid(early, NULL, 0);

  char *t2 = sandbox_extension_issue_file(CLASS_RW, dir, 0);
  if (t2) {
    write_file(tokfile, t2);
    result("ext_sandboxed_process_can_reissue_token", "PASS", "len=%zu", strlen(t2));
  } else {
    result("ext_sandboxed_process_can_reissue_token", "FAIL", "%s", strerror(errno));
  }
  (void)q;
  return 0;
}

static int cmd_peer(const char *dir, const char *tokfile) {
  char p[1024];
  snprintf(p, sizeof p, "%s/peer-before.txt", dir);
  check("ext_isolation_separate_sandboxed_process_denied", 0, try_write(p), p);
  snprintf(p, sizeof p, "%s/peer-child-before.txt", dir);
  char sh[1200];
  snprintf(sh, sizeof sh, "echo x > '%s'", p);
  char *a[] = {"/bin/sh", "-c", sh, NULL};
  spawn_check("ext_isolation_separate_process_child_denied", 0, a, p);
  char *tok = read_token(tokfile);
  int64_t h = tok ? sandbox_extension_consume(tok) : -1;
  check("ext_reissued_token_consumed_by_second_process", 1, h > 0, h > 0 ? "ok" : strerror(errno));
  snprintf(p, sizeof p, "%s/peer-after.txt", dir);
  check("ext_second_process_write_after_consume", 1, try_write(p), p);
  snprintf(p, sizeof p, "%s/peer-child-after.txt", dir);
  snprintf(sh, sizeof sh, "echo x > '%s'", p);
  char *b[] = {"/bin/sh", "-c", sh, NULL};
  spawn_check("ext_second_process_child_write_after_consume", 1, b, p);
  return 0;
}

static void exec_probe(const char *name, const char *path, char *const argv[]) {
  char out[300] = "";
  int rc = run(argv, 25, out, sizeof out - 1);
  char d[400];
  snprintf(d, sizeof d, "%s rc=%d out=%.160s", path, rc, out);
  check(name, 1, rc == 0, d);
}

static int cmd_execs(const char *realhome) {
  char p[600];
  char *git[] = {"/usr/bin/git", "--version", NULL};
  exec_probe("exec_usr_bin_git", "/usr/bin/git", git);
  char *zsh[] = {"/bin/zsh", "-c", "echo zsh-ok", NULL};
  exec_probe("exec_bin_zsh", "/bin/zsh", zsh);
  char *bash[] = {"/bin/bash", "-c", "echo bash-ok", NULL};
  exec_probe("exec_bin_bash", "/bin/bash", bash);
  char *brew[] = {"/opt/homebrew/bin/git", "--version", NULL};
  if (file_has("/opt/homebrew/bin/git")) exec_probe("exec_opt_homebrew_bin_git", "/opt/homebrew/bin/git", brew);
  char *tmux[] = {"/opt/homebrew/bin/tmux", "-V", NULL};
  if (file_has("/opt/homebrew/bin/tmux")) exec_probe("exec_opt_homebrew_bin_tmux", "/opt/homebrew/bin/tmux", tmux);
  char *node[] = {"/opt/homebrew/bin/node", "-e", "console.log(process.version, require('os').userInfo().homedir)", NULL};
  if (file_has("/opt/homebrew/bin/node")) exec_probe("exec_opt_homebrew_bin_node", "/opt/homebrew/bin/node", node);
  char *dummy1[] = {"/usr/local/bin/ovs-dummy", NULL};
  exec_probe("exec_usr_local_bin_dummy_script", "/usr/local/bin/ovs-dummy", dummy1);
  snprintf(p, sizeof p, "%s/.local/bin/ovs-dummy", realhome);
  char *dummy2[] = {p, NULL};
  exec_probe("exec_real_home_local_bin_dummy_script", p, dummy2);
  char *ps[] = {"/bin/ps", "-axo", "pid,ppid,command", NULL};
  exec_probe("exec_ps_axo", "/bin/ps", ps);
  char *lsof[] = {"/usr/sbin/lsof", "-nP", "-i", "-a", "-p", "1", NULL};
  char out[300] = "";
  int rc = run(lsof, 20, out, sizeof out - 1);
  result("exec_lsof", rc >= 0 && rc < 2 ? "PASS" : "FAIL", "rc=%d out=%.160s", rc, out);
  char *sec[] = {"/usr/bin/security", "list-keychains", NULL};
  exec_probe("exec_security_list_keychains", "/usr/bin/security", sec);
  char *sec2[] = {"/usr/bin/security", "find-generic-password", "-s", "ovs-nonexistent-item", NULL};
  rc = run(sec2, 20, out, sizeof out - 1);
  result("exec_security_find_generic_password", rc == 44 ? "PASS" : "FAIL", "rc=%d (44=not found, i.e. reachable) out=%.160s", rc, out);
  char *env[] = {"/bin/zsh", "-l", "-i", "-c", "env", NULL};
  time_t t0 = time(NULL);
  rc = run(env, 20, out, sizeof out - 1);
  result("exec_login_interactive_shell_env", rc == 0 ? "PASS" : "FAIL", "rc=%d secs=%ld out=%.160s", rc, (long)(time(NULL) - t0), out);
  char *lc[] = {"/bin/launchctl", "list", NULL};
  rc = run(lc, 20, out, sizeof out - 1);
  result("exec_launchctl_list", rc == 0 ? "PASS" : "FAIL", "rc=%d out=%.160s", rc, out);
  return 0;
}

static int cmd_pty(void) {
  const char *shells[][4] = {{"/bin/zsh", "-f", "-i", NULL}, {"/bin/bash", "--noprofile", "--norc", "-i"}, {"/bin/zsh", "-l", "-i", NULL}};
  const char *names[] = {"pty_zsh_interactive", "pty_bash_interactive", "pty_zsh_login_interactive"};
  for (int i = 0; i < 3; i++) {
    int m;
    struct winsize ws = {24, 80, 0, 0};
    pid_t pid = forkpty(&m, NULL, NULL, &ws);
    if (pid < 0) {
      result(names[i], "FAIL", "forkpty: %s", strerror(errno));
      continue;
    }
    if (pid == 0) {
      setenv("TERM", "xterm-256color", 1);
      setenv("PS1", "$ ", 1);
      execv(shells[i][0], (char *const *)shells[i]);
      _exit(127);
    }
    const char *script = "echo JC$((6*7)); tty; sleep 30 & jobs; kill %1; wait; echo DONE$((1+1)); exit\n";
    sleep(1);
    write(m, script, strlen(script));
    char buf[8192];
    size_t len = 0;
    time_t end = time(NULL) + 12;
    while (time(NULL) < end && len < sizeof buf - 1) {
      struct pollfd pf = {m, POLLIN, 0};
      if (poll(&pf, 1, 300) > 0) {
        ssize_t k = read(m, buf + len, sizeof buf - 1 - len);
        if (k <= 0) break;
        len += k;
        buf[len] = 0;
        if (strstr(buf, "DONE2")) break;
      }
    }
    buf[len] = 0;
    kill(pid, SIGKILL);
    waitpid(pid, NULL, 0);
    close(m);
    int bad = strstr(buf, "tty pgrp") || strstr(buf, "no job control") || strstr(buf, "Operation not permitted");
    int good = strstr(buf, "JC42") && strstr(buf, "DONE2") && strstr(buf, "[1]");
    for (char *c = buf; *c; c++)
      if (*c == '\n' || *c == '\r' || *c == '\t') *c = ' ';
    char tail[300];
    size_t off = len > 280 ? len - 280 : 0;
    snprintf(tail, sizeof tail, "%s", buf + off);
    result(names[i], good && !bad ? "PASS" : "FAIL", "job_control_error=%d echo_and_jobs=%d tail=%s", bad, good, tail);
  }
  return 0;
}

static int cmd_misc(const char *tmpdir, const char *realhome) {
  char p[1024], out[400] = "";
  int ls = socket(AF_INET, SOCK_STREAM, 0);
  struct sockaddr_in a = {.sin_len = sizeof a, .sin_family = AF_INET, .sin_addr.s_addr = htonl(INADDR_LOOPBACK)};
  int bound = bind(ls, (struct sockaddr *)&a, sizeof a) == 0 && listen(ls, 4) == 0;
  socklen_t al = sizeof a;
  getsockname(ls, (struct sockaddr *)&a, &al);
  check("net_listen_127_0_0_1_tcp", 1, bound, bound ? "bound" : strerror(errno));
  int c = socket(AF_INET, SOCK_STREAM, 0);
  check("net_connect_127_0_0_1_tcp", 1, connect(c, (struct sockaddr *)&a, sizeof a) == 0, strerror(errno));

  struct addrinfo hints = {.ai_socktype = SOCK_STREAM}, *res = NULL;
  int g = getaddrinfo("github.com", "443", &hints, &res);
  int ok = 0;
  if (g == 0 && res) {
    int s = socket(res->ai_family, SOCK_STREAM, 0);
    ok = connect(s, res->ai_addr, res->ai_addrlen) == 0;
    close(s);
  }
  check("net_outbound_github_443", 1, ok, g ? gai_strerror(g) : strerror(errno));

  struct sockaddr_un un;
  int us = socket(AF_UNIX, SOCK_STREAM, 0);
  memset(&un, 0, sizeof un);
  un.sun_len = sizeof un;
  un.sun_family = AF_UNIX;
  snprintf(p, sizeof p, "%s/s.sock", tmpdir);
  snprintf(un.sun_path, sizeof un.sun_path, "%s", p);
  unlink(p);
  check("unix_socket_bind_in_container_short_path", 1, bind(us, (struct sockaddr *)&un, sizeof un) == 0, p);

  snprintf(p, sizeof p, "%s/t.sock", tmpdir);
  if (file_has("/opt/homebrew/bin/tmux")) {
    char *n[] = {"/opt/homebrew/bin/tmux", "-S", p, "new-session", "-d", "-s", "ovs", "sleep 20", NULL};
    int rc = run(n, 20, out, sizeof out - 1);
    char *l[] = {"/opt/homebrew/bin/tmux", "-S", p, "list-sessions", NULL};
    char out2[300] = "";
    int rc2 = run(l, 20, out2, sizeof out2 - 1);
    result("tmux_new_session_in_container", rc == 0 && rc2 == 0 ? "PASS" : "FAIL", "new rc=%d %.100s list rc=%d %.100s", rc, out, rc2, out2);
    char *k[] = {"/opt/homebrew/bin/tmux", "-S", p, "kill-server", NULL};
    run(k, 10, NULL, 0);
  } else {
    result("tmux_new_session_in_container", "INFO", "tmux not installed on the runner");
  }

  snprintf(p, sizeof p, "%s/Library/LaunchAgents/ovs-probe.plist", realhome);
  check("write_real_home_launchagents", 0, try_write(p), p);
  snprintf(p, sizeof p, "%s/.claude", realhome);
  char q[1100];
  snprintf(q, sizeof q, "%s/ovs-hook.json", p);
  check("write_real_home_dot_claude", 0, try_write(q), q);
  snprintf(q, sizeof q, "%s/.claude/settings.json", getenv("HOME"));
  snprintf(p, sizeof p, "%s/.claude", getenv("HOME"));
  mkdir(p, 0755);
  check("write_container_home_dot_claude", 1, try_write(q), q);
  return 0;
}

static int cmd_write(const char *path) { return try_write(path) ? 0 : 1; }

static int cmd_daemon(const char *bin, const char *sock) {
  setenv("OVERSEER_SOCKET", sock, 1);
  char *a[] = {(char *)bin, "daemon", "--detach", NULL};
  char out[500] = "";
  int rc = run(a, 40, out, sizeof out - 1);
  result("daemon_detach_from_sandboxed_parent", rc == 0 ? "PASS" : "FAIL", "rc=%d out=%.300s", rc, out);
  return 0;
}

int main(int argc, char **argv) {
  if (argc < 2) return 2;
  const char *m = argv[1];
  if (!strcmp(m, "issue") && argc > 2) return cmd_issue(argv[2]);
  if (!strcmp(m, "suite") && argc > 5) return cmd_suite(argv[2], argv[3], argv[4], argv[5], argc > 6 ? argv[6] : NULL);
  if (!strcmp(m, "peer") && argc > 3) return cmd_peer(argv[2], argv[3]);
  if (!strcmp(m, "execs") && argc > 2) return cmd_execs(argv[2]);
  if (!strcmp(m, "pty")) return cmd_pty();
  if (!strcmp(m, "misc") && argc > 3) return cmd_misc(argv[2], argv[3]);
  if (!strcmp(m, "write") && argc > 2) return cmd_write(argv[2]);
  if (!strcmp(m, "daemon") && argc > 3) return cmd_daemon(argv[2], argv[3]);
  return 2;
}
