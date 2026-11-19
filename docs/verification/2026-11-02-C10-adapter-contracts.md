# C10 — headless adapter contracts and read-only doctor

2026-11-02, Windows 11 x64 build 26200. Local qualification passed:
**27 tests** (17 adapter, 7 engine, 2 CLI unit and one real CLI integration),
Clippy with warnings denied, workspace/all-target checks and formatting.
No native agent was launched. The durable result is
[C10-adapter-contracts.result.json](2026-11-02-C10-adapter-contracts.result.json).

The adapter crate declares 30 operation groups, exact version/build identities,
fixture-bound capabilities, placement rules and bounded metadata/transcript/export
pages. Missing support refuses explicitly. The headless engine works with zero
adapters or a bounded in-memory fake. Native launch, login, update and post-V1
import/fork operations stay refused. No concrete native adapter is installed by
this increment.

Qualification evidence remains bound to its agent, exact version including
prerelease/build, and synthetic/native fixture kind. Registration and operation
admission both validate that binding, including a descriptor changed after
registration. The review's fake-to-native/rebound-build cases are regressions.

`shinatama doctor [--agent ID] [--json]` reports a versioned read-only result.
Missing or unqualified installations produce an explicit refused/degraded state
and exit 2; this is not a successful native launch qualification. The CLI neither
opens a state database nor creates configuration. The real CLI integration uses
synthetic executable/wrapper files and nonexistent home/data directories; it
verifies exit 2, unchanged files, no copied private wrapper content and no new
state. A separate run against the installed environment found one Claude Code
candidate and correctly kept it Degraded without a registered native adapter.

PATH discovery admits at most 32 directories and 128 candidates. Windows drive
and parent-reparse admission precede candidate file inspection. Wrappers remain
unresolved and unexecuted. Native file-version resources are read-only metadata,
with a 1 MiB resource ceiling and a finite inspection count; they never qualify
a CLI version or capability. Unknown/mismatched versions remain explicit.
Native OS metadata calls have no hard-preemption guarantee.

Plan interpretation: C10 supplies bounded probe-output parsing and refusal for
unknown versions. Actual native `--help`/`--version` probes run through the later
qualified C11/C09R launch path and per-agent QSET. Requiring those launches before
C10 completion would introduce a C10→C11→C09R prerequisite cycle. No native probe
or version is certified by a synthetic output fixture or PE resource. Connection
architecture must be chosen and qualified before each concrete adapter is enabled.
Mukade remains an injected catalog boundary until C14 qualifies the pinned API;
the contracts do not duplicate its native archive parser. Daodan stays deferred.

Both C10 units share their tested crate/module and CLI integration and are saved
as one runnable parent checkpoint. The prepared C11 environment source is neither
exported, tested nor included in this commit.

```powershell
cargo test --offline -p shinatama-adapter -p shinatama-engine -p shinatama-cli -j1 -- --test-threads=1
cargo clippy --offline -p shinatama-adapter -p shinatama-engine -p shinatama-cli --all-targets -j1 -- -D warnings
cargo check --offline --workspace --all-targets --features custom-protocol -j1
cargo build --offline -p shinatama-cli -j1
target/debug/shinatama.exe doctor --json
```

The existing C05 desktop executable/source is unchanged. The dependency inventory
still has 292 third-party packages; only nine dev-test role classifications were
added, with no version/license/text change. C09R resource/fault calibration,
production P14 containment and real native adapter/version gates remain open.
