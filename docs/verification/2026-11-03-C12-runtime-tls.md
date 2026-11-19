# C12 — actual private runtime, storage and child-only TLS

Windows 11 x64 build 26200; pinned Node 24.14.1, modules ABI 137, built-in SQLite
3.51.2. The actual prepared CLI entry SHA-256 is
`0c0e71df9811dbc20050fc1e9887d6da8afcefc73ff78348f1a51bcd5e9cca3e`.
All processes run in admitted native jobs before execution, with private roots,
finite streams and exact cleanup proof. No provider, user configuration, account
or existing certificate is involved in the proof.

The actual CLI answers its exact owned health identity, reports version 0.1.207,
and shuts down. Its storage is schema 9 with all six hash indexes, integrity/FK
checks pass and the checked content-delete plans contain no table scan. Separately
rebuilt canonical source exercises 600 CCR entries and 200000 messages; incremental
saves finish in 51–53 ms over three repetitions, below the frozen 750 ms limit.
Health and CONNECT remain responsive during overlapping saves (latest maxima
68.198 ms and 73.906 ms). The source-level result is not described as a rebuilt
CLI result. Shared references, rollback, repeated migration and no-write updates
also pass in this single contained integration fixture.

Real local TLS proves both blind CONNECT and certificate interception: without
the synthetic CA they fail validation; with child-only NODE_EXTRA_CA_CERTS they
validate and receive the expected local HTTPS marker. Interception presents a
different leaf fingerprint, whereas blind CONNECT preserves the origin leaf.
TLS verification disabling is refused. Parent/owner TLS environment is unchanged;
no OS certificate installation occurs. Raw active process counts remain bounded
and all native jobs/ledger reservations are proven released.

Reproduction: `cargo test --offline -p shinatama-engine --test sidecar_runtime -j1
-- --ignored --test-threads=1 --nocapture`. Machine evidence is written to the
ignored `.prototype-evidence/sidecar-runtime.json`; the test refuses overwriting
an existing result. The maintained fixture source is committed with this record.

Windows Node rejected extended-length CLI paths with EISDIR. The runtime now
passes equivalent ordinary absolute DOS paths to Node, while retaining canonical
path pins and C01 grants. The upstream bundle was not patched.

Open: full SC16 soak, source-to-CLI rebuild qualification, effective compression
knobs beyond observed CCR/Absorb, and Node redistribution license notice. No full
SC16, native-agent certification or public-package readiness is inferred.
