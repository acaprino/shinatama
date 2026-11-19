# C04 — project catalog and native Windows folder picker

Date: 2026-11-02. Windows 11 x64 build 26200; Node 24.14.1;
WebView2 156.0.4314.58. **Final local Windows qualification passed.**
The connected catalog and real native picker both passed serially on the rebuilt
normal desktop binary after fixing the configuration-loading race found by the
first connected UI test. Real network/DriveFS qualification remains open.
The machine-readable record is
[C04-project-catalog.result.json](2026-11-02-C04-project-catalog.result.json).

## Implemented behavior and bounds

Project roots and the explicitly selected default are stored in the typed
`projects.manager.v1` setting. First setup chooses its default explicitly;
there is no hardcoded drive or implicit replacement when that root disappears.
The root editor preserves unavailable configured paths and discards unsaved
drafts without changing selection. The legacy recent-folder interface remains
available before the initial default has been chosen.

The catalog lists first-level directories, without requiring `.git`, and uses
server-side filtering and paging. Previously opened projects precede unopened
folders; unopened folders use modification time. The frontend renders at most
100 project rows. Pages can be smaller to fit the response byte budget, so Next
uses the returned effective page size. The outside-root history holds at most
ten project references; pruning those references does not delete projects.

Compatibility decision: explicitly selecting an absolute folder through the
existing B02 “Use folder” action is treated like selecting it through F3 for
outside-root history. Both are voluntary project opens. This extends the FRD's
F3 wording while retaining the working B02 entry point. Scanning roots,
configuring roots and merely rendering candidates never populate that history.
The history remains outside-root only and is capped at ten references.

The backend admits at most 16 roots, one active scan per root, 1,000 candidates
and 4,000 examined entries per root. Cached candidate metadata has an 8 MiB total
and 2 MiB per-root budget; complete responses have a 256 KiB ceiling. Omitted
results and root failures are reported. A logical scan timeout cannot interrupt
a blocked OS filesystem call; a replacement worker is not spawned while that
call remains active. These limits are admission rules, not a measured C09R
process-memory or network-latency guarantee.

Configuration loading is separate from directory scanning. The root editor
and its Save action remain disabled until the database configuration has loaded,
without waiting for remote directory scans. Older asynchronous results cannot
lower the current configuration revision. The first GUI test found that the
initial cached revision could otherwise be edited before the authoritative
revision arrived, producing a stale-revision error on Save. The final connected
catalog repeat passed with the corrected loading/revision behavior.

Mutations reserve a request ID with synchronous `catalog_begin` before async
dispatch. Cancel pressed during reservation waits for its acknowledgement and
never dispatches Save in between. Cancellation and entering commit have one
atomic winner. A completed/committing save is not described as rolled back;
the dialog offers “Close; save continues”. Running cancelled workers keep their
slot until returning. These concurrency properties also have backend unit
coverage; the local GUI fixture does not simulate an indefinitely blocked UNC
call.

## Connected catalog proof

`apps/desktop/tests/catalog-smoke.mjs` creates an isolated OS-temporary home,
two project roots and 186 synthetic folders. It opens the actual embedded
desktop app through CDP port 19231. The test checks these groups:

1. Two roots and an explicit second default persist; 186 projects paginate into
   100 and 86 rows; prior usage wins over an old modification time.
2. Filtering resets the page offset, remains bounded and preserves keyboard
   focus. A pending filter cannot dispatch Next using an obsolete offset.
3. Cancelling the catalog and discarding a root draft preserve the selected
   project and the existing real PowerShell terminal session. `/` typed into
   xterm remains terminal input, and that shell produces its health marker.
4. After closing, the test renames only its own second root. Restart restores
   selection and reports the missing default without choosing another root or
   automatically starting a terminal.

The initial run failed at Save because of the configuration race above. The
final rerun passed all seven checks in 9.7 seconds with P14 wrapper exit 0 and
`wrapper in global job: true`. The screenshot was inspected: it shows only
synthetic projects and the retained unavailable default. The catalog driver's
PM6 note is a scope boundary, supplied by the separate native proof below.

## Real native F3 proof — final pass

`apps/desktop/tests/native-picker-smoke.mjs` uses an independent temporary home
and CDP port 19232. It presses the application's real F3 handler from the app
chrome, never xterm. `native-folder.ps1` finds only a visible `#32770` dialog
whose process ID matches the spawned app and whose owner chain reaches its
known main HWND. Every native action rechecks ownership. There are no global
keystrokes, clipboard use, fake picker results or production test commands.

The final run passed these five checks through P14 containment in approximately
11 seconds, wrapper exit 0 and `wrapper in global job: true`:

1. F3 opens the native dialog; targeted IDCANCEL returns to an enabled app
   dialog without an error or a changed selection/project history.
2. A second real picker, invoked through the existing scoped app command,
   returns exactly `{ "kind": "cancelled" }` when its native dialog is cancelled.
3. F3 selection saves the exact synthetic folder containing a space and `à`.
4. The selected path and stable project ID survive closing and restarting.
5. No terminal starts automatically; the custom frame close exits cleanly.

The native helper prefers a writable UI Automation ValuePattern under the
filename host. This host's PowerShell/.NET Win32 UIA proxy was unavailable:
Edit 1152 appeared as a Pane without that pattern. The successful run therefore
used the exact owned native Edit 1152, verified class/PID/parentage, then scoped
Unicode WM_SETTEXT and WM_GETTEXT. Readback occurs only after overwriting the
field with the synthetic path, and no control values or names are logged.
[Microsoft documents WM_GETTEXT for cross-process control text](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowtextw).
IDOK targets only the owned dialog. UI Automation support itself is not
classified as qualified by this fallback.

Windows may retain an invisible dialog HWND after completion. The proof
requires both disappearance of the native dialog and the completed real
app result/state; hiding a window alone cannot pass. The Tauri internal invoke
entry is immutable, so the final driver does not replace it with an observer.

The final native evidence is `.prototype-evidence/native-picker.json`, SHA-256
`E5FF28DEB97A91E66B467BD577729FDAD869A6EB4248F8DD50DCBED4BDB7FCE4`.
The final driver refuses an occupied CDP port and verifies that
the connected app's private data directory equals its unique synthetic home
before making UI changes. Both guards were included in the successful final
repeat on the corrected integrated binary. All owned app/helper processes
were closed before the maintenance integration proof received the GUI slot.

## Final evidence binding

The successful serial runs completed on 2026-11-02 before 19:54:16 UTC. SHA-256:

| Artifact | SHA-256 |
|---|---|
| `target/debug/shinatama-desktop.exe` | `3EBF467D5F4E5141D16B06DCE5E2FA632DB5576D641B578AFB19BDF6A61666C0` |
| `.prototype-evidence/project-catalog.json` | `239F6F3AC47A7B02539CB0E763897D0F93A63016532E80C2C54EB3FCD1D47DBD` |
| `.prototype-evidence/native-picker.json` | `E5FF28DEB97A91E66B467BD577729FDAD869A6EB4248F8DD50DCBED4BDB7FCE4` |
| `.prototype-evidence/project-roots.png` | `81200E57D52D5CC0A37C073D168CC8FD8F0C8B447546CACE82B8FFACF4AC4F07` |

The companion JSON also binds the three test-driver/helper source hashes.

## Reproduce and resume

Run on an interactive Windows desktop, after building the normal frontend and
desktop binary. Do not run another GUI test concurrently: focus would invalidate
the keyboard/idle observations.

```powershell
npm run build
cargo build -p shinatama-desktop --features custom-protocol -j1
verify/P14/target/debug/p14-probe.exe wrap-run node apps/desktop/tests/catalog-smoke.mjs
verify/P14/target/debug/p14-probe.exe wrap-run node apps/desktop/tests/native-picker-smoke.mjs
```

Both drivers refuse an occupied dedicated CDP port, use only synthetic temporary
data, and remove only their verified unique fixture prefixes. The native driver
also verifies the connected private directory before UI changes. Sanitized
results are written under ignored `.prototype-evidence`; the committed result
beside this document records the durable qualification status. The native test
is an explicit interactive-host check, not an automatically qualified hosted-CI
desktop test.

Real UNC, disconnected network shares, Google Drive/DriveFS latency, and actual
large remote directories have not been qualified by these local synthetic
folders. The global U3 keyboard matrix, accessibility at every scale, C09R
aggregate resource calibration and subprocess-fault resilience remain separate
gates. No model request or Daodan integration belongs to this proof.
