# Spec: Muro keeps the plugins of Shinatama's agents in sync

Status: draft for the owner's review, 2026-11-19. Decisions taken by the owner on 2026-11-19:
a Muro registry of Shinatama's own, Muro shipped in the package, a sync at every start plus
Sync now and Open Muro in Settings, and the sync run by Muro's own background engine, which
Shinatama launches (Muro ADR-0028), not by Shinatama.

## Problem Statement

Shinatama launches Claude Code, Codex, OpenCode and Pi each in a private home of its own, a box
in Shinatama's data folder. The plugins the user manages with Muro go to the harnesses' usual
homes, never to Shinatama's boxes, so the agents Shinatama launches do not have them. Putting a
plugin into a box today means doing it by hand, for each agent, and keeping it current by hand.
The user wants Shinatama itself to keep the plugins of the agents it launches in sync, with Muro
as the tool that manages and syncs them, reachable from Shinatama.

## Solution

Shinatama ships Muro and keeps a Muro registry of its own in its data folder. At every start Shinatama makes
sure that each of its agent boxes is a Muro harness in that registry, then launches its Muro in
the background on that registry. Muro watches those harnesses and enforces the list itself, as
it does for any registry (Muro ADR-0028): it checks the sources, plans and applies, and records
the state. Shinatama shows that recorded state. Settings gains a Plugins section
that tells what the last sync did and offers Sync now and Open Muro. Open Muro shows Muro's own
window on Shinatama's registry, where the user chooses the plugins of Shinatama's agents with all
of Muro's features: sources, update policies, adaptation. Shinatama never touches the user's own
Muro registry or the harnesses' usual homes.

## User Stories

1. As a Shinatama user, I want the agents Shinatama launches to get the plugins I choose for them, so that their sessions have the tools I rely on.
2. As a Shinatama user, I want to choose those plugins in Muro, so that I use one tool with sources, update policies and adaptation instead of copying files.
3. As a Shinatama user, I want to open Muro from Shinatama's Settings, so that I do not have to find, install or configure it.
4. As a Shinatama user, I want Muro to open on Shinatama's own list, so that a change I make there never reaches my other harnesses.
5. As a Shinatama user, I want my own Muro and my harnesses' usual homes left alone, so that Shinatama has no effect outside its own folder.
6. As a Shinatama user, I want each of Shinatama's agents to appear in Muro as a harness named after Shinatama and the agent, so that I recognise them.
7. As a Shinatama user, I want an agent I have never launched to appear once Shinatama has created its home, so that Muro never points at a folder that does not exist.
8. As a Shinatama user, I want Shinatama to sync at every start without making me wait, so that the window opens at once and the plugins are current when I open a session.
9. As a Shinatama user, I want a Sync now button, so that a plugin I just chose in Muro reaches my agents without a restart.
10. As a Shinatama user, I want to see when the last sync ran and what it did (installed, updated, removed, nothing to do), so that I know my agents are current.
11. As a Shinatama user, I want to see in plain words why a sync did not run or failed (Muro not in this build, Muro switched off, Muro open on the same list, a blocker in the plan, an error), so that I know what to do.
12. As a Shinatama user, I want a sync to apply only what Muro planned for Shinatama's harnesses, so that nothing else changes.
13. As a Shinatama user, I want a sync to leave alone the plugins it did not install, so that Marionette, which Shinatama gives every session itself, and anything I added by hand stay.
14. As a Shinatama user, I want Marionette to keep reaching every session as today, so that the new sync changes nothing I already have.
15. As a Shinatama user, I want a fresh install to have Muro with no separate download, so that Open Muro works from the first start.
16. As a Shinatama user, I want an empty list to change nothing, so that a fresh install never alters its agents.
17. As a Shinatama user, I want a slow or failing sync never to delay or block an agent launch, so that my work never waits for Muro.
18. As a Shinatama user, I want pressing Sync now during a sync to give me that sync's result instead of starting another, so that two syncs never race on the same list.
19. As a Shinatama user, I want Shinatama to tell me when Muro's window is open on Shinatama's list and the sync waits for it, so that I understand why nothing happened.
20. As a Shinatama user, I want a session that is open while a sync installs a plugin to keep running, so that the plugin applies from that agent's next start.
21. As a Shinatama user, I want a switch that turns the integration off, as for Marionette and Xiox, so that I can rule it out while troubleshooting.
22. As a Shinatama user, I want the Plugins section to work at the window's minimum width, from the keyboard and in both looks, like the rest of Settings.
23. As a Shinatama user, I want each sync recorded in Shinatama's log, so that a failure can be understood later.
24. As a maintainer, I want Muro pinned to one exact release whose archive is checked against its SHA-256, so that a package never ships an unverified Muro.
25. As a maintainer, I want moving to a newer Muro to be one pin file and a rebuild, so that a Muro release reaches users with the next Shinatama release.
26. As a maintainer, I want a development build without the shipped Muro to say so instead of failing, so that the app runs from source.

## Implementation Decisions

- **Package.** The pinned Muro release is Muro's portable Windows zip of one `muro-v<version>` release of acaprino/muro, checked against its SHA-256, unpacked whole into a Muro folder of the package beside the program. The release fetch script gains a mode that unpacks a whole archive; it keeps checking the archive's digest and size. Muro's own data folder in the package is never used.
- **Registry.** Shinatama's Muro registry is a folder in Shinatama's data home. Every Muro call names it explicitly: the command-line tool's data argument, and the data-folder variable for Muro's window. Muro's portable default is never relied on.
- **Harnesses (targets).** One Muro target for each agent box that exists: identifier `shinatama-<harness>`, a name made of Shinatama and the agent's name, the harness kind (claude-code, codex, opencode, pi), the agent's configuration folder in its box as root, the executable that Muro's own resolver returns for that harness, enabled, never exclusive. The root is the box home for Claude Code, Codex and Pi, and the OpenCode folder of the box's XDG configuration for OpenCode, which Muro configures through that folder's file. Shinatama creates a missing target and updates one that differs, with the registry revision Muro returned; it never removes a target and never edits plugins.
- **Sync is Muro's.** Shinatama never plans or applies. Before launching its Muro, Shinatama registers the targets with one-shot requests and sets the registry's automatic sync on with its interval. Then it starts its Muro in the background on that registry, on a thread of its own after Marionette's seed and sync. Muro's worker resolves the sources, plans, applies and records the state, at its start and at every interval.
- **One owner.** Muro lets one process own a registry. Shinatama's own Muro is that owner while Shinatama runs; Shinatama reads the registry state from the state file Muro records, never with a second request process. Shinatama stops its Muro when Shinatama exits.
- **Status.** Read from Muro's recorded sync state and given to the page: not in this build, switched off, starting, last check and last sync with their times, the pending changes and the harnesses out of date, or the error Muro recorded. Shinatama logs when it starts or stops its Muro and when Muro records an error.
- **Commands.** Status, Sync now (asks the running Muro to check and apply now), and Open Muro (shows the window of Shinatama's Muro; refused, with the reason, when Muro is not in the build or is switched off).
- **Never global.** Shinatama's Muro never registers itself to start with Windows and never touches the user's own Muro, its registry or its tray entry.
- **Settings.** A Plugins section: one status line and two buttons, Sync now and Open Muro, disabled with the reason when they cannot run.
- **Switch.** `SHINATAMA_MURO=off` turns it all off: no targets, no sync, the buttons disabled and the status saying why.
- **Records.** A new decision record for the integration; the package contents in the install and release requirements; the user guide and the README.

## Testing Decisions

- A good test checks behaviour seen from outside: which requests Shinatama sends to Muro and what it does with Muro's answers, never the internal structure.
- One seam: a Muro runner that sends one request to Muro's command-line tool, asks Muro's resolver, or starts and stops Shinatama's Muro, and returns Muro's answer or its error. Tests use a fake runner. Prior art: Marionette's fetch interface and its fakes in the engine's unit tests.
- Unit tests at that seam: the target payloads built from box layouts (each harness's root, a missing box skipped, an equal target not saved again, the revision passed); automatic sync switched on in Shinatama's registry before the launch; the status read from a recorded state file (each state, a missing or unreadable file); the switch; Muro missing.
- Packaging: the fetch step run alone for the pin into a scratch folder, as for Marionette.
- Real run: the debug app with an isolated data home and the fetched Muro: targets appear in Muro, a plugin chosen there is installed into the box by Sync now.
- The page: the existing TypeScript test runner for the status text the Settings section shows for each outcome.

## Out of Scope

- The user's own Muro registry and the harnesses' usual homes.
- Plugin sets per instance (one tab's own set): Muro targets the agent's box only.
- Moving Marionette under Muro: Marionette keeps its own sync and per-launch injection.
- Hermes Agent and DeepSeek Harness.
- A plan or apply loop of Shinatama's own: Muro's background engine does it.
- Starting Muro's AI adaptation from Shinatama: it is available in Muro's window.

## Further Notes

- Muro's resolver returns npm launchers for OpenCode and Pi. Muro configures those two through their files and does not run them, so the targets are valid.
- Muro 0.3.3's portable zip is 72 MB; unpacked it adds more to the package.
- Muro's registry lock: one process owns Shinatama's registry, the Muro Shinatama launched.

### What Shinatama needs from a Muro release

Checked against Muro's `develop` on 2026-11-19 evening (ADR-0028 plan 1, unreleased; the latest
release, 0.3.3, has none of it):

| Need | Muro `develop` |
|---|---|
| Background start on a named registry | Yes: `--background` with the data-folder variable |
| Show the window of a running Muro from another process | Yes: a second start on the same registry raises it |
| Sync now from another process | Yes: the `sync_now` request, beside the running Muro |
| No Start-with-Windows entry of its own for a hosted instance | Yes: a separate autostart name, and nothing is written unless turned on |
| Recorded sync state | Yes: the sync and automation state files, carried by `snapshot` |
| A tray entry that says it is Shinatama's | No: the tray always says Muro |
| A graceful quit a host can ask for | No: only the tray's Quit, which waits for the sync |
| A start that survives a request already holding the registry | No: it ends with exit code 101 (found 2026-11-19); Shinatama sends nothing while its Muro starts and starts it once more |

Owner decision of 2026-11-19: Shinatama's Muro quits with Shinatama. Until Muro offers a quit
request, Shinatama takes the registry's operation lock (which waits for an apply that runs and
keeps the next one from starting), then ends the Muro it started; past a bounded wait it leaves it
running. Muro's Start-with-Windows name accepts letters, digits, `-` and `_` only:
Shinatama's is `Muro-Shinatama`. Shinatama ships Muro
from the first Muro release that has the rows marked Yes; until then it finds a Muro folder beside
its program or named by `SHINATAMA_MURO`.
