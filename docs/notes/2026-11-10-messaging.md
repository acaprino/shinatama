# C31 messaging between instances (2026-11-10)

`crates/messaging` + `crates/state/src/messages.rs` at `6d4514b`.

`cargo test -p shinatama-messaging`: 10 passed (cross-project deny and setting, 16 KB, expiry
<= 1 h and never delivered after it, redaction before storage, dedupe yields one message,
delivered vs acked, injection canary stays inside an escaped `<peer-message untrusted>`,
ping-pong stops at depth 6 without `in_reply_to`, 6/min rate limit, pause + alert, 5 retries
with 5/10/20/40/80 s backoff then failed with alert, offline recipient waits then receives,
paste never with a prompt/draft open or input in the last 10 s, pull-only stays queued for
the inbox, four-operation API surface).

Limits:
- Library only: no MCP tools (C32), no desktop directory of live instances, no transport to
  real agents, no UI for alerts. Per-agent channel meaning (R14) is declared, not measured (M4).
- A pause lifts after 10 minutes; there is no user "resume" control yet.
- Injection resistance per agent (R08, M6) is not measured: no real agent received a canary.
