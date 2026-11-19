# Source-owned sidecar preparation

Run `pwsh -NoProfile -File sidecar/prepare.ps1` to verify the tree at
`sidecar/source-build/prepared` against the pinned lock, without running Node or
npm and without reading anything outside this directory. A changed, missing or
unknown file fails; the lock is never updated by the script.

`provenance.lock.json` records every permitted source, artifact and runtime Zod
file, byte count and SHA-256. The source lives in `prepared/src` and
`prepared/kernel`; `prepared/dist` is built from it with `node scripts/build.mjs`
(the build needs the development dependencies, which are not tracked, so it runs
in a scratch copy with `npm ci`). After a rebuild the lock's source and artifact
entries, the tree hashes, `entrySha256` and `prepared/bundle.identity.json` are
refreshed together.

On Windows `npm ci` links `acp-kernel` and `billion-context-pi-subagents` as
junctions, and esbuild then names their modules `kernel/dist/...` instead of
`node_modules/acp-kernel/dist/...`: the bundle differs from the pinned one with
no source change. After a first `node scripts/build.mjs` (it builds both
packages' `dist`), replace the two junctions with copies of `kernel` and
`pi-subagents` and build again: the unchanged tree then reproduces every pinned
artifact byte for byte, so a rebuild changes only what the source changed. The
build stamps `dist/build-info.json` with `MUSASHI_TERMINAL_BILI_SOURCE_FINGERPRINT`,
which must be the new source tree hash. The tree hash is SHA-256 over each entry,
in path order, as `path NUL sha256 NUL bytes LF`. Sidecar tests run with the pinned
Node in the scratch copy: `node --experimental-transform-types --test <file>`.

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
