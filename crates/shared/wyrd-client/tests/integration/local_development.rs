//! A fresh Rust process loads the gateway Agents an installed `wyrd get`
//! downloaded and runs each loaded runtime Agent on its deployment.
//!
//! The child of the Python installed-CLI journey
//! `test_installed_cli_gateway_setup`, which supplies the saved deployment and
//! the downloaded bundle; it is ignored everywhere else.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use skald_agent::{Agent, default_prompt_resolver, register_prompt_card};
use skald_tool::ToolRegistry;
use wyrd_client::bifrost::{Bifrost, BifrostOptions};
use wyrd_client::state::WyrdState;
use wyrd_client::{PublicWyrdGatewayCaller, WyrdClient};
use wyrd_spec::ExecutionRoute;
use wyrd_spec::vala::api::QueryParam;

/// `examples/gateway-setup/agents` alias (its Prompt provider) → deployment,
/// Prompt model, and the answer the journey's upstream fixture returns.
const GATEWAY_AGENTS: [(&str, &str, &str, &str); 4] = [
    ("openai", "openai-prod", "acme/gpt-4o-agent", "hi"),
    ("anthropic", "claude-prod", "claude-haiku-4-5", "hi"),
    ("google", "gemini-team", "tuned/gemini-2.5-pro", "hi"),
    (
        "jev",
        "judge-pool",
        "jev-judge",
        r#"{"helpful":{"type":"noul","noul":0.9}}"#,
    ),
];

/// One retained `vala.gateway.calls` row of a run, keyed by its `run_id`.
#[derive(Debug, PartialEq, Deserialize)]
struct Retained {
    /// The Run the call is correlated to.
    run_id: String,
    /// The requested deployment.
    deployment: String,
    /// The requested upstream model.
    model: String,
    /// The deployment that served the call.
    resolved_deployment: String,
    /// The Agent Card the call is attributed to.
    card_uid: Option<String>,
    /// The call's outcome.
    outcome: String,
}

/// Load each downloaded Agent with the public Agent loader, inspect what it
/// restored, attach only the public gateway caller, and run that same loaded
/// Agent; then read back the calls retained under each Run and Card.
///
/// The bundle's exact pinned Prompt is registered under its exact registered
/// ref so `Agent::from_card` resolves it; the route, identity, and Prompt
/// provider and model are the loader's alone, so a field it drops fails here.
///
/// # Panics
/// Panics when the saved deployment, bundle, loader, gateway call, or the
/// retained evidence does not match.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the installed CLI local-development fixture"]
async fn cli_gateway_agents() {
    let bundle = std::env::var("WYRD_LOCAL_DEV_AGENT_BUNDLE").expect("downloaded bundle path");
    let client = WyrdClient::from_global().expect("saved endpoint and credential");
    let state = WyrdState::from_path_with(bundle, client.clone()).expect("bundle loads");
    let mut runs = BTreeMap::new();
    for (alias, deployment, model, answer) in GATEWAY_AGENTS {
        let hydrated = state.agent(alias).expect("downloaded Agent");
        let prompt = skald_prompt::Prompt::from_native(hydrated.prompt().metadata.prompt.clone());
        register_prompt_card(hydrated.prompt_ref(), prompt).expect("pinned Prompt registers");
        let agent = Agent::from_card(
            hydrated.card().clone(),
            &ToolRegistry::new(),
            default_prompt_resolver(),
        )
        .expect("downloaded Agent loads");

        let ExecutionRoute::WyrdGateway { deployment: routed } = agent.execution_route() else {
            panic!("{alias}: loaded route {:?}", agent.execution_route());
        };
        assert_eq!(routed.as_str(), deployment, "{alias}");
        let card_ref = agent
            .card_ref()
            .expect("identity")
            .expect("loaded identity");
        assert_eq!(&card_ref, state.card_ref(alias).expect("alias"), "{alias}");
        assert_eq!(agent.id(), hydrated.card().name, "{alias}");
        assert_eq!(
            agent.to_card().expect("loaded card").spec,
            hydrated.card().spec,
            "{alias}"
        );
        let native = agent.prompt().native();
        assert_eq!(
            (native.provider().as_str(), native.model.as_str()),
            (alias, model)
        );

        let run = state.run_for_card(alias).expect("run opens");
        let caller = PublicWyrdGatewayCaller::new(client.clone())
            .with_subject(Some(run.run_id()), Some(run.subject_uid()));
        let agent = agent.with_gateway_caller(Arc::new(caller));
        let ran = agent
            .run_prompt(
                &skald_runtime::default_registry(),
                agent.prompt(),
                &[("question", "hi")],
            )
            .await
            .unwrap_or_else(|error| panic!("{alias} runs: {error}"));
        assert_eq!(ran.output, answer, "{alias}");
        runs.insert(
            run.run_id().as_str().to_owned(),
            Retained {
                run_id: run.run_id().as_str().to_owned(),
                deployment: deployment.to_owned(),
                model: model.to_owned(),
                resolved_deployment: deployment.to_owned(),
                card_uid: Some(run.subject_uid().to_string()),
                outcome: "succeeded".to_owned(),
            },
        );
    }

    let bifrost = Bifrost::connect_with(&client, BifrostOptions::default())
        .await
        .expect("Bifrost connects");
    let params: Vec<QueryParam> = runs.keys().cloned().map(QueryParam::String).collect();
    let query = "SELECT run_id, requested_model['deployment'] AS deployment, \
                 requested_model['model'] AS model, resolved_deployment, card_uid, outcome \
                 FROM vala.gateway.calls WHERE run_id IN ($1, $2, $3, $4)";
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let rows: Vec<Retained> = bifrost.sql_as(query, &params).await.expect("calls read");
        let retained: BTreeMap<_, _> = rows.into_iter().map(|r| (r.run_id.clone(), r)).collect();
        if retained == runs || Instant::now() > deadline {
            assert_eq!(retained, runs);
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
