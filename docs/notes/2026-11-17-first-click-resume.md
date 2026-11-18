# Resume works at the first click; failures say why

Date: 2026-11-17. Status: implemented. Reported by the user: no Resume worked at the first
click, it took two or three, each failed click showed an error first, and the window kept
showing "The saved layout is unavailable. The current workspace is preserved."

## What happened

The first open after a start (a Resume, a New session) does three things that a later open
does not, each against a short fixed deadline:

1. it starts the model proxy, which nothing had started yet: 30 s
   (`agents/service.rs`, `ensure_sidecar`);
2. it checks the harness, starting its TUI in a scratch folder and waiting for its first
   screen: 7 s (`run_canary`);
3. it prepares the launch, including the SHA-256 of the whole harness executable (Claude's is
   ~370 MB, read cold at the first launch, often under an antivirus scan): 10 s
   (`PREPARE_BUDGET` of the four launchers).

A missed deadline failed the click but kept what had been warmed up (the proxy kept
starting, the executable sat in the file cache), so the second or third click passed. On the
user's machine on 2026-11-17 the proxy was started at 12:08:23 by the first click, listened at
12:08:38, and the first Resume that opened was at 12:09:44.

New session hid most of it: the window retries for up to 60 s when the answer says "still
finishing". Resume had no retry, and `agents/resume.rs` turned every start failure into the
code `unavailable` and dropped the harness's reason, so the window said only "The conversation
could not be resumed; its saved conversation is kept. Try again." Nothing was logged, so which
deadline failed on the user's machine cannot be proven after the fact. The smoke tests never
saw it: each one starts the proxy, or a New session, before its first Resume.

## What changed

- **The proxy starts with Shinatama** (`sidecar::start_at_launch`), with the same admission as
  an open; a user's Stop still stops it.
- **An open the user clicked waits** instead of failing: up to 2 minutes for the proxy it
  started (sooner on a failed, retired or stopping proxy; a proxy waiting for free memory
  retries by itself and the open waits for it), up to 60 s for a ready proxy that missed health
  checks (was 5 s), up to 30 s for the TUI check (was 7 s; it ends at the first screen), 60 s
  for launch preparation (was 10 s; it ends when the hash ends).
- **The window says what it waits for**: each step (`proxy`, `proxy_resources`, `check`,
  `reading`, `prepare`, `start`) is sent as `agent-opening` and shown as "Opening Claude Code:
  checking Claude Code…" until the open ends.
- **A failure says why**: the Resume error carries the harness's reason (`message`) next to its
  code, and the window shows it.
- **`shinatama.log`** in the data folder (`app_log.rs`, 1 MiB, then `shinatama.log.1`): each
  open and Resume with its outcome, reason and duration, when the proxy was ready, the Marionette
  sync, layout refusals and page loads. Fixed sentences, codes and durations only.

## The saved layout notice

It appears when the window cannot read its saved layout at load. The window then keeps the panes
it has and, until the next start, does not save its layout. The notice dropped the reason. The
user's saved layout passes the window's own checks, and the notice did not reproduce on a fresh
data folder, after a normal close or after a forced kill, so its cause on the user's machine is
not proven. One cause is in the code: one layout operation at a time, and a second one (the load
of a page that replaced one whose save was still in flight) was refused at once with "Workspace
storage is busy". Now it waits its turn for up to 5 s. A failed read of the process identity is
no longer kept for the whole session. A refusal that can pass is retried four times over 5 s.
The notice now gives the code and the storage's sentence, and the log records it.

## OpenCode: "The bundled plugins could not be verified for OpenCode."

At 13:13 something ran Marionette's Python scripts (muro) inside OpenCode's adapter in the synced
store, and Python wrote 15 `__pycache__/*.pyc` files there. The store's digest check refuses any
file its lock does not list, so every later OpenCode launch was refused. The new Marionette
0.3.0 release (14:26) is unrelated: it has the same tag, asset and layout, passes the sync's
checks, and the sync installs it at the next start.

- OpenCode and Pi now load a private copy of their adapter (`marionette-opencode\`,
  `marionette-pi\`), mirrored from the verified store at every launch, as Claude Code already
  did. What a session writes stays in the copy, and the next mirror removes it.
- When a harness's adapter fails its check, the session starts without Marionette and the log
  says why, as AVVIO.md already said. The sync at the next start replaces a release that fails
  its check.

## Verification

- Engine: `a_cache_written_where_opencode_loads_marionette_never_reaches_the_store` failed
  before the fix with the user's error (`BundledPlugins` at the second launch) and passes now;
  the same for Pi. `cargo test -p shinatama-engine --lib`: 213 passed.
- Desktop: the log's line and rotation tests; `cargo test -p shinatama-desktop`: 100 passed. Run
  them with TEMP outside any Git repository: a data folder inside one is refused.
- Frontend: the Resume reason and the layout refusal tests; `npm test`: 142 passed.
