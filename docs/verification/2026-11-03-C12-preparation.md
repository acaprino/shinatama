# C12.1–C12.2 — pinned sidecar and private configuration

Static preparation passes for 609 allowlisted source/artifact/Zod files with exact
SHA-256 and size checks. The canonical source is unchanged; the current entry hash
is 0c0e71df9811dbc20050fc1e9887d6da8afcefc73ff78348f1a51bcd5e9cca3e.
It differs from the older note's fingerprint despite the same 0.1.207 label. No
configuration, credentials or user state was copied. Clean-build qualification is
still separate from verifying the available source/build receipt.

The installed x64 Node 24.14.1 executable is copied to ignored local runtime output
and pinned by SHA-256 58e74bf02fc5bbacc41dcb8bef089961cd5bddd37830b87784e4fc624d145d1f.
Preparation uses PE metadata without executing Node. The local installation lacks
the complete license notice: public distribution readiness remains false.

The engine verifies the pinned bundle and runtime on a background preparation
worker, constructs a child-only private environment and direct loopback CLI argv,
and prepares private directories/configuration with C01 grants. Configuration is
created only when absent. Existing content is preserved and remains semantically
unverified. No process, provider, global configuration or certificate-store probe
is invoked by these preparation APIs.

Six focused runtime/configuration tests pass, including environment bounds and
preserving an existing synthetic configuration. Static preparation and runtime-copy
scripts pass. Actual Node ABI, builtin SQLite, traffic, TLS, six storage indexes,
incremental CCR latency and supervision require the following contained integration
proof; none is inferred from preparation. SC16 and distribution gates remain open.
