# C46 — Windows attention widget preview

2026-11-03, Europe/Rome. The user explicitly requested an early version.

The local Windows build includes a transparent, borderless 440×190 widget with
three friendly generated Shinatama portraits from `docs/assets/widget/`, a short bubble and
dismiss/open actions. **Shinatama alerts** controls visual alerts and Windows
sound and offers **Prova viso e suono**. Boolean preferences use local WebView
storage; reduced-motion disables the brief arrival fade. The portrait itself is
static per notice, with random expression changes on new notices and no immediate
repeat. The invented SVG was removed after the user's correction. The widget runs in the desktop
process and exits with the main window. F30/F31's independent process, persistent
feed and permission answers remain separate work.

## Wiring

The existing Rust terminal-read path observes BEL, OSC9 and OSC777 with a bounded
streaming parser. It retains no payload. These controls mean only “terminal
requests attention”, never a certain input/approval request or finished agent
turn. Plain text, progress OSC9, unknown OSC and DCS/APC/PM/SOS create no notice.
Original bytes still reach xterm. EOF raises **process ended**, not turn complete.
Retained terminal metadata can be read independently from the live-process list.

The queue holds at most 16 notices, coalesces by session and removes test notices
on real events. Signal cooldown is five seconds per session; sound cooldown is
three seconds globally. There are at most eight retained parsers. Only main can
configure or create test notices; the widget can read, open or dismiss the current
id. Stale ids are refused. Open consumes that exact notice atomically; workspace
modals defer session focus until close. No action writes terminal input.

Windows positioning/showing use SetWindowPos with NOACTIVATE and SHOWWINDOW;
hiding uses SW_HIDE. The window also has WS_EX_NOACTIVATE and TOPMOST. Merely
setting the window property proved insufficient in the first UI attempt.
Placement respects monitor work area, negative coordinates and DPI.

## Executed checks

1. `npm run build` — passes, main and widget bundled.
2. `cargo build --locked -p shinatama-desktop --features custom-protocol -j1` —
   passes. The executable used by `Shinatama.cmd` includes the preview.
3. `cargo test --locked -p shinatama-desktop --bin shinatama-desktop attention` —
   eight tests pass: chunk boundaries, cancellation, ignored strings/progress,
   malformed/oversized controls, no text heuristics, flood and queue bounds.
4. `node apps/desktop/tests/attention-smoke.mjs --surface-only` — passes in an
   isolated home/profile. Native hidden startup, visible/topmost/noactivate,
   unchanged foreground in the tested appearance, sound path, mute/reload
   preferences, IPC privilege rejection, stale ids, disable/hide, no WebView
   errors, and exit of both windows on main close. Physical audio listening is
   not automated.

Receipt: `.prototype-evidence/attention-widget.json`, status
`surface_pass_live_deferred`. Screenshot:
`.prototype-evidence/attention-widget-native.png` is an explicitly labelled test.

Portrait correction, 2026-11-03: the user rejected the invented face. It was
replaced with the canonical unchanged Shinatama PNG already in the repository.
Frontend and embedded desktop builds passed again, as did the native surface
smoke with the image loaded at its original 128×128 resolution. The screenshot
above was regenerated with the corrected portrait. Source and bundled PNG have
the same SHA256: `ec63d6675c392eb02251bcb602dc71ae8fdef8941a8397c06b61263bf8a65d44`.

Later the same day the user requested an ImageGen redraw and two or three less
serious expressions. Three transparent 1254×1254 RGBA portraits were generated
with the built-in tool: smile, happy and curious. All have alpha spanning 0–255.
They replace the small canonical image in the popup; the reference remains
unchanged. Source assets and exact final prompts:
[generation record](../assets/widget/generation.md). The popup begins with smile,
then randomly chooses one of the other expressions only for a new notice id.
The frontend and embedded desktop builds pass with all three portraits bundled.
The native surface smoke passes again, including the new checks that the
high-resolution generated portrait loads, a refresh preserves its expression,
and a new notice selects a different expression. The native screenshot and JSON
receipt were regenerated with these assets.

## Remaining proof

The full `node apps/desktop/tests/attention-smoke.mjs` attempted a normal contained
PowerShell start. The resource governor refused it: “New terminals are waiting
for healthy resource measurements.” A bounded wait remained at **Resources:
reduced activity**. No governor bypass or weaker policy was introduced.

Live PTY → bubble → correct-session-click → EOF is wired but not qualified by
this record. No provider, live harness or live permission prompt was exercised.
Claude's qualified launch keeps safe mode and hooks disabled. Per-harness
pending-request detection and final W gates remain open. C46 stays pending until
the full proof can run with normal resource admission.
