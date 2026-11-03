# Source-owned sidecar preparation

Run `pwsh -NoProfile -File sidecar/prepare.ps1` to verify the pinned canonical
source and existing Musashi artifact. Add `-Mode Prepare` to create the static
bundle at `sidecar/source-build/prepared` without running Node or npm. Repeating
preparation verifies the existing output; changed inputs fail instead of updating
the lock or downloading replacements. An interrupted copy leaves one
`prepared.incomplete` directory for inspection.

`provenance.lock.json` records every permitted source, artifact and runtime Zod
file, byte count and SHA-256. The default source is the owned package in
`D:/Projects/horness-launcher/packages/billion-context`; the existing build comes
from Musashi's `sidecar/source-build`, prepared by its unmodified
`res/sidecar-prepare.mjs`. This route copies those validated static outputs; it
does not invoke Musashi's preparation script or read its live configuration.

The six schema-v9 index names and incremental CCR method are checked in both the
source and the pinned entry. The current entry fingerprint differs from the older
storage-note fingerprint, while the release label remains `0.1.207`. A version
label alone never satisfies this lock. No clean rebuild, private runtime setup,
certificate trust, supervision, provider coverage or latency qualification has
been performed here. **SC16 remains open.**

`pwsh -NoProfile -File sidecar/prepare-runtime.ps1 -Mode Prepare` copies the pinned
local Node executable into ignored `sidecar/runtime` after SHA-256, PE-machine and
file-version checks. Node is not invoked. It also verifies and copies the complete
official Node 24.14.1 `LICENSE` beside the executable; archive, executable, checksum
and license fingerprints are bound in `sidecar/runtime.lock.json` and
`sidecar/licenses/node-v24.14.1/provenance.json`. The receipt sets
`licenseBundleComplete=true` and `distributionReady=true` only for the complete
publisher-provided Node license bundle. This is not legal release approval or
runtime qualification; the ABI/SQLite execution proofs are separate. See
[the license provenance](../licenses/README.md).

Shinatama incorporates [billion-context](https://github.com/ranxianglei/billion-context)
and [acp-kernel](https://github.com/ranxianglei/acp-kernel) by ranxianglei.
Their original license and provenance notices remain in the copied
source. Zod retains its MIT license. Updates belong in the canonical source and
require a reviewed lock refresh; no request-path patch belongs in Shinatama.
