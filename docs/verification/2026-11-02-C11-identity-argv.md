# C11.3 — immutable command arguments and launch identity

2026-11-02, Windows local qualification: **62 adapter/engine tests passed**,
including six argument-plan and seven identity tests. Clippy with warnings denied,
formatting, frontend build and the locked dependency inventory passed.

Command planning preserves individual OS arguments, accepts qualified native EXE
or explicit Node/script forms and never invokes a shell. Recipes bind the exact
agent/version/build, operation and evidence kind. Wrapper scripts, NUL, relative
paths, excessive argument counts/bytes and oversized Windows command lines refuse.
Node minimums belong to the qualified recipe, including Pi's explicit minimum.

Each identity uses OS randomness for its canonical instance ID and 256-bit token.
Only the child-environment override exposes the wire token. The serializable
record holds a domain-separated SHA-256 hash bound to instance and thread IDs;
verification uses constant-time digest comparison and rejects unknown, ended,
malformed or rebound records. Debug output redacts secrets and command contents.
The record is ready for persistence; this step creates no registry/table and
does not claim durable storage or native launch qualification.

Locked runtime dependencies reuse getrandom 0.4.3 and sha2 0.10.9 already in the
workspace graph, adding subtle 2.6.1 (BSD-3-Clause) for constant-time comparison.
The reviewed inventory contains 293 packages and no inventory errors. Seven
existing release-review entries remain open; this is not release legal approval.

```powershell
cargo test --offline -p shinatama-adapter -p shinatama-engine -j1 -- --test-threads=1
cargo clippy --offline -p shinatama-adapter -p shinatama-engine -p shinatama-cli --all-targets -j1 -- -D warnings
cargo fmt --all --check
python scripts/dependencies.py check --offline
npm run build
```

No process was started by these planners. Composition and common UI/CLI preflight
follow in C11.4; native execution remains gated by C09R. Independent UI changes
in the shared checkout are excluded from this backend commit.
