# C11.1 — child-only inherited environment plan

2026-11-02, Windows x64. Synthetic process qualification passed.
The adapter/engine/CLI regression run passed **41 tests**, including 31 adapter
tests (nine new environment unit tests and five integration tests). Clippy,
formatting and the normal frontend build passed.

The pure adapter module declares inherited-marker names for the four recorded
P20 adapters and removes their union by exact Windows name, without prefix
matching. The parent snapshot remains immutable; sibling plans are independent.
Home/configuration/provider settings pass through, with explicitly typed
Shinatama identity and terminal overrides applied to the child afterward.

Generic Codex sandbox names are removed only when `SBX_NONET_ACTIVE` was present,
including an empty value. `GIT_EDITOR` is removed only when it exactly equals a
helper command supplied by a separately qualified Claude/version profile. There
is no guessed helper-path substring rule. Conditional plans refuse a changed
parent snapshot instead of applying a stale decision. Values and token overrides
are redacted from Debug; there is no environment serialization for diagnostics.

Admission limits names/count/value bytes before copying. Malformed name/value
input is refused without partial changes. A name containing `=` could change its
meaning when encoded as `name=value`; Windows drive-current-directory names such
as `=C:` are retained explicitly. These distinctions follow
[Microsoft's CreateProcess environment contract](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw).
Two new regressions failed on accepted malformed `CLAUDECODE=bad` and inherited
NUL values, then passed after bounded validation; valid drive pseudo-names still
pass. No real credential/configuration file is read.

Qualification commands:

```powershell
cargo test --offline -p shinatama-adapter -p shinatama-engine -p shinatama-cli -j1 -- --test-threads=1
cargo clippy --offline -p shinatama-adapter -p shinatama-engine -p shinatama-cli --all-targets -j1 -- -D warnings
npm run build
```

The Windows process proof builds three synthetic snapshots (sandbox marker empty,
`0`, and absent with a user editor), then starts two independent children from
each. All six children are the test binary itself; none is an agent. Values are
checked internally and only names are printed. The driver has a five-second
deadline, an 8 KiB output cap for the known fixture, no visible console and
termination limited to its owned child. It changes neither the parent process
environment nor a sibling snapshot and reads no credentials/configuration.
All owned children exited; the proof does not depend on a prebuilt P14 binary.

The marker declarations retain their recorded partial Windows versions;
Hermes/DeepSeek lists and Pi's prepended PATH handling remain explicit gaps.
No native agent/version, interactive nested launch, login or private-home
capability is qualified by the synthetic tests. Full P20 and per-agent QSET
remain open; native execution still requires C09R admission.
