# SDK test audit — consolidated summary

Sources: `rust.md`, `typescript.md`, `python-integration.md`, `python-unit.md`
in this directory. Each test was asked two questions: (1) does it exercise
intended production behaviour, and (2) would a user understand it?

## Verdict totals

| Scope | Tests | KEEP | TIGHTEN | REWRITE | MOVE | DELETE |
|---|---:|---:|---:|---:|---:|---:|
| Rust | 116 | 46 | 10 | 7 | 26 | 27 |
| TypeScript | 63 | 9 | 26 | 13 | 5 | 10 |
| Python integration | 96 | 27 | 35 | 11 | 10 | 13 |
| Python unit | 461 | 183 | 132 | 70 | 12 | 64 |
| **Total** | **736** | **265 (36%)** | **203 (28%)** | **101 (14%)** | **53 (7%)** | **114 (15%)** |

About a third of the tests are fine as they stand. Another 28% test the right
thing but read badly. The remaining 36% exercise the wrong thing, belong in a
different tier, or should not exist.

## Why the tests read badly

The tests were written to tick off requirements, not to show how a user would
use the SDK. Wherever the SDK could not do something a test needed, the test
worked around the gap (with SQL, server hooks, CLI subprocesses, polling or
hand-built YAML) instead of exposing it. So most of what follows is a list of
**missing SDK capabilities**, not just a list of badly written tests.

## API gaps (merged across languages)

**A. Already closed by draft rev 64**

| Gap | Languages | Rev 64 |
|---|---|---|
| No local judgment: tests start a run and poll it, or call `Verification.execute` | all | REQ-188 `observe.verify` → `Judgment` |
| SDK `Verification` handle exists | all | REQ-189 removal |
| Artifact `sha256`/`size_bytes` computed by hand in tests | all | REQ-191 |
| Operator hook URL spliced into YAML at runtime | Py, TS | REQ-190 |
| Card YAML built in code | all | REQ-192 fixtures |

**B. Blocks a clean journey rewrite (candidates for rev 64)**

| Gap | Languages | A user should be able to write |
|---|---|---|
| B1. Rows written are not guaranteed to be readable afterwards. Tests use `flush_bifrost()`, 40× or 90 s loops, or retry on 429. | all | after `state.shutdown()` / `writer.flush()`, a read on the same client sees the acknowledged rows |
| B2. No way to wait for a Drift baseline to become ready. Tests poll Card status, or re-download the bundle in a loop. | all | `observe.verify` refuses with a typed `BASELINE_NOT_READY`, plus `cards.wait_ready(ref, timeout)` |
| B3. Python cannot read a Card's served `status`. It downloads through the CLI and parses YAML. | Py | `cards.get(ref).status` (TS already has this) |
| B4. No SDK download of a hydrated Service bundle. Every Python journey shells out to the `wyrd` CLI. | Py | `cards.download(service_ref, dest)` |
| B5. The run view's subject is an untyped string that tests split on `#`. The Python stub documents a different format. | Py, TS | `view.card_ref: CardRef` with a `uid` |
| B6. Errors are not typed. In Rust, the code lookup goes through `verification::WyrdError`, which is being removed. An async write refusal surfaces as `UpstreamFailure.details["original_code"]` on either `record` or `shutdown`, nondeterministically. Some TS refusals have no catalog code. | Rust, TS | one top-level SDK error with `.code`, and async refusals raised at one documented point (`shutdown`) |

**C. Real gaps that do not block rev 64 (separate change)**

| Gap | Languages |
|---|---|
| Emit is not aware of backpressure, so tests hand-roll QUEUE_FULL retry loops | all |
| No helpers for access tokens, principal ids, or OTLP exporter options; tests make raw `POST /auth/token`, decode JWTs, and set gRPC metadata by hand | all |
| Stored OTel attributes come back as protobuf bytes with no decoder | Py, TS |
| No typed rows for verification result history (`details` is a JSON string) | all |
| No SDK way to issue a Card-bound API key | Rust |
| Rust SQL is built with `format!` instead of parameters; an unwritten built-in table raises TABLE_NOT_FOUND instead of returning zero rows | Rust |
| Typed request constructors are missing (operator connections, `CardRef`); TS `TableConfig` takes positional optionals | Rust, TS |
| Provider credentials can only be set up through raw HTTP or the CLI. This is a harness gap: `startTestServer({ providerCredentials })` would fix it | all |
| SDK does not expose the trusted artifact-manifest hash, so tests reimplement blake3 | Py |
| Python authoring gaps: `DataCard`/`ModelCard.from_path` (40 tests read `card.json` by hand), typed splits and target columns, typed interface options, typed `agent.to_card()`, typed callback context, `prompt.response_schema`, a documented mock provider and canned responses, `WyrdConfig` readback, `ModelSignature` dimensions, typed query terminals, and `record` from a row model | Py |

## Cross-cutting problems

1. **Card YAML built in code.** All three SDKs do this, in about 60 tests. Fix: shared checked-in `fixtures/cards/<graph>/` used by every language (REQ-192).
2. **Several stories in one test.** About 50 tests, some covering 10–15 stories in 200+ lines. Fix: one story per test, sharing the server per file.
3. **Test-only server hooks and internal state.** About 50 tests use `verification_runs()` polling, `retire_fitted_format`, `table_describe_count`, `seed_bifrost_rows`, or direct SQL on `vala.audit_staging`. Fix: seed and read through the SDK. Move internal assertions to Rust server and Bifrost tests. The only clock control kept is `make_binding_due`.
4. **Engine statistics asserted from SDK journeys.** About 11 tests check NIST X-bar/S limits, PSI bins, or DataFusion semantics. Fix: move them to Vala and Oracle Rust tests. Journeys assert only the `Judgment`.
5. **CLI subprocess reads.** 16 Python journeys. Fixed by B3 and B4.
6. **Weak error assertions.** About 40 tests use a bare `raises(WyrdError)`, a message regex, or accept any of several codes, including a legacy `SKALD_` one. Fix: assert the single catalog code in every case.
7. **Helper sprawl.** `rejection` is redefined 7 times in TS. The token exchange appears 4 times in Python. There are three hand-rolled HTTP mock servers per language. Fix: one support module per SDK, using the native matcher (vitest `rejects.toMatchObject`, pytest `raises`) and one shared mock (wiremock is already a dependency).
8. **Tests of nothing.** About 50 tests:
   - name-bans for removed surfaces (AGENTS.md §12 says to delete these);
   - serde and config-default round-trips that the schema goldens already pin;
   - tests of their own test doubles;
   - assertions of a constant against itself;
   - TS type checks run as runtime tests (these should use `expectTypeOf`).
9. **Reliability, bench, and sink tests in the client e2e lane.** 9 Rust tests, some of which start a server they never use. Move them to the reliability lane or to a bench.
10. **Hygiene.**
    - The process environment is mutated without restore. Use `monkeypatch` / `vi.stubEnv`.
    - Python imports private extension modules in 6 places.
    - 11 `WyrdTestServer` hooks are missing from the `.pyi` stub.
    - Two Python files are run by no mise lane (`test_error_contract.py`, `examples/test_examples_smoke.py`).
    - One TS test imports the private `../../index.cjs`.

## Good examples to keep (models for the rewrite)

- Rust:
  - `typed_sql_projects_rows_and_refuses_a_mismatch`
  - `blocking_client_registers_writes_and_reads_back`
  - the wiremock Operator assertion in `drift_verification.rs`
- TypeScript: the admin, Gateway, OTel export, and Operator-connection journeys.
- Python:
  - the Bifrost client, Cards registry, offline `WyrdState`, and gateway client tests;
  - the prompt, media, settings, `CardRef`, workflow, hydration, and OTel unit tests;
  - `test_state_journey.py`, which already loads a checked-in graph.

## Target structure

```
fixtures/                         # shared by Rust, Python, TS (REQ-192)
  cards/<graph>/*.yaml            # static Cards; no digests, path-only Operator URLs
  baselines/*.parquet
  invalid/*.yaml                  # one deliberately broken Card per refusal
sdks/wyrd-sdk-<lang>/tests/
  support/                        # server lifecycle, connection, one HTTP receiver
  journeys/<story>.<ext>          # one file per user story, one story per test:
                                  #   register_and_hydrate, observe_a_run,
                                  #   verify_in_real_time, scheduled_drift_alerts_operator,
                                  #   query_bifrost, gateway_inference, operator_connections
  unit/                           # pure SDK logic and local refusals only
  types/                          # compile-time type tests (TS .test-d.ts, Python ty fixtures)
```

Moved out of the SDK lanes: engine statistics, cache and fence behaviour,
audit staging, legacy-format retirement, and the binding and run lifecycle. These
go to Rust server, Vala, and Bifrost tests, and to HTTP/MCP tests.
