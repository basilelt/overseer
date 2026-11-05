# Privacy policy

Effective date: 5 November 2026

This policy covers the Overseer apps for Android, iOS, macOS, Windows and Linux, published by
Basile Le Thiec ("we"). Overseer is open source: https://github.com/basilelt/overseer.

## Summary

Overseer connects your device to an Overseer server that you run on your own computer or server.
We do not run any server for the app, and we do not collect, store, sell or share any of your data.
The app contains no analytics, no advertising and no tracking.

## Data the app handles

- **Connection details**: the host names, user names and ports of your servers, and the SSH key the
  app creates to connect to them. They are stored only on your device; secrets are kept in the
  platform's secure storage (Android Keystore, iOS Keychain).
- **Your work**: sessions, terminals, code changes, files and messages travel only between the app
  and your own server, over an encrypted SSH connection or your local network. They are never sent
  to us.
- **Logs and crash reports**: kept on your device and your server. They leave them only when you
  copy them yourself or include them in a feedback report (see below). Secrets are masked.

## Third parties

- **GitHub**: to install or update the Overseer server on a host, the app downloads release files
  from github.com. GitHub receives the usual request data (such as your IP address) under the
  GitHub Privacy Statement.
- **Feedback**: when you choose to send feedback, an agent runs on your own server and creates a
  GitHub issue with your own GitHub account. You see and can edit everything before it is sent.
- **AI coding agents**: the agents you start (such as Claude Code or Codex) run on your server under
  your own accounts with their providers; their providers' terms and privacy policies apply. The app
  does not send them anything itself.

## Children

Overseer is a developer tool and is not directed at children under 13.

## Deleting your data

All app data is on your device and your servers. Uninstall the app or clear its storage to delete
it from the device; remove the `~/.overseer` folder to delete it from a server.

## Changes

Changes to this policy are published at this address, with a new effective date.

## Contact

Open an issue at https://github.com/basilelt/overseer/issues.
