//! The benchmark tenant's Card graph and the arrivals the steps send.
//!
//! The tenant registers, as a tenant administrator would, an assertion
//! Verifier of four checks over one 2 KiB `context` slot, a Model, and a
//! Service binding the Verifier to the Model, once per closed-loop client.
//! Each client is its own application: it loads its pulled Service bundle
//! with its Service family runtime account's key, run as that Service root,
//! so every `verify` and `submit` carries its own authenticated owner, exactly as `WyrdState` does in production
//! when many applications share a tenant.
//!
//! L1 also registers the canonical Verifier ([`CANONICAL`]): one Assert and
//! two Agent checks, each Agent running one `code.run` between two model
//! exchanges through a local Gateway deployment. The deployment answers from
//! a local fixture provider ([`Conversations`]) that replies to each
//! conversation from its own messages, never from a shared queue, and keeps
//! a bounded per-arrival ledger the driver reconciles after the window. A
//! tenant Role granting `tool:invoke` on `code.run`, which no built-in Role
//! grants, is seeded for the runtime accounts as an administrator would. A
//! run that selects a model step also binds a judge Verifier whose one Agent
//! check makes one model call through the same deployment. No external
//! model or credential is involved; L2 keeps the assertion Verifier alone.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use axum::body::Bytes;
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::{Value, json};
use wyrd_client::cards::{BundleMode, CardGraphPuller, CardSelector, Cards};
use wyrd_client::config::ClientConfig;
use wyrd_client::state::WyrdState;
use wyrd_client::{Gateway, GlobalConfig, Principals, WyrdClient};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    CreateServicePrincipalRequest, WorkloadFamily, WorkloadKind, WorkloadRestriction,
};
use wyrd_testing::gateway_capture::GatewayCapture;
use wyrd_testing::release_server::{SERVER_URL, SetupTenant};

use crate::verification::Result;

/// The assertion Verifier's `metadata.name`, which every arrival names.
pub const ASSERT_VERIFIER: &str = "bench-assert";

/// The canonical L1 Verifier's `metadata.name`, which every L1 arrival names.
pub const CANONICAL_VERIFIER: &str = "bench-canonical";

/// The judge Verifier's `metadata.name`, which every model arrival names.
pub const JUDGE_VERIFIER: &str = "bench-judge";

/// The judge Verifier's one input slot, which its Verdict cites.
pub const JUDGE_SLOT: &str = "answer";

/// The local Gateway deployment the judge's Agent check routes through.
pub const JUDGE_DEPLOYMENT: &str = "bench-judge";

/// Bytes of padding that bring an assertion context to 2 KiB.
const PADDING: usize = 1_980;

/// The assertion Verifier: four `equals "yes"` checks over one `json` slot.
const ASSERT: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: bench-assert\n  version: 1.0.0\nspec:\n  inputs:\n    context: {type: json}\n  requirements:\n    - id: a\n      statement: a is yes.\n      check: {kind: assert, path: inputs.context.a, operator: equals, expected: \"yes\"}\n    - id: b\n      statement: b is yes.\n      check: {kind: assert, path: inputs.context.b, operator: equals, expected: \"yes\"}\n    - id: c\n      statement: c is yes.\n      check: {kind: assert, path: inputs.context.c, operator: equals, expected: \"yes\"}\n    - id: d\n      statement: d is yes.\n      check: {kind: assert, path: inputs.context.d, operator: equals, expected: \"yes\"}\n";

/// The judge Verifier: one Agent check over one `json` slot, routed through
/// [`JUDGE_DEPLOYMENT`] for one iteration, answering a plain cited Verdict.
const JUDGE: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: bench-judge\n  version: 1.0.0\nspec:\n  inputs:\n    answer: {type: json}\n  requirements:\n    - id: judged\n      statement: The answer is acceptable.\n      check:\n        kind: agent\n        inputs: {answer: inputs.answer}\n        agent:\n          execution_route: {kind: wyrd_gateway, deployment: bench-judge}\n          run_config: {max_iterations: 1}\n          prompt:\n            provider: openai\n            model: gpt-4o\n            messages:\n              - {role: user, content: \"Judge {{answer}}\"}\n            response_format: verdict\n";

/// The canonical L1 Verifier: one Assert and two independent Agent checks,
/// all eligible at once. Each Agent sees its own name and the arrival's
/// `sequence` in its prompt and the whole context as `/inputs/context.json`,
/// runs one `code.run` through [`JUDGE_DEPLOYMENT`], and answers a cited
/// Verdict whose data repeats what its program printed.
const CANONICAL: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: bench-canonical\n  version: 1.0.0\nspec:\n  inputs:\n    context: {type: json}\n  requirements:\n    - id: asserted\n      statement: d is yes.\n      check: {kind: assert, path: inputs.context.d, operator: equals, expected: \"yes\"}\n    - id: first\n      statement: The first Agent reads the arrival's own context.\n      check:\n        kind: agent\n        inputs: {sequence: inputs.context.sequence}\n        workspace: {/inputs/context.json: inputs.context}\n        agent:\n          execution_route: {kind: wyrd_gateway, deployment: bench-judge}\n          tool_names: [code.run]\n          run_config: {max_iterations: 2}\n          prompt:\n            provider: openai\n            model: gpt-4o\n            messages:\n              - {role: user, content: \"BENCH-AGENT first {{sequence}}\"}\n            response_format:\n              kind: verdict\n              data:\n                type: object\n                properties: {agent: {type: string}, sequence: {type: string}}\n                required: [agent, sequence]\n                additionalProperties: false\n    - id: second\n      statement: The second Agent reads the arrival's own context.\n      check:\n        kind: agent\n        inputs: {sequence: inputs.context.sequence}\n        workspace: {/inputs/context.json: inputs.context}\n        agent:\n          execution_route: {kind: wyrd_gateway, deployment: bench-judge}\n          tool_names: [code.run]\n          run_config: {max_iterations: 2}\n          prompt:\n            provider: openai\n            model: gpt-4o\n            messages:\n              - {role: user, content: \"BENCH-AGENT second {{sequence}}\"}\n            response_format:\n              kind: verdict\n              data:\n                type: object\n                properties: {agent: {type: string}, sequence: {type: string}}\n                required: [agent, sequence]\n                additionalProperties: false\n";

/// The Model's one registered artifact file: its bytes are never read, but
/// the Python SDK hydrates a Model only from a verified artifact directory.
const MODEL_ARTIFACT: &str = "bench-model\n";

/// The Model the Verifier judges, with one registered artifact,
/// [`MODEL_ARTIFACT`], so it is declared alone.
const MODEL: &str = "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: bench-model\n  version: 1.0.0\nspec:\n  interface:\n    kind: Custom\n    meta:\n      framework_version: 0.1.0\n      loader_module: fixture\n      loader_class: TinyModel\n      extra: {}\n  task_type: Other\n  signature:\n    inputs:\n      - name: f0\n        dtype: float64\n    outputs:\n      - name: score\n        dtype: float64\n  card_refs: []\nartifacts:\n  - relative_path: model.bin\n    content_type: application/octet-stream\n";

/// The Service binding the assertion Verifier to the already declared
/// Model by its exact identity; each application renames it
/// `bench-service-<index>`.
const SERVICE: &str = "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: bench-service\n  version: 1.0.0\nspec:\n  components:\n    - alias: model\n      ref: {kind: Model, name: bench-model, version: 1.0.0}\n      verified_by:\n        - verifier: ./assert.yaml\n";

/// The canonical Verifier's binding, appended to [`SERVICE`] for L1.
const CANONICAL_BINDING: &str = "        - verifier: ./canonical.yaml\n";

/// The judge's binding, appended to [`SERVICE`] when the run has model steps.
const JUDGE_BINDING: &str = "        - verifier: ./judge.yaml\n";

/// The tenant Role every L1 runtime account holds beside `workload`.
const CODE_ROLE: &str = "bench_code";

/// The two Agents of [`CANONICAL`], by the name each prompt carries.
pub const AGENTS: [&str; 2] = ["first", "second"];

/// The id of each canonical Agent's one `code.run` call.
const CALL_ID: &str = "call-code";

/// Most arrivals [`Conversations`] holds turns for until [`Conversations::take`].
const LEDGER_BOUND: usize = 1 << 21;

/// Problems [`Ledger::unreconciled`] names before it only counts.
const NAMED_PROBLEMS: usize = 8;

/// The Service every application renames: [`SERVICE`], binding the
/// canonical Verifier when `canonical` and the judge when `judge`.
fn service(canonical: bool, judge: bool) -> String {
    let mut service = SERVICE.to_owned();
    if canonical {
        service.push_str(CANONICAL_BINDING);
    }
    if judge {
        service.push_str(JUDGE_BINDING);
    }
    service
}

/// Whether arrival `sequence` answers `d` wrong: every odd one, so both
/// verdicts stay in every window.
pub const fn failing(sequence: u64) -> bool {
    sequence % 2 == 1
}

/// The 2 KiB context of arrival `sequence`.
pub fn context(sequence: u64) -> serde_json::Value {
    let d = if failing(sequence) { "no" } else { "yes" };
    serde_json::json!({
        "a": "yes", "b": "yes", "c": "yes", "d": d,
        "padding": "x".repeat(PADDING),
    })
}

/// The canonical context of arrival `sequence`: [`context`] with the
/// arrival's `sequence` as a string, still about 2 KiB.
pub fn canonical_context(sequence: u64) -> Value {
    let mut context = context(sequence);
    context["sequence"] = json!(sequence.to_string());
    context
}

/// The fixed program canonical Agent `agent` runs through `code.run`: it
/// reads the mounted context and prints the Agent and the arrival's
/// sequence as JSON.
fn program(agent: &str) -> String {
    format!(
        "import json\nfrom pathlib import Path\ncontext = json.loads(Path('/inputs/context.json').read_text())\nprint(json.dumps({{'agent': '{agent}', 'sequence': context['sequence']}}))"
    )
}

/// The canonical Agent and arrival a prompt `content` addresses, as
/// `BENCH-AGENT <agent> <sequence>`.
fn addressed(content: &str) -> Option<(usize, u64)> {
    let (agent, sequence) = content.strip_prefix("BENCH-AGENT ")?.split_once(' ')?;
    let agent = AGENTS.iter().position(|known| *known == agent)?;
    Some((agent, sequence.trim().trim_matches('"').parse().ok()?))
}

/// A buffered Chat Completions answer whose assistant message is `message`.
fn completion(message: Value) -> Bytes {
    let mut completion = GatewayCapture::completion();
    completion["choices"][0]["message"] = message;
    Bytes::from(completion.to_string())
}

/// What one canonical Agent's conversation asked the provider for one
/// arrival: its `code.run` turns and its final turns that carried exactly
/// its own program's output.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Turns {
    /// First turns, each answered with one `code.run` call.
    pub calls: u32,
    /// Final turns whose tool result was this conversation's own output.
    pub finals: u32,
}

/// The provider's per-arrival account of every canonical conversation.
#[derive(Debug, Default)]
pub struct Ledger {
    /// Each arrival's turns, by Agent in [`AGENTS`] order.
    turns: HashMap<u64, [Turns; 2]>,
    /// Final turns whose tool result was missing or not their own output.
    mismatched: u64,
    /// Whether an arrival found the ledger full and went unrecorded.
    overflowed: bool,
}

impl Ledger {
    /// Counts one turn of `agent` for arrival `sequence` with `turn`, or
    /// marks the ledger overflowed when it holds [`LEDGER_BOUND`] arrivals.
    fn count(&mut self, sequence: u64, agent: usize, turn: impl FnOnce(&mut Turns)) {
        if !self.turns.contains_key(&sequence) && self.turns.len() >= LEDGER_BOUND {
            self.overflowed = true;
            return;
        }
        turn(&mut self.turns.entry(sequence).or_default()[agent]);
    }

    /// How many of the `acknowledged` arrivals are not exactly one
    /// `code.run` turn and one own-output final turn per Agent, plus every
    /// mismatched final turn and an overflow, and, when `exhaustive` (no
    /// call was refused), every conversation of an arrival nobody
    /// acknowledged; with the first problems named.
    pub fn unreconciled(&self, acknowledged: &[u64], exhaustive: bool) -> (u64, Vec<String>) {
        let exact = [Turns {
            calls: 1,
            finals: 1,
        }; 2];
        let mut problems = Vec::new();
        let mut count = 0;
        let mut name = |count: &mut u64, problem: String| {
            *count += 1;
            if problems.len() < NAMED_PROBLEMS {
                problems.push(problem);
            }
        };
        if self.overflowed {
            name(
                &mut count,
                format!("the provider ledger reached its {LEDGER_BOUND}-arrival bound"),
            );
        }
        if self.mismatched > 0 {
            count += self.mismatched - 1;
            name(
                &mut count,
                format!("{} final turns carried no own code output", self.mismatched),
            );
        }
        let mut seen = HashSet::with_capacity(acknowledged.len());
        for sequence in acknowledged {
            if !seen.insert(*sequence) {
                name(
                    &mut count,
                    format!("arrival {sequence} was acknowledged twice"),
                );
            } else if self.turns.get(sequence) != Some(&exact) {
                name(
                    &mut count,
                    format!("arrival {sequence}: turns {:?}", self.turns.get(sequence)),
                );
            }
        }
        if exhaustive {
            for sequence in self
                .turns
                .keys()
                .filter(|sequence| !seen.contains(sequence))
            {
                name(
                    &mut count,
                    format!("arrival {sequence} conversed but was never acknowledged"),
                );
            }
        }
        (count, problems)
    }
}

/// The local provider's side of every model conversation an L1 run holds.
///
/// A canonical Agent's first turn is answered with one `code.run` of
/// [`program`]; a turn that already carries a tool result is answered with a
/// passing Verdict citing the arrival's `/sequence` in slot `context`, with
/// the printed output as its data, only when that result is exactly this
/// conversation's own output, else with a failed Verdict. Every other
/// prompt, a judge's, gets the judge's fixed passing Verdict citing
/// [`JUDGE_SLOT`].
pub struct Conversations {
    /// The turns of the current window.
    ledger: Mutex<Ledger>,
    /// The judge's answer, identical on every call.
    judged: Bytes,
}

impl Default for Conversations {
    fn default() -> Self {
        Self {
            ledger: Mutex::default(),
            judged: Bytes::from(GatewayCapture::verdict_completion(JUDGE_SLOT).to_string()),
        }
    }
}

impl Conversations {
    /// The answer to one Chat Completions request `body`, or `None` for a
    /// body that is not a chat request or names an unknown Agent.
    pub fn reply(&self, body: &[u8]) -> Option<Bytes> {
        let request: Value = serde_json::from_slice(body).ok()?;
        let messages = request["messages"].as_array()?;
        let Some((agent, sequence)) = messages
            .iter()
            .filter(|message| message["role"] == "user")
            .find_map(|message| addressed(message["content"].as_str()?))
        else {
            return Some(self.judged.clone());
        };
        let Some(result) = messages.iter().find(|message| message["role"] == "tool") else {
            self.ledger()
                .count(sequence, agent, |turns| turns.calls += 1);
            let call = json!({
                "id": CALL_ID,
                "type": "function",
                "function": {
                    "name": "code_run",
                    "arguments": json!({ "source": program(AGENTS[agent]) }).to_string(),
                },
            });
            return Some(completion(
                json!({ "role": "assistant", "content": null, "tool_calls": [call] }),
            ));
        };
        let expected = json!({ "agent": AGENTS[agent], "sequence": sequence.to_string() });
        let printed = result["content"]
            .as_str()
            .and_then(|content| serde_json::from_str::<Value>(content).ok())
            .and_then(|run| serde_json::from_str::<Value>(run["output"].as_str()?.trim()).ok());
        let own = result["tool_call_id"] == CALL_ID && printed.as_ref() == Some(&expected);
        let outcome = if own {
            self.ledger()
                .count(sequence, agent, |turns| turns.finals += 1);
            "passed"
        } else {
            self.ledger().mismatched += 1;
            "failed"
        };
        let verdict = json!({
            "outcome": outcome,
            "reason": "the Agent's program read this arrival's context",
            "citations": [{ "slot": "context", "locator": { "json_pointer": "/sequence" } }],
            "data": expected,
        });
        Some(completion(
            json!({ "role": "assistant", "content": verdict.to_string() }),
        ))
    }

    /// Removes and returns the window's ledger.
    pub fn take(&self) -> Ledger {
        std::mem::take(&mut *self.ledger())
    }

    /// The window's ledger, locked only for one count: every caller parses,
    /// validates, and builds its answer outside the lock, so concurrent
    /// provider calls serialize on the counter update alone.
    fn ledger(&self) -> MutexGuard<'_, Ledger> {
        self.ledger.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One provisioned tenant: its applications' states.
pub struct Tenant {
    /// One application per closed-loop client: its pulled Service bundle
    /// under that Service's own Card-bound key.
    pub states: Vec<WyrdState>,
    /// Each state's pulled bundle and key, in the same order, which a
    /// language driver loads through its own public SDK.
    pub applications: Vec<Application>,
}

/// One provisioned application as a language driver loads it: its pulled
/// Service bundle and that Service's runtime key.
pub struct Application {
    /// The pulled Service bundle directory.
    pub bundle: PathBuf,
    /// The Service family runtime account's key, never written to disk.
    pub key: SecretString,
}

impl Tenant {
    /// Declares the Card graph for `setup`'s tenant under `root`, the
    /// Model alone first, with `applications` Services, then provisions each application through
    /// [`Self::application`]. With a `provider` base URL (L1) it first
    /// points [`JUDGE_DEPLOYMENT`] at that provider, seeds [`CODE_ROLE`],
    /// and binds the canonical Verifier in every Service, and the judge too
    /// when `judge`.
    ///
    /// # Errors
    ///
    /// Returns a file, client, deployment, Role, declaration, key, or pull
    /// failure.
    pub async fn provision(
        setup: &SetupTenant,
        root: &Path,
        applications: usize,
        provider: Option<&str>,
        judge: bool,
    ) -> Result<Self> {
        let directory = root.join(&setup.slug);
        std::fs::create_dir_all(&directory)?;
        for (name, body) in [
            ("assert.yaml", ASSERT),
            ("model.yaml", MODEL),
            ("canonical.yaml", CANONICAL),
            ("judge.yaml", JUDGE),
            ("model.bin", MODEL_ARTIFACT),
        ] {
            std::fs::write(directory.join(name), body)?;
        }
        let admin = connect(&setup.api_key)?;
        let mut roles = vec!["workload".to_owned()];
        if let Some(provider) = provider {
            let deployment = serde_json::from_value(serde_json::json!({
                "name": JUDGE_DEPLOYMENT,
                "adapter": {"openai_compatible": {"base_url": format!("{provider}/v1")}},
                "auth": "none",
                "capabilities": ["chat_completions"],
            }))?;
            Gateway::connect_with(&admin)
                .await?
                .put_deployment(&deployment)
                .await?;
            seed_code_role(setup.tenant_id.parse()?).await?;
            roles.push(CODE_ROLE.to_owned());
        }
        let service = service(provider.is_some(), provider.is_some() && judge);
        let cards = Cards::connect_with(&admin).await?;
        // An artifact-bearing Card is declared alone; every Service then
        // references it by its exact identity.
        Box::pin(cards.declare_from_path(&directory.join("model.yaml"))).await?;
        let mut states = Vec::with_capacity(applications);
        let mut provisioned = Vec::with_capacity(applications);
        for index in 0..applications {
            let application =
                Self::application(&admin, &cards, &directory, &service, &roles, index).await?;
            states.push(WyrdState::from_path_with(
                &application.bundle,
                connect(&application.key)?,
            )?);
            provisioned.push(application);
        }
        Ok(Self {
            states,
            applications: provisioned,
        })
    }

    /// Declares Service `bench-service-<index>` from `service` beside the
    /// shared Verifiers and Model in `directory` (an identical redeclaration
    /// of those reuses them), creates its family runtime account holding
    /// `roles`, and pulls its bundle; a state loaded from it runs as the
    /// Service root.
    ///
    /// # Errors
    ///
    /// Returns a file, client, declaration, key, or pull failure.
    async fn application(
        admin: &WyrdClient,
        cards: &Cards,
        directory: &Path,
        service: &str,
        roles: &[String],
        index: usize,
    ) -> Result<Application> {
        let file = directory.join(format!("service-{index}.yaml"));
        let name = format!("name: bench-service-{index}\n");
        std::fs::write(&file, service.replacen("name: bench-service\n", &name, 1))?;
        let service = Box::pin(cards.declare_from_path(&file)).await?;
        let issued = Principals::connect_with(admin)
            .await?
            .create_service_principal(&CreateServicePrincipalRequest {
                name: format!("bench-service-{index}-runtime"),
                roles: roles.to_vec(),
                description: None,
                workload_restriction: Some(WorkloadRestriction::Family(WorkloadFamily {
                    kind: WorkloadKind::Service,
                    name: service.root.name.clone(),
                })),
            })
            .await
            .map_err(|error| format!("creating the Service account: {error}"))?;
        let bundle = directory.join(format!("bundle-{index}"));
        Box::pin(CardGraphPuller::new(cards.registry_context()).pull(
            &CardSelector::exact(service.root.clone()),
            &bundle,
            BundleMode::Complete,
        ))
        .await
        .map_err(|error| format!("pulling the Service bundle: {error}"))?;
        let key = SecretString::from(issued.credential.expose().to_owned());
        Ok(Application { bundle, key })
    }
}

/// Seeds [`CODE_ROLE`] in `tenant`, granting `tool:invoke` on `code.run`
/// alone, through repository Postgres: the server has no Role authoring
/// route, so a tenant administrator's custom Role is a row.
///
/// # Errors
///
/// Returns a missing DSN setting or a connection or SQL failure.
async fn seed_code_role(tenant: DataTenantId) -> Result<()> {
    let dsns = wyrd_sql::dsn::ResolvedDsns::from_env()?;
    let postgres = wyrd_sql::WyrdPostgres::connect_from_dsns(&dsns).await?;
    let mut conn = postgres.tenant_conn(tenant).await?;
    wyrd_sql::queries::auth::insert_role(
        &mut conn,
        uuid::Uuid::new_v4(),
        CODE_ROLE,
        &json!([{ "resource": "tool", "action": "invoke", "scope": { "tool": "code.run" } }]),
        false,
    )
    .await?;
    conn.commit().await?;
    Ok(())
}

/// A public client of the server presenting `credential`.
///
/// # Errors
///
/// Returns the client configuration failure.
fn connect(credential: &SecretString) -> Result<WyrdClient> {
    Ok(WyrdClient::with_config(ClientConfig {
        credential: Some(SecretString::from(credential.expose_secret().to_owned())),
        ..ClientConfig::from_global_with_overrides(&GlobalConfig::default(), Some(SERVER_URL), None)
    })?)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{ASSERT, JUDGE, JUDGE_DEPLOYMENT, JUDGE_SLOT, JUDGE_VERIFIER, failing, service};

    /// The JSON form of the Card YAML `card`.
    ///
    /// # Panics
    ///
    /// Panics when `card` is not a valid Card.
    fn card(card: &str) -> Value {
        serde_json::to_value(wyrd_spec::format::yaml::from_str(card).expect("a valid Card"))
            .expect("json")
    }

    /// The judge Verifier is one Agent check over its one slot, routed
    /// through the local deployment for one iteration and answering a plain
    /// Verdict; only a model run binds it beside the unchanged assertion
    /// Verifier.
    ///
    /// # Panics
    ///
    /// Panics when the judge or its binding drifts.
    #[test]
    fn model_fixture_declares_one_gateway_agent_check() {
        let judge = card(JUDGE);
        assert_eq!(judge["metadata"]["name"], JUDGE_VERIFIER);
        assert_eq!(
            judge["spec"]["inputs"]
                .as_object()
                .map(|inputs| inputs.keys().collect::<Vec<_>>()),
            Some(vec![&JUDGE_SLOT.to_owned()])
        );
        let requirements = judge["spec"]["requirements"]
            .as_array()
            .expect("requirements");
        assert_eq!(requirements.len(), 1);
        let check = &requirements[0]["check"];
        assert_eq!(check["kind"], "agent");
        assert_eq!(check["inputs"], json!({ JUDGE_SLOT: "inputs.answer" }));
        let agent = &check["agent"];
        assert_eq!(
            agent["execution_route"],
            json!({ "kind": "wyrd_gateway", "deployment": JUDGE_DEPLOYMENT })
        );
        assert_eq!(agent["run_config"]["max_iterations"], 1);
        assert_eq!(agent["prompt"]["response_format"], "verdict");
        assert_eq!(
            card(ASSERT)["spec"]["requirements"]
                .as_array()
                .map(Vec::len),
            Some(4)
        );
        let verifiers = |canonical: bool, judge: bool| {
            card(&service(canonical, judge))["spec"]["components"][0]["verified_by"]
                .as_array()
                .expect("bindings")
                .iter()
                .map(|binding| binding["verifier"].as_str().unwrap_or_default().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(verifiers(false, false), ["./assert.yaml"]);
        assert_eq!(
            verifiers(true, true),
            ["./assert.yaml", "./canonical.yaml", "./judge.yaml"]
        );
    }

    /// Every odd arrival fails and every even one passes.
    ///
    /// # Panics
    ///
    /// Panics when an arrival's failure is miscomputed.
    #[test]
    fn odd_arrivals_fail() {
        let failing: Vec<u64> = (0..6).filter(|s| failing(*s)).collect();
        assert_eq!(failing, vec![1, 3, 5]);
    }
}
