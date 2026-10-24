//! Seeded evidence read through the real MCP catalog, schema, and SQL paths.

use std::path::Path;
use wyrd_client::bifrost::BifrostOptions;

use rmcp::ClientServiceExt as _;
use rmcp::model::CallToolRequestParams;
use rmcp::service::{RoleClient, RunningService};
use secrecy::ExposeSecret as _;
use serde_json::{Value, json};
use wyrd_client::Bifrost;
use wyrd_client::bifrost::{Correlation, TableConfig};
use wyrd_client::cards::Cards;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_runtime::Permission;
use wyrd_runtime::principal::PrincipalId;
use wyrd_server::scribe_outbox::ScribeWrite;
use wyrd_server::verification::results::{ResultPayloadBuilder, ResultRun};
use wyrd_spec::auth::Attribution;
use wyrd_spec::card::verifier::Check;
use wyrd_spec::ids::{ExecutionId, VerificationResultId};
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::GetCardResponse;
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::{
    RequirementDetail, RequirementOutcome, RequirementResult, VerificationOutcome,
};
use wyrd_spec::verification::{DriftWindow, VerificationVerdict};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::canonical_signals;

use crate::connectivity::{McpJourneyError, client, discover, problem, structured, transport};

/// A real reader with correlated telemetry, a declared dataset, and published judgments.
pub(super) struct EvidenceInvestigation {
    /// Owns retained rows and their production read boundary.
    server: WyrdTestServer,
    /// Authorized reader of every seeded table.
    agent: RunningService<RoleClient, ()>,
    /// Exact subject Card whose evidence is investigated.
    subject: CardRef,
    /// Drift result published for the subject, used as a detail pivot.
    drift_result: VerificationResultId,
    /// Assertion result published for the subject, used as a pivot.
    assertion_result: VerificationResultId,
    /// Exact Drift Verifier matching the retained feature report.
    drift_verifier: CardRef,
    /// Exact assertion Verifier from the shared `no-refund-promise` fixture.
    assertion_verifier: CardRef,
    /// Instrumentation scope isolating the canonical signals.
    scope: String,
}

impl EvidenceInvestigation {
    /// Seed shared telemetry and a checked-in Service; publish native fixture reports.
    ///
    /// # Errors
    /// Returns server, credential-exchange, registration, ingestion,
    /// publication, or MCP failures.
    pub(super) async fn start() -> Result<Self, McpJourneyError> {
        let server = WyrdTestServer::start_bound().await?;
        let signals =
            canonical_signals::seed_canonical_signals(&server, "mcp-investigation").await?;
        let admin = server
            .bootstrap_service("mcp-evidence-admin", &["admin"])
            .await?;
        let key = admin.api_key().ok_or("administrator has a machine key")?;
        let credential = ResolvedCredential::ApiKey(key.expose_secret().to_owned().into());
        let http = client(&server, credential)?;
        let cards = Cards::connect_with(&http).await?;
        let fixtures = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../fixtures/cards"
        ));
        for component in ["support-model.yaml", "support-agent.yaml"] {
            Box::pin(cards.declare_from_path(&fixtures.join("declare_and_pull").join(component)))
                .await?;
        }
        let receipt =
            Box::pin(cards.declare_from_path(&fixtures.join("mcp_investigation/service.yaml")))
                .await?;
        let drift = Box::pin(
            cards.declare_from_path(&fixtures.join("mcp_investigation/drift-verifier.yaml")),
        )
        .await?;
        let assertion = Box::pin(
            cards.declare_from_path(&fixtures.join("declare_and_pull/no-refund-promise.yaml")),
        )
        .await?;
        let bifrost = Bifrost::connect_with(&http, BifrostOptions::default()).await?;
        bifrost.use_table(
            TableConfig::describe_with(&http, "bifrost.datasets.investigation_events").await?,
        );
        for id in [1, 2] {
            bifrost.insert(
                serde_json::to_vec(&json!({ "id": id, "value": "observed" }))?,
                Correlation::default(),
            )?;
        }
        bifrost.flush().await?;
        bifrost.shutdown().await?;

        let agent = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::BearerToken(signals.token.into()),
                    None,
                )?,
                discover(),
            )
            .await?;
        let [drift_result, assertion_result] =
            Self::publish_results(&server, &receipt.root, &drift.root, &assertion.root).await?;
        Ok(Self {
            server,
            agent,
            subject: receipt.root,
            drift_verifier: drift.root,
            assertion_verifier: assertion.root,
            scope: signals.scope,
            drift_result,
            assertion_result,
        })
    }

    /// Reuse the native result mapper and shared Scribe outbox; no parallel row schema.
    ///
    /// Publishes one failed drift result from `drift_verifier` and one failed
    /// assertion result from `assertion_verifier` for `subject`, each under its own
    /// execution, and flushes them to Bifrost.
    ///
    /// Returns the drift and assertion result IDs the payloads minted, in
    /// that order.
    ///
    /// # Errors
    /// Returns fixture decoding, result mapping, or publication failures.
    async fn publish_results(
        server: &WyrdTestServer,
        subject: &CardRef,
        drift_verifier: &CardRef,
        assertion_verifier: &CardRef,
    ) -> Result<[VerificationResultId; 2], McpJourneyError> {
        let seed = server.verification_fixture().await?;
        let now = chrono::Utc::now();
        let failed = |id: &str, statement: &str, detail, check| -> Result<_, McpJourneyError> {
            Ok(VerificationOutcome {
                verdict: VerificationVerdict::Failed,
                requirements: vec![RequirementResult {
                    id: TaskId::new(id)?,
                    step: None,
                    statement: statement.to_owned(),
                    outcome: RequirementOutcome::Failed,
                    reason: "seeded failure".to_owned(),
                    actual: None,
                    citations: Vec::new(),
                    data: None,
                    detail,
                    check: serde_json::from_value::<Check>(check)?,
                    provenance: None,
                }],
                steps: Vec::new(),
                evidence: std::collections::BTreeMap::new(),
            })
        };
        let drift = failed(
            "latency-under-threshold",
            "Mean latency stays at or under its alert threshold",
            Some(RequirementDetail::Drift(serde_json::from_str(
                include_str!(
                    "../../../../../../../fixtures/cards/mcp_investigation/drift-report.json"
                ),
            )?)),
            json!({
                "kind": "drift", "on": "inputs.latency", "method": "Custom",
                "signal": {"kind": "Metric", "name": "latency"},
                "condition": {"kind": "Statistical"},
                "profile": {"kind": "Custom", "metric_name": "latency", "baseline_value": 1.0, "alert_threshold": 1.0}
            }),
        )?;
        let assertion = failed(
            "no-refund-promise",
            "The answer does not promise a guaranteed refund",
            None,
            json!({
                "kind": "assert", "path": "inputs.answer",
                "operator": "not_contains", "expected": "guaranteed refund"
            }),
        )?;
        let window = DriftWindow {
            start: now - chrono::Duration::hours(1),
            end: now,
        };
        let subject_uid = subject.uid.as_ref().ok_or("registered subject has a UID")?;
        let mut result_ids = Vec::with_capacity(2);
        for (execution_id, outcome, window, reference) in [
            (ExecutionId::new_v7(), drift, Some(&window), drift_verifier),
            (ExecutionId::new_v7(), assertion, None, assertion_verifier),
        ] {
            let version = reference.version.to_string();
            let uid = reference.uid.as_ref().ok_or("Verifier has a UID")?;
            let run = ResultRun {
                execution_id,
                run_id: None,
                verifier_version: &version,
                card_uid: subject_uid,
                owner_card_uid: Some(subject_uid),
                group_id: None,
                deployment_environment: None,
                binding_id: None,
                trigger: None,
                window,
            };
            let payload = ResultPayloadBuilder::new(run, uid, now, now).build(&outcome)?;
            result_ids.push(payload.result_id());
            server.state().bifrost.scribe_outbox().stage(
                server.data_tenant_id(),
                ScribeWrite::Result {
                    payload,
                    attribution: Attribution::direct(PrincipalId::new(seed.system_principal())),
                },
            );
        }
        server.flush_bifrost().await?;
        Ok(result_ids
            .try_into()
            .map_err(|_| "one result is published per seeded outcome")?)
    }

    /// Invoke a tool with its documented argument object.
    ///
    /// # Errors
    /// Returns malformed fixture arguments or MCP transport failures.
    fn request(name: &str, arguments: &Value) -> Result<CallToolRequestParams, McpJourneyError> {
        Ok(CallToolRequestParams::new(name.to_owned()).with_arguments(
            arguments
                .as_object()
                .ok_or("tool arguments are an object")?
                .clone(),
        ))
    }

    /// Read a complete bounded query result and require a successful terminal.
    ///
    /// # Errors
    /// Returns MCP, query, or terminal failures.
    async fn query(&self, sql: &str) -> Result<Value, McpJourneyError> {
        let arguments = json!({ "sql": sql, "max_rows": 20, "max_bytes": 65536 });
        let result = structured(
            self.agent
                .call_tool(Self::request("bifrost.query", &arguments)?)
                .await?,
        )?;
        assert_eq!(result["terminal"]["outcome"], "success");
        assert!(result["columns"].is_array());
        Ok(result["rows"].clone())
    }

    /// Discover and describe every evidence table before selecting its fields.
    ///
    /// # Errors
    /// Returns MCP or schema response failures.
    pub(super) async fn describe_evidence(&self) -> Result<(), McpJourneyError> {
        let listed = structured(
            self.agent
                .call_tool(Self::request("bifrost.list_tables", &json!({}))?)
                .await?,
        )?;
        let tables = listed["tables"]
            .as_array()
            .ok_or("table catalog is an array")?;
        for (namespace, name, field) in [
            ("bifrost.traces", "spans", "trace_id"),
            ("bifrost.logs", "records", "span_id"),
            ("bifrost.datasets", "investigation_events", "value"),
            ("bifrost.verification", "results", "verifier_card_uid"),
            ("bifrost.drift", "result_features", "score"),
        ] {
            assert!(
                tables
                    .iter()
                    .any(|table| table["namespace"] == namespace && table["name"] == name)
            );
            let description = structured(
                self.agent
                    .call_tool(Self::request(
                        "bifrost.describe_table",
                        &json!({ "namespace": namespace, "name": name }),
                    )?)
                    .await?,
            )?;
            assert!(
                description["user_fields"]
                    .as_array()
                    .ok_or("described fields")?
                    .iter()
                    .any(|entry| entry["name"] == field)
            );
        }
        Ok(())
    }

    /// A trace pivot reaches the correlated error log and failed span.
    ///
    /// # Errors
    /// Returns MCP or SQL failures.
    pub(super) async fn correlate_trace(&self) -> Result<(), McpJourneyError> {
        let trace_id =
            wyrd_spec::vala::ids::TraceId::from_bytes(canonical_signals::TRACE_ID)?.to_hex();
        let rows = self.query(&format!("SELECT l.severity_text, l.event_name, s.name FROM bifrost.logs.records l JOIN bifrost.traces.spans s ON l.trace_id = s.trace_id AND l.span_id = s.span_id WHERE s.trace_id = X'{trace_id}' AND l.scope_name = '{}' LIMIT 20", self.scope)).await?;
        assert_eq!(
            rows,
            json!([["ERROR", "tool.retry.exhausted", "execute_tool search"]])
        );
        Ok(())
    }

    /// A subject Card pivot selects judgments, whose result ID selects the Drift detail.
    ///
    /// # Errors
    /// Returns MCP, SQL, or missing-identity failures.
    pub(super) async fn follow_subject_and_results(&self) -> Result<(), McpJourneyError> {
        for reference in [&self.drift_verifier, &self.assertion_verifier] {
            let response: GetCardResponse = serde_json::from_value(structured(
                self.agent
                    .call_tool(Self::request(
                        "cards.get",
                        &json!({ "card_ref": reference }),
                    )?)
                    .await?,
            )?)?;
            assert_eq!(response.card.metadata.name, reference.name);
            assert_eq!(response.card.metadata.uid, reference.uid);
        }
        let uid = self.subject.uid.as_ref().ok_or("subject UID")?;
        let rows = self.query(&format!("SELECT result_id, verdict FROM bifrost.verification.results WHERE subject_card_uid = '{uid}' ORDER BY result_id LIMIT 20")).await?;
        let mut expected = [
            self.drift_result.to_string(),
            self.assertion_result.to_string(),
        ];
        expected.sort();
        assert_eq!(
            rows,
            json!(expected.map(|result_id| json!([result_id, "failed"])))
        );
        let rows = self.query(&format!("SELECT feature, score, threshold, verdict FROM bifrost.drift.result_features WHERE result_id = '{}' LIMIT 20", self.drift_result)).await?;
        assert_eq!(rows, json!([["latency", 3.0, 1.0, "drift"]]));
        Ok(())
    }

    /// Direct SQL reads a Service-declared table and distinguishes absent evidence.
    ///
    /// # Errors
    /// Returns MCP or SQL failures.
    pub(super) async fn read_custom_data(&self) -> Result<(), McpJourneyError> {
        assert_eq!(
            self.query(
                "SELECT id, value FROM bifrost.datasets.investigation_events ORDER BY id LIMIT 20"
            )
            .await?,
            json!([[1, "observed"], [2, "observed"]])
        );
        assert_eq!(self.query("SELECT result_id FROM bifrost.verification.results WHERE result_id = 'absent' LIMIT 20").await?, json!([]));
        Ok(())
    }

    /// Refusals never include partial rows, for permissions or either result ceiling.
    ///
    /// # Errors
    /// Returns principal, MCP, or unexpected-success failures.
    ///
    /// # Panics
    /// Panics when a `max_rows` or `max_bytes` refusal does not carry
    /// `WYRD_VALA_413_QUERY_RESULT_TOO_LARGE`, when the restricted reader's
    /// `bifrost.gateway.calls` read is not refused with status 403, or when any
    /// refusal includes `rows`.
    pub(super) async fn refuse_unauthorized_or_oversized_reads(
        &self,
    ) -> Result<(), McpJourneyError> {
        for arguments in [
            json!({ "sql": "SELECT id, value FROM bifrost.datasets.investigation_events", "max_rows": 1 }),
            json!({ "sql": "SELECT id, value FROM bifrost.datasets.investigation_events", "max_bytes": 1 }),
        ] {
            let failure = problem(
                self.agent
                    .call_tool(Self::request("bifrost.query", &arguments)?)
                    .await?,
            )?;
            assert_eq!(failure["code"], "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE");
            assert!(failure.get("rows").is_none());
        }
        self.server
            .seed_role("mcp_evidence_reader", &[Permission::bifrost_query_read()])
            .await?;
        let reader = self
            .server
            .bootstrap_service("evidence-reader", &["mcp_evidence_reader"])
            .await?;
        let token = self
            .server
            .exchange_api_key(reader.api_key().ok_or("reader key")?)
            .await?;
        let restricted = ()
            .serve_with_lifecycle(
                transport(
                    &self.server,
                    ResolvedCredential::BearerToken(token.into()),
                    None,
                )?,
                discover(),
            )
            .await?;
        let arguments =
            json!({ "sql": "SELECT request_payload FROM bifrost.gateway.calls LIMIT 20" });
        let failure = problem(
            restricted
                .call_tool(Self::request("bifrost.query", &arguments)?)
                .await?,
        )?;
        assert_eq!(failure["status"], 403);
        assert!(failure.get("rows").is_none());
        restricted.cancel().await?;
        Ok(())
    }

    /// Close the reader before draining Scribe and the server.
    ///
    /// # Errors
    /// Returns MCP or shutdown failures.
    pub(super) async fn shutdown(self) -> Result<(), McpJourneyError> {
        self.agent.cancel().await?;
        self.server.shutdown().await?;
        Ok(())
    }
}
