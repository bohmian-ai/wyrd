# Focused follow-up — Observe canvas test and skipped file-size hook

**RESOLVED. No material finding proposed.**

Immutable subject: prior `dd98e9ef78d20e7de47e7af623644fed0ff9075a` to candidate `9c38fdfc3423a24ba450e82635528236a813af43`, source in `/tmp/wyrd-task001-review-9c38`. Original base `7b36f8fc6`; full context remains available. Read the review skill/scope, actual test delta, task's revised evidence, candidate metric URL/query/loader owners and applicable mandatory no-test-weakening rule. CodeGraph used before code location. No source, index or commits changed; no broad discovery restarted.

## Question and source-backed diagnosis

The extra `d8bfff7c2` change adds `apart=1` to the Observe Metrics canvas journey. Its fixture at `src/lib/ui-pages.integration.test.ts:629–639` deliberately records the same gauge `ui.canvas` from two services, with the same metric type/unit/temporality/monotonic meaning. The unchanged assertions at lines 766 onward require two legend rows and the first two series colors. This is a two-series visual scenario, not an assertion that the default mode must preserve two series.

Producer-to-consumer path (paths relative to `crates/wyrd/wyrd-server/wyrd-ui`):

1. `src/lib/observe-host.ts:118` parses URL `apart=1` to true; `listInputs` at lines 192–219 forwards `{ apart: true }` as the selected Metric loader input. With no flag and no group keys it sends neither, as the existing default mode specifies.
2. `packages/ui/src/lib/observe.ts:508–551` documents and implements default cross-series aggregation: `metricQueries` groups by recorded meaning and any explicit group keys. Without keys, both fixture services share one group; gauge values average, yielding one plotted legend row. With apart, the SQL includes `series` in group identity and retains the two recorded series independently.
3. `loadMetric` at lines 658–695 uses the same plan, then labels only plotted IDs when apart is true. Thus this switch is the existing production every-series path, not a fixture-only bypass or alternate result producer.
4. The user-facing calculation controls in `src/routes/t/[tenantKey]/observe/+layout.svelte:359–360,392–396` explicitly distinguish default merge from every-series mode. The added test query selects the same mode a user can select normally.
5. Existing `ui-pages.test.ts:210–260` separately asserts default aggregation SQL, apart identity and host-to-loader propagation. Neither those tests nor default production behavior changed in this candidate.

The earlier one-row-versus-two failure is therefore explained by a scenario-input mismatch: the test requested the default merged projection while asserting the intended separate-series canvas. Adding apart selects that required scenario. The diff does not change any assertion, timeout, retry, skip, service fixture, renderer, query function or production default. Decreasing the expected row count would have weakened the two-series visual proof; the actual correction preserves that proof.

The TASK-001 Notes and commit record diagnose this precise merge behavior. This report independently confirms it against the producer/query/consumer path and supplies the bounded diagnostician evidence required for a test-input correction. No additional implementation fix or reporting-only remediation is warranted. A full browser rerun is not claimed by this reviewer; parent verification and task-recorded results remain separately attributed.

## Development lifecycle evidence

The candidate Notes explicitly distinguish the full command's failed refused-setup phase from an isolated successful lifecycle run and label port contention only as a suspected, unproven cause. This candidate makes no lifecycle test/launcher change. That wording does not establish a green full integration run or a proven contention diagnosis, but it does not conceal or weaken a failed assertion. The existing implementation-audit scope continues to disclose this limit; no new source defect or correction is established by this unconfirmed hypothesis.

## Skipped pre-commit hook

Read original `.git/hooks/pre-commit`: it invokes `mise run check:file-size`. Candidate `mise.toml:1380–1389` checks added/modified staged blobs against 1,048,576 bytes, excluding named lockfiles. It is a byte-size guard, not a line-count/style rule. The hook and its task are unchanged.

A read-only Python/subprocess check enumerated the actual added/modified paths in `git diff --name-only --diff-filter=AM dd98e9ef7 9c38fdfc3`, applied the same basename exemptions, and measured each immutable candidate blob with `git cat-file -s <candidate>:<path>`. Results:

| Candidate file | Bytes | 1 MiB bound |
|---|---:|---|
| TASK-001 task document | 20,609 | PASS |
| brand/components.json | 26,955 | PASS |
| ui-pages.integration.test.ts | 84,377 | PASS |

All non-exempt added/modified blobs pass the hook's actual criterion. Running the hook with an empty index would prove nothing; no staging was done. The disclosed use of --no-verify omitted an execution step, but the independent equivalent check establishes that the reviewed deliverable does not violate its gate. No file splitting, compression, exception, hook change or new check is justified.

## Result and limits

The test change preserves the intended two-service visual scenario through an existing public mode, with a concrete source-backed diagnosis. It is a bounded repair of the red task gate, not an unrelated production feature or evidence weakening. The file-size criterion passes on committed blobs. No follow-up uncertainty remains in the assigned paths and no material finding is proposed.

Not rerun here: browser/integration journeys, native builds or visual acceptance. Risk remains the already disclosed full integration lifecycle failure and outstanding final visual acceptance; this report certifies neither.
