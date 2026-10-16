# Independent narrow verification

- PASS cargo check --locked -p wyrd-ai --test integration (mise exec); Rust nextest list integration --run-ignored=all completed.
- PASS exact wyrd-spec verifier_fixtures test fixture_verifiers_satisfy_the_staged_contract, 1 selected/1 passed.
- PASS cargo clippy --locked -p wyrd-client -p wyrd-ai -p wyrd-sdk-python -p wyrd-sdk-ts --all-features --lib -- -D warnings.
- PASS py:typecheck via mise with UV_CACHE_DIR=/tmp/wyrd-r1-uv-cache (original default cache read-only).
- PASS direct existing local tsc --noEmit via mise exec node (pnpm wrapper cache database unavailable); existing local Vitest list verifier-building-blocks succeeded.
- PASS Python pytest collect-only verifier_building_blocks, 19 collected, integration markers; direct existing venv via mise exec (uv default cache read-only).
- Exact 3 client wait/replay tests compiled and selected but all fail before behavior because wiremock cannot bind an OS port (sandbox Operation not permitted). Re-ran new deadline test with WYRD_LOG=info,wyrd_client=debug; same bind denial. This is an execution limit, no product assertion failure established.
- Cumulative git diff --check reports whitespace in committed historical review *.diff artifacts; no source whitespace issue established by that aggregate. Do not rewrite review provenance.

All commands/logs in this directory. No full journey execution or gate, per approved deferral. No production source changed.

Additional completed checks: cargo fmt --all --check PASS; py:lints (Ruff plus authored-stub validator) PASS with temporary UV cache; py:format:check PASS (168 files); direct installed Biome lint/format PASS (56 files each), invoked through mise exec node with canonical lane arguments; source-only cumulative git diff --check excluding changes/ provenance PASS. TS direct tsc returned exit 0.
