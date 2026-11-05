# Security policy

## Supported versions

Only the latest release gets security fixes. Every merge to `main` is a release, so a fix ships
as a new patch version; update the desktop app, the daemon and the mobile apps to it.

## Report a vulnerability

Do not open a public issue, pull request or discussion for a security problem.

Report it privately through GitHub:
[Security > Report a vulnerability](https://github.com/basilelt/overseer/security/advisories/new).
Include what an attacker can do, the version, the platform, and the steps or a proof of concept
to reproduce it.

You get an answer within 7 days. Once the problem is confirmed, a fix is prepared in a private
fork, released, and the advisory is published with credit to you unless you prefer otherwise.

## Scope

In scope, for example:

- the daemon: its local socket and HTTP API, authentication tokens, the board served on loopback,
  file access outside the workspaces it manages, command execution;
- the remote mode over SSH and the phone apps' connection to a server;
- the desktop app: the Tauri IPC surface, the webview, links and files opened from agent output;
- updates and releases: update signature checks, the release workflow, the published artifacts.

Out of scope: attacks that already need full control of the user's account on the machine
running the daemon, denial of service by the local user, and vulnerabilities in the coding agents
Overseer launches (report those to their vendors).
