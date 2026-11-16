# Verification evidence

Immutable candidate: `74fc2190e255a3e3cf7515091ff45ab86a288f6b`; prior candidate `4d418df7c23ee03a976f89349b74debdddee72c6`.

## Fresh review commands

| Command | Observed outcome |
|---|---|
| `git status --short`, `git rev-parse HEAD` | Initially clean, exact candidate above |
| `git merge-base --is-ancestor 4d418df7c 74fc2190e` | Exit 0; no rebase correspondence needed |
| `git diff --check 4d418df7c 74fc2190e` | Exit 0; no whitespace errors |
| `mise exec -- cargo fmt --all -- --check` (the `fmt:check` task's command) | Exit 0; no edits |
| `mise exec -- docker info --format '{{.ServerVersion}}'` | Exit 1; permission denied connecting to `/home/thorrester/.config/colima/default/docker.sock` |
| `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(/^pg_deferred_evidence::/)"'` | Exit 1 during Docker provisioning, before migrations/test selection. **Zero tests executed**, not a product test failure. Wrapper's existing rebuild/cleanup also denied Docker access. |

`mise run lints` **PASS**, exit 0: workspace `cargo clippy --locked --workspace --all-features --all-targets -- -D warnings` and shipped `cargo clippy --locked -p wyrd-server --bin wyrd-server -- -D warnings` both completed. No fresh runtime assertion is implied by Clippy.

## Source-inspected and carried-forward evidence

Normal routing is `mise.toml` `test:wyrd` → repository-managed Postgres → `test:wyrd:inner` → `scripts/run-family-tests.sh wyrd` → default `cargo nextest run --locked` for family packages. The four lifecycle tests now have no ignore; no nextest default filter excludes the module. This establishes routing from source, not successful runtime execution here.

Prior R1 remediation Evidence records nine deferred-evidence tests passing without `--run-ignored`, migrations, lint PASS and whitespace PASS. Those are implementation-recorded results, not fresh review runs. The Artifact rejection/valid-acceptance assertions and unchanged recovery assertions are available for source inspection. Delegated evidence row and overlapping-offer receipt have no new connected regression assertion; their Evidence rows explicitly claim source-only closure and describe unsuccessful fixture/interleaving attempts. The independent validator assesses those limitations against required proof rather than relabeling them green tests.

Prior R1 reports retain acceptance/source/assertion evidence for unaffected SDK/CLI/MCP/schema/direct/engine/lifecycle boundaries. No broad gate, full SDK journey, code generation or benchmark is rerun for this bounded remediation review. No production source, generated output, Git identity or refs may change.
