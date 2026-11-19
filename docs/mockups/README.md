# Workspace mockups

Updated 2026-11-02 using the accepted readable FRDs (FRD-0001–FRD-0017).
These are illustrative interaction proposals, including later delivery slices.
Their screen arrangement, button placement and sample responses are not approved
requirements. Neither variant depicts only the first terminal-first milestone.
They use sample data; no agent, model, filesystem action or web request runs.
Preview settings and records reset when the page reloads.

Open [workspace.html](workspace.html) for the complete workspace, or
[workspace-lite.html](workspace-lite.html) for the compact terminal layout with
the same later-feature previews.
Keep workspace-features.css and workspace-features.js beside both HTML files.
No build or install is needed.

## Features to explore

1. **Learning:** companion beside the workers, preset actions, neutral Aided
   Decision cards, harness/model choice, quiz, web mode and daily cost cap.
   Approvals remain in the worker.
2. **Learning memory:** personal concept levels, dates, lower, delete, forget
   and disable. Viewed Learning cards update after two seconds; Aided Decision
   cards write nothing. This measures exposure, not understanding.
3. **Workspace:** Overview, projects, session search, Daodan tasks, moderated
   roundtable and browser routing. Import/export and native resume are previews.
   Expired native sessions offer a new conversation from seed.
4. **Agents and launch:** six harnesses, placement, per-instance sets, linked
   worktrees and routing. Lite includes a sample DeepSeek ACP chat.
   Owned program + system home is blocked.
5. **Settings:** model router/tool chooser, BillionContext, local julia1,
   optional TokenEconomizer (off by default), widget, appearance and privacy.
   Unsupported paths are labelled; no token-savings percentage is claimed.

In the complete mockup, **Hop thread** offers destination selection, seed
preview, exclusions and close/fork handling. V1 is seed-only.

## Preview shortcuts

Ctrl+E Learning · Ctrl+F sessions · Ctrl+K tasks · Ctrl+O Overview ·
Ctrl+T new-instance review. Escape closes the workspace dialog.
The complete mockup retains Ctrl+H for hop and its layout controls.
Final native bindings await verification.

The complete workspace retains its 900 px minimum width; Lite uses 760 px.
Feature dialogs and the companion scroll independently. The PNGs show the
workspace defaults, Learning in Lite and the global Overview.

## Scope and sources

The original full mockup contained a Git Diff tab and a full-editor action.
Removed on 2026-11-02. The user explicitly excludes this feature, including
future scope. The former ADR-0005 "come later" wording was an unsupported
inference, not evidence of a requested feature; the record now corrects it.

| Represented surface | Requirement and delivery boundary |
| --- | --- |
| Terminals, splitters, tab groups, profiles | FRD-0001 R09–R34, R42–R46; the first milestone covers these areas, not every later requirement in those ranges |
| Files navigator | FRD-0001 R35; later V1, not M0 |
| Learning, Aided Decision, quiz and memory | FRD-0012; staged companion delivery R1–R5 |
| Sessions, Overview, Daodan and roundtable | FRD-0006, FRD-0010, FRD-0008 and FRD-0007; later surfaces |
| Browser, routing and token settings | FRD-0011, FRD-0015, FRD-0013 and FRD-0016; capability-gated previews |

This mockup does not cover every accepted requirement. In particular, the
Markdown source/preview editor and document previews (FRD-0001 R36–R41) have
not been drawn. The Files sample represents the navigator, not those editors.
