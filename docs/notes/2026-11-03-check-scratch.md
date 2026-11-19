# Released Check scratch

Version probes and isolated TUI checks now create their temporary directories
through `check_scratch::create`. Only that in-memory ownership receipt can select
a directory for cleanup; previous runs' directories are never adopted. Native
cleanup uncertainty preserves both the owner and its scratch. Persistent agent
homes, credentials, normal instances and native archives are outside this cleanup.

Cleanup requires the actual Released result, including its persisted ledger
revision. Probe/process/C01 pins are dropped first. A successful TUI startup
receipt carries that same real release status. A closed canary without a startup
receipt keeps its finite scratch rather than inventing a release receipt.

The two focused Scratch tests passed: nine consecutive create/cleanup cycles
leave no probe/trial prefixes, and a junction is refused before any deletion while
its external synthetic sentinel stays intact. Limits remain 512 entries, 64 MiB,
depth 12 and three attempts. The sharing test uses a real directory LIST handle.

Directory pins in Claude probe/launch use `FILE_LIST_DIRECTORY` together with
`FILE_READ_ATTRIBUTES` and exclude DELETE sharing. Attribute-only handles do not
provide the required rename guard. The existing three local Probe tests passed,
including rename refusal while pinned and successful rename after release.

The AgentOnlySmoke source now verifies its synthetic C01 root, reads child names
only, requires a zero-prefix baseline and requires zero probe/trial prefixes after
a successful Check. The focused native app flow **passed** on 2026-11-03:
fresh baseline zero and post-Check zero, followed by original Claude TUI startup,
stop and application close. Log: `.verification/c15-scratch-live.log`.
The nine filesystem cycles are not a claim that nine native checks ran.
