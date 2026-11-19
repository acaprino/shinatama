# Model proxy panel: never a dead end, always the cause

Design, 2026-11-18, short (a fix to existing behaviour). Approved by the owner in the session
where the incident below was diagnosed.

## Intent

Owner, 2026-11-18, while the proxy was down: "ancora problemi con il proxy!!!", "e non si riesce
a ripristinare", "ma perché non dice qual è la causa", "così non si capisce", "queste
incongruenze vanno fixate nella ui di shinatama", "ancora una volta non c'è gestione resiliente
dello stato della ui". The panel and the open errors must always say the real state and its cause,
and must always leave one action that works.

## What happened (the case this design answers)

- D: (data folder) reached 0 bytes free at 20:22. The proxy logged
  `ENOSPC ... SPILLOVER ALSO FAILED — data at risk` in its own log (`bili.log`), which Shinatama
  never reads; its process ended at 20:52.
- The supervisor's last published state was Recovering / CleanupUncertain; after that it never
  published again. `Controller::status` turns a snapshot older than 3 s into phase `unavailable`,
  and `buttons()` enables nothing in `unavailable`: Start, Retry recovery and Stop all disabled,
  with "Waiting to confirm cleanup before restarting the proxy" shown forever, also after space
  was freed. Only closing and reopening Shinatama brought the proxy back.
- Open refusals said "The Shinatama network sidecar is not ready" (Claude) and "The local session
  service changed during Codex preparation" (Codex, OpenCode, Pi): the engine's launch gate
  (`verify_sidecar`) drops the phase, reason and age it refused on.
- Panic or deadlock is unknown: a panicking thread is logged nowhere.

## Goals

1. The panel is never without an action that works.
2. Every refusal and every panel state says what is wrong and why, in the user's terms.
3. The next stall leaves evidence in `shinatama.log`.

## Non-goals

- Changing the proxy (the pinned sidecar source build).
- Replacing a stuck supervisor in-process.
- The resources indicator ("Resources: checking").

## Anti-goals

- A message that claims something unverified ("saved sessions are preserved" while saving fails).
- A recovery that can leave two owners of one proxy process, port or reservation.
- `status()` doing file or network I/O on the caller's thread.

## Approaches

- **A. Detect the stall, contain panics, offer Restart Shinatama (picked).** The controller marks
  a supervisor silent for 15 s as stalled; the panel says since when and what it last reported
  and offers Restart Shinatama behind a confirmation; a panic inside the supervisor's loop becomes
  Failed with its message, so Retry works without a restart. Restart is the one exit that always
  works, and the existing close path already restores the sessions at the next start.
- **B. Replace the stuck supervisor in-process.** Rejected: the supervisor is the single owner of
  the proxy process, its job and its admission reservation (`CLAIMED`); a second owner beside a
  stuck one can hold the same port and reservation twice.
- **C. Better wording only.** Rejected: the panel would still have no working button.

## Design

1. **Stall.** `Status` gains `observedAtUnixMs` (the supervisor's last observation) and `stalled`
   (no observation for more than 15 s while not Stopped, Failed or Retired). The panel shows
   "The proxy supervisor has not reported since HH:MM:SS (last state: <phase label>, <message>)."
   and a **Restart Shinatama** button, enabled whenever the status is stalled or could not be
   read. Its inline confirmation says that the running terminals stop and that their sessions are
   restored at the next start. Restart runs the existing close sequence (flush, release the layout,
   `terminal_shutdown`), then starts a new Shinatama that waits for the old process to end before
   it takes any lock, then closes the window.
2. **Panic containment.** The supervisor's loop body runs under `catch_unwind`; a panic publishes
   Failed with a new reason `SupervisorFault` and the panic text (bounded, control-free), so Start
   and Retry recovery are enabled.
3. **Disk.** A background sampler reads the free space of the data folder's volume every 10 s; the
   status carries it. Under 1 GB the panel says "Disk D: has N MB free"; under the database's own
   floor (65 MB) it says that saving fails, and no message claims anything is preserved.
4. **Open refusals.** One sentence for all four harnesses, built from the controller's status at
   the moment of refusal: phase, its message, since when, the last exit and the disk line when low.
   "The local session service changed during … preparation" goes.
5. **Evidence.** A panic hook writes one bounded line per panic (thread, location, message) to
   `shinatama.log`; the controller writes one line when the supervisor stalls and one when it
   reports again.

## Constraints

- AGENTS.md: work on a short branch from `develop`, merged back and deleted; update the install in
  place by copying the built executables; never change versions or tags by hand.
- Rebuilding needs several GB on D: (the `target` folder was deleted to free space on 2026-11-18).
- `shinatama.log` lines stay bounded and control-free (ADR-0030).

## Left to the builder

Exact sentences beyond those above, the sampler's home (controller or its own module), how the
new process waits for the old one (argument, timeout), and test placement.
