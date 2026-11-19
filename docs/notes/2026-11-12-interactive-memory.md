# Interactive terminal memory (2026-11-12)

User decision: keep ongoing work running. Memory pressure is an advisory widget
event, not a reason to close or suspend an interactive terminal.

## Running terminals

Interactive shell and agent jobs omit the Windows per-process and per-job memory
limit flags. Their resource observer never requests a stop for memory growth.
The resource display shows measured consumption without presenting reservation
values as enforced memory caps. Existing process-count, CPU and queue constraints
are unchanged; internal background workloads keep their hard memory limits.

## Opening a new terminal

Admission checks fresh available physical memory and remaining system commit.
Both must cover the startup allowance and margin:

| Launch | Startup allowance | Margin | Minimum available in each counter |
|---|---:|---:|---:|
| PowerShell terminal | 64 MiB | 128 MiB | 192 MiB |
| Native agent terminal | 128 MiB | 128 MiB | 256 MiB |

The local service and version probes required by an explicit Open use this small
startup check too; they retain their hard memory limits. Optional background
work retains its separate conservative governor/admission policy. Concurrent
pending starts add their startup allowances to avoid spending the same margin.

These are launch checks, not ongoing memory caps or promises about an agent's
later consumption. The check does not subtract existing terminals' theoretical
reservations from the available counters a second time. A refusal leaves all
existing terminals running and explains the available and required memory.

A newly initialized reservation ledger uses the administrative `MAX_TOTAL`
ceiling independently of current free RAM. This does not allocate that memory
or require it to be free at startup. Older recognized policies that froze this
ceiling at half the free memory on first launch migrate to `MAX_TOTAL` only when
the ledger is empty, with a transaction comparing the complete policy and its
revision. Active and retired reservations are never changed by this migration;
unknown policies are refused. Slot and other aggregate limits remain in place.

## Widget warning

A backend worker checks cached observations every two seconds, including while
the main window is minimized. System and terminal samples must be fresh (at
most three seconds old).

- Warn when available RAM **or** remaining commit is below 512 MiB.
- Clear the pressure episode when **both** are at least 768 MiB, avoiding repeated
  warnings around one threshold.
- Identify the terminal with the largest measured private commit, its session
  label and consumption in MiB. Partial coverage is stated explicitly. This
  identifies the largest measured terminal, not the cause of all system pressure.
- Preserve pending input/completion notices. Updating one pressure episode does
  not replay its announcement; dismissal suppresses it until recovery.
- Keep keyboard focus unchanged. Opening the warning focuses its associated
  terminal only when the user requests it. No warning action stops or pauses work.

## Verification and rollout scope

Regression coverage targets memory growth without a stop, absence of Windows
memory-limit flags for interactive jobs, retained hard limits for background
jobs, small startup headroom, honest telemetry and warning lifecycle/ranking.
Historical C09 hard-cap verification remains historical evidence, not a proof
of this replacement behavior.

This note documents source behavior. It does not certify an installed or
deployed build. Jobs already created by an older running app retain that app's
original Windows configuration; updating source does not restart or alter them.
