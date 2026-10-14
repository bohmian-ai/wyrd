//! A seeded support workload and the analytical queries Bifrost must answer
//! over it exactly as plain DataFusion does.
//!
//! Three related tables are modelled on a support desk:
//! - conversations, one row per answered question (a retried run appears
//!   twice);
//! - model calls, zero or more per run;
//! - verdicts, zero or more per run.
//!
//! The data includes nulls in every nullable column, duplicate keys, ties,
//! runs with no calls or no verdicts, and orphan calls and verdicts whose run
//! never appears.
//!
//! [`ANALYTICAL_QUERIES`] lists the questions an analyst asks of that desk.
//! [`SupportOracle`] answers each one over the identical rows from in-memory
//! tables, with no Bifrost involved. A journey writes [`SupportWorkload`]
//! through Bifrost in some data state (published, live, mixed, or live on a
//! remote Scribe), runs the same SQL, and requires an identical answer. A
//! mismatch then points at Bifrost's own layers (live fragments, pushdown,
//! distribution, publication) rather than at DataFusion's SQL semantics.
//!
//! The rows' run key is named `run_key` because `run_id` is a reserved
//! Bifrost correlation column.

use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanArray, Int64Array, RecordBatch, StringArray, TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use arrow::util::display::{ArrayFormatter, FormatOptions};
use datafusion::datasource::MemTable;
use datafusion::error::Result;
use datafusion::prelude::SessionContext;

/// Seed every journey generates its workload from.
pub const SUPPORT_WORKLOAD_SEED: u64 = 0x005E_ED0F_D35C;

/// Distinct runs the generator answers; retries and null keys come on top.
const RUNS: i64 = 300;

/// Every `RETRY_EVERY`th run was retried and appears twice in conversations.
const RETRY_EVERY: i64 = 37;

/// First run key of the orphan calls and verdicts no conversation owns.
const ORPHAN_KEY: i64 = 10_000;

/// First conversation's `created_at`, 2026-01-01T00:00:00Z, in microseconds.
const FIRST_CREATED_AT_US: i64 = 1_767_225_600_000_000;

/// The questions customers ask; few, so many conversations tie on one.
const QUESTIONS: [&str; 6] = [
    "where is my order",
    "refund status",
    "change delivery address",
    "missing item",
    "cancel subscription",
    "damaged item",
];

/// The model deployments a run may call.
const DEPLOYMENTS: [&str; 3] = ["haiku", "sonnet", "opus"];

/// Call outcomes, weighted towards success.
const OUTCOMES: [&str; 5] = ["ok", "ok", "ok", "error", "timeout"];

/// The verifiers that may judge a run.
const VERIFIERS: [&str; 3] = ["groundedness", "tone", "policy"];

/// Verdicts, weighted towards a pass.
const VERDICTS: [&str; 3] = ["pass", "pass", "fail"];

/// One of the three support tables, named the same way in every data state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportTable {
    /// One row per answered question.
    Conversations,
    /// Zero or more model calls per run.
    Calls,
    /// Zero or more verifier verdicts per run.
    Verdicts,
}

impl SupportTable {
    /// Every support table, in the order a writer registers them.
    pub const ALL: [Self; 3] = [Self::Conversations, Self::Calls, Self::Verdicts];

    /// The table's unqualified name, which a caller prefixes with its namespace.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Conversations => "conversations",
            Self::Calls => "calls",
            Self::Verdicts => "verdicts",
        }
    }

    /// The table's user columns. A writer registers these, and every batch
    /// [`SupportWorkload::batch`] builds carries exactly this schema.
    #[must_use]
    pub fn schema(self) -> SchemaRef {
        let key = Field::new("run_key", DataType::Int64, true);
        Arc::new(Schema::new(match self {
            Self::Conversations => vec![
                key,
                Field::new("question", DataType::Utf8, true),
                Field::new("delivered", DataType::Boolean, true),
                Field::new("latency_ms", DataType::Int64, true),
                Field::new(
                    "created_at",
                    DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                    false,
                ),
            ],
            Self::Calls => vec![
                key,
                Field::new("deployment", DataType::Utf8, true),
                Field::new("outcome", DataType::Utf8, false),
                Field::new("duration_ms", DataType::Int64, true),
            ],
            Self::Verdicts => vec![
                key,
                Field::new("verifier", DataType::Utf8, false),
                Field::new("verdict", DataType::Utf8, true),
            ],
        }))
    }
}

/// One conversations row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    /// The answered run; `None` for a conversation whose run was never recorded.
    pub run_key: Option<i64>,
    /// The customer's question, `None` when it was not captured.
    pub question: Option<&'static str>,
    /// Whether the answer reached the customer, `None` when unknown.
    pub delivered: Option<bool>,
    /// End-to-end latency, `None` when it was not measured.
    pub latency_ms: Option<i64>,
    /// When the conversation started, in microseconds since the epoch.
    pub created_at_us: i64,
}

/// One model calls row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The run that made the call.
    pub run_key: Option<i64>,
    /// The deployment called, `None` when the call never routed.
    pub deployment: Option<&'static str>,
    /// How the call ended.
    pub outcome: &'static str,
    /// How long the call took, `None` when it was not measured.
    pub duration_ms: Option<i64>,
}

/// One verdicts row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// The judged run.
    pub run_key: Option<i64>,
    /// The verifier that judged it.
    pub verifier: &'static str,
    /// The judgment, `None` when the verifier abstained.
    pub verdict: Option<&'static str>,
}

/// The deterministic rows of the three support tables.
///
/// The generator uses splitmix64, so the rows depend only on the seed, never
/// on a dependency's random-number algorithm.
#[derive(Debug, Clone)]
pub struct SupportWorkload {
    /// The conversations rows, in generation order.
    pub conversations: Vec<Conversation>,
    /// The model calls rows, in generation order.
    pub calls: Vec<Call>,
    /// The verdicts rows, in generation order.
    pub verdicts: Vec<Verdict>,
}

impl SupportWorkload {
    /// Generates the workload for `seed`.
    ///
    /// Runs `1..=RUNS` each get one conversation, and every `RETRY_EVERY`th
    /// run gets a second one. Each run then gets zero to four calls and zero
    /// to three verdicts. Two conversations, three calls, and two failing
    /// verdicts have a null key. Fifteen calls and ten verdicts belong to runs
    /// that have no conversation. Because a failing verdict has a null key,
    /// `NOT IN` over failing keys is empty, which is the SQL rule this data is
    /// built to check.
    #[must_use]
    pub fn generate(seed: u64) -> Self {
        let mut random = SplitMix64(seed);
        let mut workload = Self {
            conversations: Vec::new(),
            calls: Vec::new(),
            verdicts: Vec::new(),
        };
        for run in 1..=RUNS {
            let attempts = if run % RETRY_EVERY == 0 { 2 } else { 1 };
            for attempt in 0..attempts {
                workload
                    .conversations
                    .push(random.conversation(Some(run), run * 60 + attempt));
            }
            for _ in 0..random.below(5) {
                workload.calls.push(random.call(Some(run)));
            }
            for _ in 0..random.below(4) {
                workload.verdicts.push(random.verdict(Some(run)));
            }
        }
        for (offset, verifier) in [0, 1].into_iter().zip(VERIFIERS) {
            workload
                .conversations
                .push(random.conversation(None, (RUNS + offset) * 60));
            workload.verdicts.push(Verdict {
                run_key: None,
                verifier,
                verdict: Some("fail"),
            });
        }
        for offset in 0..15 {
            workload.calls.push(random.call(Some(ORPHAN_KEY + offset)));
        }
        for _ in 0..3 {
            workload.calls.push(random.call(None));
        }
        for offset in 0..10 {
            workload
                .verdicts
                .push(random.verdict(Some(ORPHAN_KEY + 5 + offset)));
        }
        workload
    }

    /// Builds `table`'s rows as one batch in [`SupportTable::schema`].
    ///
    /// # Errors
    ///
    /// Returns an Arrow error only if a column does not fit the schema, which
    /// would be a bug in this generator.
    pub fn batch(&self, table: SupportTable) -> Result<RecordBatch> {
        let columns: Vec<ArrayRef> = match table {
            SupportTable::Conversations => {
                let rows = &self.conversations;
                vec![
                    Arc::new(Int64Array::from_iter(rows.iter().map(|row| row.run_key))),
                    Arc::new(StringArray::from_iter(rows.iter().map(|row| row.question))),
                    Arc::new(BooleanArray::from_iter(
                        rows.iter().map(|row| row.delivered),
                    )),
                    Arc::new(Int64Array::from_iter(rows.iter().map(|row| row.latency_ms))),
                    Arc::new(
                        TimestampMicrosecondArray::from_iter_values(
                            rows.iter().map(|row| row.created_at_us),
                        )
                        .with_timezone("UTC"),
                    ),
                ]
            }
            SupportTable::Calls => {
                let rows = &self.calls;
                vec![
                    Arc::new(Int64Array::from_iter(rows.iter().map(|row| row.run_key))),
                    Arc::new(StringArray::from_iter(
                        rows.iter().map(|row| row.deployment),
                    )),
                    Arc::new(StringArray::from_iter_values(
                        rows.iter().map(|row| row.outcome),
                    )),
                    Arc::new(Int64Array::from_iter(
                        rows.iter().map(|row| row.duration_ms),
                    )),
                ]
            }
            SupportTable::Verdicts => {
                let rows = &self.verdicts;
                vec![
                    Arc::new(Int64Array::from_iter(rows.iter().map(|row| row.run_key))),
                    Arc::new(StringArray::from_iter_values(
                        rows.iter().map(|row| row.verifier),
                    )),
                    Arc::new(StringArray::from_iter(rows.iter().map(|row| row.verdict))),
                ]
            }
        };
        Ok(RecordBatch::try_new(table.schema(), columns)?)
    }

    /// Loads the identical rows into a fresh in-memory DataFusion session.
    ///
    /// # Errors
    ///
    /// Returns the batch or registration failure.
    pub fn oracle(&self) -> Result<SupportOracle> {
        let session = SessionContext::new();
        for table in SupportTable::ALL {
            let batch = self.batch(table)?;
            session.register_table(
                table.name(),
                Arc::new(MemTable::try_new(table.schema(), vec![vec![batch]])?),
            )?;
        }
        Ok(SupportOracle { session })
    }
}

/// Plain DataFusion over the support workload, the reference every Bifrost
/// data state must match.
pub struct SupportOracle {
    /// A session holding the three tables as unqualified in-memory tables.
    session: SessionContext,
}

impl SupportOracle {
    /// Runs `query` over the in-memory tables and returns its comparable rows.
    ///
    /// # Errors
    ///
    /// Returns the planning or execution failure.
    pub async fn answer(&self, query: &AnalyticalQuery) -> Result<Vec<Vec<String>>> {
        let batches = self.session.sql(&query.sql("")).await?.collect().await?;
        query.rows(&batches)
    }
}

/// One analytical question over the support tables.
#[derive(Debug, Clone, Copy)]
pub struct AnalyticalQuery {
    /// A stable name that identifies the query in a failure report.
    pub name: &'static str,
    /// The SQL, with `{conversations}`, `{calls}`, and `{verdicts}` standing
    /// for the three table names.
    template: &'static str,
    /// Whether the SQL fixes the row order, so results are compared in order.
    ordered: bool,
}

impl AnalyticalQuery {
    /// Renders the SQL for tables named `{prefix}conversations`,
    /// `{prefix}calls`, and `{prefix}verdicts`.
    #[must_use]
    pub fn sql(&self, prefix: &str) -> String {
        SupportTable::ALL
            .iter()
            .fold(self.template.to_owned(), |sql, table| {
                sql.replace(
                    &format!("{{{}}}", table.name()),
                    &format!("{prefix}{}", table.name()),
                )
            })
    }

    /// Renders `batches` as rows of display strings that can be compared
    /// across engines. Nulls render as `NULL`. An unordered query's rows are
    /// sorted, so two correct engines always agree.
    ///
    /// # Errors
    ///
    /// Returns the Arrow formatting failure.
    pub fn rows(&self, batches: &[RecordBatch]) -> Result<Vec<Vec<String>>> {
        let options = FormatOptions::default().with_null("NULL");
        let mut rows = Vec::new();
        for batch in batches {
            let formatters = batch
                .columns()
                .iter()
                .map(|column| ArrayFormatter::try_new(column.as_ref(), &options))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for row in 0..batch.num_rows() {
                rows.push(
                    formatters
                        .iter()
                        .map(|formatter| formatter.value(row).to_string())
                        .collect(),
                );
            }
        }
        if !self.ordered {
            rows.sort();
        }
        Ok(rows)
    }
}

/// The analytical questions every Bifrost data state must answer exactly as
/// [`SupportOracle`] does. Each one combines subqueries with joins.
///
/// Every ordered query sorts on all of its output columns. Rows that tie on
/// every sort key are therefore identical, so any correct order agrees.
pub const ANALYTICAL_QUERIES: [AnalyticalQuery; 10] = [
    AnalyticalQuery {
        name: "above_average_latency_with_failed_calls",
        template: "SELECT c.run_key, c.latency_ms FROM {conversations} c \
                   WHERE c.latency_ms > (SELECT AVG(latency_ms) FROM {conversations}) \
                   AND c.run_key IN (SELECT run_key FROM {calls} WHERE outcome <> 'ok') \
                   ORDER BY c.latency_ms DESC, c.run_key LIMIT 25",
        ordered: true,
    },
    AnalyticalQuery {
        name: "delivered_run_profile",
        template: "SELECT c.run_key, c.question, \
                   (SELECT COUNT(*) FROM {calls} m WHERE m.run_key = c.run_key) AS calls, \
                   (SELECT MAX(m.duration_ms) FROM {calls} m WHERE m.run_key = c.run_key) \
                   AS slowest_ms, v.verdicts \
                   FROM {conversations} c \
                   LEFT JOIN (SELECT run_key, COUNT(*) AS verdicts FROM {verdicts} \
                   GROUP BY run_key) v ON v.run_key = c.run_key \
                   WHERE c.delivered",
        ordered: false,
    },
    AnalyticalQuery {
        name: "fail_verdict_membership",
        template: "SELECT \
                   (SELECT COUNT(*) FROM {conversations} WHERE run_key IN \
                   (SELECT run_key FROM {verdicts} WHERE verdict = 'fail')) AS failed, \
                   (SELECT COUNT(*) FROM {conversations} WHERE run_key NOT IN \
                   (SELECT run_key FROM {verdicts} WHERE verdict = 'fail')) AS not_failed_with_null, \
                   (SELECT COUNT(*) FROM {conversations} WHERE run_key NOT IN \
                   (SELECT run_key FROM {verdicts} WHERE verdict = 'fail' \
                   AND run_key IS NOT NULL)) AS not_failed",
        ordered: false,
    },
    AnalyticalQuery {
        name: "judged_errors_without_a_fail",
        template: "SELECT c.run_key, c.question, m.deployment FROM {conversations} c \
                   JOIN {calls} m ON m.run_key = c.run_key AND m.outcome = 'error' \
                   WHERE EXISTS (SELECT 1 FROM {verdicts} v WHERE v.run_key = c.run_key) \
                   AND NOT EXISTS (SELECT 1 FROM {verdicts} v \
                   WHERE v.run_key = c.run_key AND v.verdict = 'fail')",
        ordered: false,
    },
    AnalyticalQuery {
        name: "deployment_spend_on_delivered_runs",
        template: "WITH per_run AS (SELECT run_key, COUNT(*) AS calls, \
                   SUM(duration_ms) AS total_ms FROM {calls} GROUP BY run_key) \
                   SELECT d.deployment, COUNT(*) AS runs, SUM(p.calls) AS calls, \
                   SUM(p.total_ms) AS total_ms \
                   FROM (SELECT DISTINCT run_key, deployment FROM {calls} \
                   WHERE deployment IS NOT NULL) d \
                   JOIN per_run p ON p.run_key = d.run_key \
                   WHERE d.run_key IN (SELECT run_key FROM {conversations} WHERE delivered) \
                   GROUP BY d.deployment",
        ordered: false,
    },
    AnalyticalQuery {
        name: "deployments_against_the_overall_average",
        template: "SELECT m.deployment, COUNT(*) AS calls, AVG(m.duration_ms) AS mean_ms \
                   FROM {calls} m JOIN {conversations} c ON c.run_key = m.run_key \
                   GROUP BY m.deployment \
                   HAVING AVG(m.duration_ms) > (SELECT AVG(duration_ms) FROM {calls}) \
                   OR COUNT(*) < (SELECT COUNT(*) / 10 FROM {calls})",
        ordered: false,
    },
    AnalyticalQuery {
        name: "verifier_outcomes_three_way",
        template: "SELECT v.verifier, m.outcome, COUNT(*) AS pairs, \
                   COUNT(DISTINCT c.run_key) AS runs \
                   FROM {conversations} c \
                   JOIN {calls} m ON m.run_key = c.run_key \
                   JOIN {verdicts} v ON v.run_key = c.run_key \
                   WHERE v.verdict IS NOT NULL \
                   AND c.latency_ms <= (SELECT MAX(latency_ms) FROM {conversations}) \
                   GROUP BY v.verifier, m.outcome",
        ordered: false,
    },
    AnalyticalQuery {
        name: "latency_rank_of_passed_runs",
        template: "SELECT delivered, run_key, latency_ms, latency_rank FROM ( \
                   SELECT delivered, run_key, latency_ms, \
                   RANK() OVER (PARTITION BY delivered ORDER BY latency_ms DESC NULLS LAST) \
                   AS latency_rank FROM {conversations} \
                   WHERE run_key IN (SELECT run_key FROM {verdicts} WHERE verdict = 'pass')) \
                   ranked WHERE latency_rank <= 5",
        ordered: false,
    },
    AnalyticalQuery {
        name: "slowest_calls_above_their_run_average",
        template: "SELECT m.run_key, m.deployment, m.duration_ms FROM {calls} m \
                   WHERE m.duration_ms > (SELECT AVG(m2.duration_ms) FROM {calls} m2 \
                   WHERE m2.run_key = m.run_key) \
                   AND m.run_key IN (SELECT run_key FROM {conversations} WHERE delivered) \
                   ORDER BY m.duration_ms DESC, m.run_key, m.deployment LIMIT 20",
        ordered: true,
    },
    AnalyticalQuery {
        name: "orphans_and_gaps",
        template: "SELECT \
                   (SELECT COUNT(*) FROM {calls} m WHERE NOT EXISTS \
                   (SELECT 1 FROM {conversations} c WHERE c.run_key = m.run_key)) \
                   AS orphan_calls, \
                   (SELECT COUNT(*) FROM {conversations} c LEFT JOIN {verdicts} v \
                   ON v.run_key = c.run_key WHERE v.run_key IS NULL) AS unjudged_conversations, \
                   (SELECT COUNT(*) FROM {conversations} c WHERE NOT EXISTS \
                   (SELECT 1 FROM {calls} m WHERE m.run_key = c.run_key)) \
                   AS conversations_without_calls",
        ordered: false,
    },
];

/// Deterministic splitmix64 source.
struct SplitMix64(u64);

impl SplitMix64 {
    /// Advances the state and returns the next 64 random bits.
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Returns a value in `0..bound`.
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    /// Returns `None` with probability `1/one_in`, else `value`.
    fn sometimes_null<T>(&mut self, one_in: u64, value: T) -> Option<T> {
        (self.below(one_in) != 0).then_some(value)
    }

    /// Picks one of `choices`.
    ///
    /// # Panics
    ///
    /// Never: the index is below `N`, which is a `usize`.
    fn pick<T: Copy, const N: usize>(&mut self, choices: [T; N]) -> T {
        let index = self.below(N as u64);
        choices[usize::try_from(index).expect("an index below N fits usize")]
    }

    /// Generates one conversation that started `minute` minutes after the
    /// first. Latencies are multiples of 50 ms, so many of them tie.
    fn conversation(&mut self, run_key: Option<i64>, minute: i64) -> Conversation {
        let question = self.pick(QUESTIONS);
        let delivered = self.below(5) != 0;
        let latency_ms = 50 * (1 + self.below(40).cast_signed());
        Conversation {
            run_key,
            question: self.sometimes_null(30, question),
            delivered: self.sometimes_null(20, delivered),
            latency_ms: self.sometimes_null(10, latency_ms),
            created_at_us: FIRST_CREATED_AT_US + minute * 60_000_000,
        }
    }

    /// Generates one model call. Durations are multiples of 100 ms.
    fn call(&mut self, run_key: Option<i64>) -> Call {
        let deployment = self.pick(DEPLOYMENTS);
        let duration_ms = 100 * (1 + self.below(30).cast_signed());
        Call {
            run_key,
            deployment: self.sometimes_null(20, deployment),
            outcome: self.pick(OUTCOMES),
            duration_ms: self.sometimes_null(12, duration_ms),
        }
    }

    /// Generates one verdict.
    fn verdict(&mut self, run_key: Option<i64>) -> Verdict {
        let verdict = self.pick(VERDICTS);
        Verdict {
            run_key,
            verifier: self.pick(VERIFIERS),
            verdict: self.sometimes_null(10, verdict),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Finds a query in [`ANALYTICAL_QUERIES`] by name.
    fn query(name: &str) -> &'static AnalyticalQuery {
        ANALYTICAL_QUERIES
            .iter()
            .find(|query| query.name == name)
            .expect("the named query is listed")
    }

    /// Checks the oracle against answers worked out without DataFusion.
    ///
    /// For two queries, the answer is computed with plain iterators over the
    /// generated rows, following SQL's three-valued rules. It is also pinned
    /// as literals, so a generator change shows up as a failed literal. The
    /// oracle must agree with both. If it did not, its agreement with Bifrost
    /// would prove nothing.
    #[tokio::test]
    async fn oracle_agrees_with_hand_counted_answers() {
        let workload = SupportWorkload::generate(SUPPORT_WORKLOAD_SEED);
        let oracle = workload.oracle().expect("oracle loads");
        let fail_keys: Vec<Option<i64>> = workload
            .verdicts
            .iter()
            .filter(|row| row.verdict == Some("fail"))
            .map(|row| row.run_key)
            .collect();
        assert!(fail_keys.contains(&None), "a failing verdict has no key");
        let failed = workload
            .conversations
            .iter()
            .filter(|row| row.run_key.is_some() && fail_keys.contains(&row.run_key))
            .count();
        let not_failed = workload
            .conversations
            .iter()
            .filter(|row| row.run_key.is_some() && !fail_keys.contains(&row.run_key))
            .count();
        let membership = vec![vec![
            failed.to_string(),
            "0".to_owned(),
            not_failed.to_string(),
        ]];
        assert_eq!(membership, [["106", "0", "202"]]);
        assert_eq!(
            oracle
                .answer(query("fail_verdict_membership"))
                .await
                .expect("membership answers"),
            membership
        );

        let matches = |left: Option<i64>, right: Option<i64>| left.is_some() && left == right;
        let orphan_calls = workload
            .calls
            .iter()
            .filter(|call| {
                !workload
                    .conversations
                    .iter()
                    .any(|row| matches(row.run_key, call.run_key))
            })
            .count();
        let unjudged = workload
            .conversations
            .iter()
            .filter(|row| {
                !workload
                    .verdicts
                    .iter()
                    .any(|verdict| matches(verdict.run_key, row.run_key))
            })
            .count();
        let without_calls = workload
            .conversations
            .iter()
            .filter(|row| {
                !workload
                    .calls
                    .iter()
                    .any(|call| matches(call.run_key, row.run_key))
            })
            .count();
        let gaps = vec![vec![
            orphan_calls.to_string(),
            unjudged.to_string(),
            without_calls.to_string(),
        ]];
        assert_eq!(gaps, [["18", "84", "78"]]);
        assert_eq!(
            oracle
                .answer(query("orphans_and_gaps"))
                .await
                .expect("gaps answer"),
            gaps
        );
    }

    /// Every listed query plans and runs on the oracle and returns rows, so
    /// no query compares two empty answers.
    #[tokio::test]
    async fn every_query_answers_with_rows() {
        let oracle = SupportWorkload::generate(SUPPORT_WORKLOAD_SEED)
            .oracle()
            .expect("oracle loads");
        for query in &ANALYTICAL_QUERIES {
            let rows = oracle.answer(query).await.expect(query.name);
            assert!(!rows.is_empty(), "{} returned no rows", query.name);
        }
    }
}
