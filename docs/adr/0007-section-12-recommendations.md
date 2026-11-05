# 0007 Recommendations for the open decisions (brief section 12)

Status: proposed, for the user to decide

1. **How an environment is requested**: PR label `preview` read by Yard. Overseer stores only
   `envRef`. (Agrees with the brief.)
2. **Dev takes over an agent's line**: SSH attach by default, new local line on demand. Attach keeps
   the exact session; a new line is created only when the runner is gone or the dev asks.
3. **What Yard is built on**: not Overseer's call; keep `envRef` opaque.
4. Overseer-specific:
   - Language: Go + generated TS types (ADR 0001).
   - Event delivery: SSE `GET /v1/events` with `Last-Event-ID` replay (7 days) as the primary channel,
     plus HMAC-signed webhooks for consumers that cannot hold a connection. No broker.
   - Held-line representation: stub in the same pane + `@sb_held` (ADR 0005).
   - Lines of offline stations on the Board: shown greyed with "station offline since <lastSeenAt>",
     state frozen at the last known value, `resume`, `hold` and `close` disabled with the reason
     (the API refuses commands to offline stations rather than queueing them).
   - New: hook transport, own hook + polling fallback vs cmux-tui journal (ADR 0004).
