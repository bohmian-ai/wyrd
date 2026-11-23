# Reference workload and measurement evidence

This is design evidence, not an executable fixture or a measured Wyrd result.
The supplied clone was read as untrusted data; none of its code was executed.

## Sources and provenance

Pinned repository: [openobserve/openobserve-clickhouse-benchmark at 289608a](https://github.com/openobserve/openobserve-clickhouse-benchmark/tree/289608a6f96e4c35783d2aafb680f0ba4dd406b8).
Article: [OpenObserve versus ClickHouse, one billion logs](https://openobserve.ai/blog/openobserve-vs-clickhouse-one-billion-logs-benchmark/).
The supplied article extraction agrees with the main published setup. Source SQL, generator, and index configuration take precedence over prose summaries.

| Source | SHA-256 of inspected bytes |
|---|---|
| `datagen/src/main.rs` | `59fd0ba767f3ad5d05a7cb019e036db40b5b4a490f98fbb31af6371cd71457e1` |
| `datagen/src/templates.rs` | `da13715f8747e575f5bc3a9baac11bb731b885016dcb0e24c0baff965c9d2d42` |
| `datagen/Cargo.lock` | `5f2775745b06f53fc9b7c1e21f7bb11d6ca113271e7923cb7ace18f0d7d890b9` |
| `schemas/clickhouse.sql` | `5898cbb31ced5904adfd41625b7e84b7e21c2764aa3584f3db5deb251f78d221` |
| `queries/queries.json` | `f14e95dd88619b3e6dfed06a2d64775e9433b4b0ac41ee7550121a5cfeac3ff4` |
| `scripts/start-openobserve.sh` | `bcdcd9368b8d93fd9361c55bbebcfd9c4b352b11af107d68ac9ea4b4442e573a` |
| `scripts/run-benchmark.py` | `de2f39809c33fff9efcac858f8edc2cfd4b082f50cbce7214e145214826ef055` |
| `scripts/measure-storage.sh` | `a8ef32bc9c53522a4054e26a40fb9ef23a5b85715e0253ac754876d1d9cafa72` |
| `RESULTS.md` | `78c8d88d28c61f51e278b6b7dbf6a6c133ab767c06df5c72ad86cd27d41a6e17` |
| `data.v2.js` | `9fca4cde0a2c34541aaaba6b3752938825e7ad5532537fe6e45df446b9271533` |

[reference-queries.json](reference-queries.json) is an unchanged copy of the
source manifest. Its eleven entries expand to nineteen cases: eight count
queries, three aggregations, and eight `*_rows` variants. Both original SQL
dialects are retained. [reference-results.json](reference-results.json) contains
the JSON data parsed from the first array in `data.v2.js`, without evaluating
JavaScript. It retains all five published samples, storage bytes and system
metadata, in the report order stated below.

## Exact case identities and predicates

Every case has the inclusive event-time bounds
`1780272000000000 <= _timestamp <= 1811808000000000` microseconds UTC.
OpenObserve supplies them as request bounds; ClickHouse spells them in SQL.
Do not replace these with a relative or narrower window.

The human Q labels differ from the source zero-based IDs. The report groups
counts, aggregations, then rows; the original driver actually executes
`q0, q0__rows, q1, q1__rows, …, q7, q7__rows, q8, q9, q10` serially.
Retain that execution order and retain this grouped display order.

| Case | Source ID | Predicate / operation |
|---|---|
| Q1 | `q0` | trace_id equality; exact SQL in manifest |
| Q2 | `q1` | span_id equality; exact SQL in manifest |
| Q3 | `q2` | rare whole message token; exact SQL in manifest |
| Q4 | `q3` | common whole message token; exact SQL in manifest |
| Q5 | `q4` | container AND trace_id; exact SQL in manifest |
| Q6 | `q5` | container AND rare token; exact SQL in manifest |
| Q7 | `q6` | common token AND rare token; exact SQL in manifest |
| Q8 | `q7` | pod-name equality; exact SQL in manifest |
| Q9 | `q8` | UTC one-hour histogram, count all rows; exact SQL in manifest |
| Q10 | `q9` | namespace count, descending count, LIMIT 10; exact SQL in manifest |
| Q11 | `q10` | common-token-filtered UTC one-hour histogram; exact SQL in manifest |
| Q12 | `q0__rows` | Q1 predicate; all 27 source columns, timestamp DESC, LIMIT 100 |
| Q13 | `q1__rows` | Q2 predicate; all 27 source columns, timestamp DESC, LIMIT 100 |
| Q14 | `q2__rows` | Q3 predicate; all 27 source columns, timestamp DESC, LIMIT 100 |
| Q15 | `q3__rows` | Q4 predicate; all 27 source columns, timestamp DESC, LIMIT 100 |
| Q16 | `q4__rows` | Q5 predicate; all 27 source columns, timestamp DESC, LIMIT 100 |
| Q17 | `q5__rows` | Q6 predicate; all 27 source columns, timestamp DESC, LIMIT 100 |
| Q18 | `q6__rows` | Q7 predicate; all 27 source columns, timestamp DESC, LIMIT 100 |
| Q19 | `q7__rows` | Q8 predicate; all 27 source columns, timestamp DESC, LIMIT 100 |

The literals are fixed, including zero-match cases at the smaller scale:

- trace: `299cb1bc60705f72a45c7d9f70a82156`;
- span: `7a4e90080fd2a013`;
- rare token: `474dbfe4c65a971176ae239869a6ba47`;
- common token: `failed`;
- container: `api-gateway-container`;
- pod: `api-gateway-9a2c1e6f4-8923f`.

Never sample replacement needles, inject matching rows, or expand the corpus
to force a nonempty answer. Report match counts and empty needles explicitly.

## Proposed lossless mapping of all 27 fields

Use one OTLP LogRecord per generated record and no duplicated full NDJSON body.
Resource/scope envelopes are empty; all non-core fields remain record attributes
under their original flat names. This avoids unrequested semantic-convention
renaming, cardinality changes or an extra copy of the message.

| Source field | OTLP input / canonical `bifrost.logs.records` field |
|---|---|
| `_timestamp` | `time_unix_nano = timestamp_us * 1000`; Timestamp(Nanosecond, UTC), checked multiply |
| `level` | `severity_text`, byte-preserved; severity_number TRACE=1, DEBUG=5, INFO=9, WARN=13, ERROR=17 |
| `message` | OTLP string body → `body_text`; canonical `body` is null |
| `host` | `attributes["host"]`, OTLP StringValue / Variant string (byte-preserved) |
| `stream` | `attributes["stream"]`, OTLP StringValue / Variant string (byte-preserved) |
| `kubernetes_namespace_name` | `attributes["kubernetes_namespace_name"]`, OTLP StringValue / Variant string (byte-preserved) |
| `kubernetes_pod_name` | `attributes["kubernetes_pod_name"]`, OTLP StringValue / Variant string (byte-preserved) |
| `kubernetes_container_name` | `attributes["kubernetes_container_name"]`, OTLP StringValue / Variant string (byte-preserved) |
| `kubernetes_node_name` | `attributes["kubernetes_node_name"]`, OTLP StringValue / Variant string (byte-preserved) |
| `kubernetes_labels_app` | `attributes["kubernetes_labels_app"]`, OTLP StringValue / Variant string (byte-preserved) |
| `kubernetes_labels_version` | `attributes["kubernetes_labels_version"]`, OTLP StringValue / Variant string (byte-preserved) |
| `kubernetes_pod_ip` | `attributes["kubernetes_pod_ip"]`, OTLP StringValue / Variant string (byte-preserved) |
| `trace_id` | hex-decode 32 characters to 16 bytes → `trace_id` FixedSizeBinary(16) |
| `span_id` | hex-decode 16 characters to 8 bytes → `span_id` FixedSizeBinary(8) |
| `request_id` | `attributes["request_id"]`, OTLP StringValue / Variant string (byte-preserved) |
| `http_method` | `attributes["http_method"]`, OTLP StringValue / Variant string (byte-preserved) |
| `http_path` | `attributes["http_path"]`, OTLP StringValue / Variant string (byte-preserved) |
| `http_status` | `attributes["http_status"]`, OTLP IntValue / Variant integer (lossless) |
| `http_latency_ms` | `attributes["http_latency_ms"]`, OTLP IntValue / Variant integer (lossless) |
| `http_bytes_out` | `attributes["http_bytes_out"]`, OTLP IntValue / Variant integer (lossless) |
| `client_ip` | `attributes["client_ip"]`, OTLP StringValue / Variant string (byte-preserved) |
| `ja3` | `attributes["ja3"]`, OTLP StringValue / Variant string (byte-preserved) |
| `user_id` | `attributes["user_id"]`, OTLP StringValue / Variant string (byte-preserved) |
| `session_id` | `attributes["session_id"]`, OTLP StringValue / Variant string (byte-preserved) |
| `region` | `attributes["region"]`, OTLP StringValue / Variant string (byte-preserved) |
| `zone` | `attributes["zone"]`, OTLP StringValue / Variant string (byte-preserved) |
| `cluster` | `attributes["cluster"]`, OTLP StringValue / Variant string (byte-preserved) |

`observed_time_unix_nano` equals the event timestamp (no new clock distribution);
flags/dropped counts are zero, event_name absent. Wyrd still writes its required
server-owned attribution and ingest columns; include their storage cost.
Generated pod IP strings are retained literally, even when an octet looks
unusual; do not normalize generator output.

The row-returning SQL explicitly reconstructs these 27 fields, with original
names and integer types: core time back to epoch microseconds, binary IDs to
lowercase hex, and the twenty-two remaining attribute values with typed casts.
Do not use canonical `SELECT *`, which would fetch a different schema, and do
not hide the canonical storage overhead in compression results.

## SQL translation contract

Keep original SQL beside each Wyrd SQL string in the eventual run record.
Translation changes dialect and field access only:

- Table: `bifrost.logs.records AS r`.
- Event bounds: `r.time_unix_nano BETWEEN to_timestamp_micros(1780272000000000) AND to_timestamp_micros(1811808000000000)`.
- trace/span comparisons use native binary literals, for example
  `r.trace_id = X'299cb1bc60705f72a45c7d9f70a82156'`; never hex-encode the column in the predicate.
- String attributes: `(r.attributes ->> 'kubernetes_container_name') = 'api-gateway-container'`, and the identical namespace/pod keys.
- Whole token `t`: `regexp_like(r.body_text, '(^|[^A-Za-z0-9])t([^A-Za-z0-9]|$)', 'i')`, with the literal token substituted unchanged.
- Q7 keeps two token predicates joined by AND, never OR or one alternation.
- Histogram: `date_trunc('hour', r.time_unix_nano)` in UTC, grouped and ordered ascending. Q10 groups namespace and orders count DESC LIMIT 10.
- Row variants use the same WHERE, all 27 reconstructed fields, and `ORDER BY r.time_unix_nano DESC LIMIT 100`. No new tie-breaker.

The reference ClickHouse tokenizer is ASCII alphanumeric, case-folded by its
index definition. Its [official tokenizer documentation](https://clickhouse.com/docs/reference/functions/regular-functions/splitting-merging-functions#tokens)
supports the proposed boundary regex. Its [hasAnyTokens documentation](https://clickhouse.com/docs/reference/functions/regular-functions/string-search-functions#hasanytokens)
also specifies that the index preprocessor applies to the input even during
filter evaluation; the source index uses `lower(message)` and lowercase needles.
[DataFusion documents regexp_like](https://datafusion.apache.org/user-guide/sql/scalar_functions.html#regexp-like);
the pinned Wyrd fork must still pass the translation tests before use.
This is an unindexed scan, not an invented equivalent full-text index.
The source OpenObserve Tantivy tokenizer need not agree for every possible
string; the benchmark uses these ASCII needles and generated messages.

Before a full run, the real Wyrd query surface must prove microsecond
round-trip through its timestamp-to-integer projection, binary literals,
Variant casts, UTC buckets, case-folding, punctuation boundaries, embedded
substrings, and empty results. An approximate substring LIKE is not acceptable.
If a function is unavailable, use the equivalent supported built-in expression
and prove it; do not add a benchmark-only SQL operator to production.

## Published Round 2 context

All context is 1,000,000,000 rows; Wyrd will be 500,000,000. The published
RESULTS.md calls its tables “hot medians”; the interactive five-sample data
reproduces its numbers with hot = min(sample 2, sample 3). Use that formula,
as requested, not the median of five or min of all five.

| Case | CH cold ms | CH hot ms | O2 Parquet cold ms | O2 Parquet hot ms | O2 Vortex cold ms | O2 Vortex hot ms |
|---|---:|---:|---:|---:|---:|---:|
| Q1 | 305.458 | 87.284 | 39.000 | 26.000 | 38.000 | 27.000 |
| Q2 | 284.265 | 80.455 | 30.000 | 25.000 | 29.000 | 26.000 |
| Q3 | 24.389 | 23.502 | 29.000 | 24.000 | 30.000 | 24.000 |
| Q4 | 1964.963 | 1890.709 | 28.000 | 25.000 | 28.000 | 27.000 |
| Q5 | 94.885 | 92.680 | 19.000 | 19.000 | 25.000 | 24.000 |
| Q6 | 28.998 | 29.200 | 30.000 | 28.000 | 36.000 | 32.000 |
| Q7 | 25.226 | 25.071 | 80.000 | 41.000 | 78.000 | 45.000 |
| Q8 | 4410.900 | 2949.211 | 29.000 | 27.000 | 29.000 | 27.000 |
| Q9 | 1590.672 | 1591.008 | 67.000 | 63.000 | 66.000 | 63.000 |
| Q10 | 1661.812 | 1639.061 | 1823.000 | 1500.000 | 1329.000 | 1204.000 |
| Q11 | 2253.213 | 2247.953 | 595.000 | 595.000 | 602.000 | 598.000 |
| Q12 | 227.249 | 189.597 | 118.000 | 77.000 | 77.000 | 44.000 |
| Q13 | 176.092 | 158.389 | 197.000 | 73.000 | 90.000 | 53.000 |
| Q14 | 86.349 | 85.567 | 68.000 | 66.000 | 52.000 | 52.000 |
| Q15 | 40.416 | 26.408 | 59.000 | 52.000 | 49.000 | 47.000 |
| Q16 | 197.196 | 187.235 | 66.000 | 56.000 | 53.000 | 32.000 |
| Q17 | 108.062 | 103.859 | 54.000 | 55.000 | 38.000 | 38.000 |
| Q18 | 93.618 | 91.637 | 90.000 | 81.000 | 67.000 | 67.000 |
| Q19 | 258.754 | 194.700 | 857.000 | 654.000 | 147.000 | 103.000 |

Class totals below are derived from the retained sample arrays, not additional
published benchmark measurements. CH published rounded total is 11,694 ms.

| Hot total / mean | ClickHouse ms | O2 Parquet ms | O2 Vortex ms |
|---|---:|---:|---:|
| Q1–Q8 counts | 5178.112 | 215.000 | 232.000 |
| Q9–Q11 aggregates | 5478.022 | 2158.000 | 1865.000 |
| Q12–Q19 row fetch | 1037.392 | 1114.000 | 436.000 |
| All 19 | 11693.526 | 3487.000 | 2533.000 |
| Geometric mean | 181.868 | 66.302 | 54.320 |

Published storage: ClickHouse 1,026.5 GB / 2.14× compression; O2 Parquet
673.5 GB / 3.26×; O2 Vortex 710.7 GB / 3.09×. Raw NDJSON: 2,199 GB.
The JSON keeps exact stored bytes; rounded publication values remain quoted
as published. Do not halve either storage or latency as a prediction of Wyrd.

Shared fan-out ingest: 16,666 seconds, 60,002 records/second, 128.1 MB/second.
The three published systems share that one measurement; no per-engine ingest
ranking can be derived. Preserve its published MB/s unit label; Wyrd reports
decimal MB/s and MiB/s with exact byte counters, rather than silently treating
the approximate published raw volume as an exact rate numerator.

## Cache-reset evidence conflict

RESULTS.md describes an external backend reset once immediately before the
full measurement pass. The supplied driver’s `run_query` calls OS reset before
each query variant only if co-located; on the separate reference driver it
cannot reset the backend. Its per-sample `backend.drop_cache()` clears engine
caches. Remote operator attestation is not proof of nineteen resets.

Recommended reproduction follows the published suite reset scope and records
`reset_scope=suite`, plus `cold_definition=first_run_per_case`; later first runs
may inherit OS pages from earlier cases. The portable proposal uses a process
restart only and labels storage cold unverified. A per-query reset experiment
would be a separate declared method, not an undocumented improvement.
