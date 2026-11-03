# Node runtime license bundle

The complete `node-v24.14.1/LICENSE` is copied **verbatim** from the official
Windows x64 Node 24.14.1 archive, not replaced by a short MIT declaration. It
includes Node's copyright and the publisher's bundled third-party terms.

Provenance was checked on 2026-11-03:

1. Fetch the official [release checksums](https://nodejs.org/dist/v24.14.1/SHASUMS256.txt)
   and [Windows archive](https://nodejs.org/dist/v24.14.1/node-v24.14.1-win-x64.zip)
   over HTTPS; validate the archive against the checksum list.
2. Hash the archive's exact `node-v24.14.1-win-x64/node.exe` entry and compare it
   with both the official `win-x64/node.exe` checksum and Shinatama's runtime pin.
   All three are `58e74bf02fc5bbacc41dcb8bef089961cd5bddd37830b87784e4fc624d145d1f`.
3. Copy only the exact full `node-v24.14.1-win-x64/LICENSE` entry. Its
   **159,872 bytes** have SHA-256
   `8efdacdc1cfa3460aeb7fe98e3c54337b971d5da70e6eee292b73b981acb220c`.

`provenance.json` records the archive and checksum fingerprints and the binary
binding. The official checksum list is retained alongside it. Detached release
signatures were **not** verified; the recorded trust source is the official HTTPS
release endpoint and its checksums.

`sidecar/prepare-runtime.ps1 -Mode Prepare` performs no network access or Node
execution. It verifies these pinned bytes, copies `LICENSE` beside the prepared
`node.exe`, and generates `runtime.identity.json`. `-Mode Verify` requires both
files and a matching receipt, and works without a Node installation on PATH or
at the original development source path. Existing executable or license bytes
that disagree with the pin are refused, never overwritten.

`distributionReady=true` refers **only to the presence and identity of this
complete publisher-provided Node license bundle** for the pinned executable.
It is not legal release approval, a runtime/ABI qualification, application-wide
dependency completeness, or permission to omit required terms from a package.
`legalReleaseApproval` remains false and the application's release review stays
open. The separate native runtime proofs establish execution compatibility.
