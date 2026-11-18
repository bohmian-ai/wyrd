//! GitHub delivery of `pull_request` executions: the connected tenant's
//! pull request is admitted and extracted as in
//! [`pg_github_pull_request_extraction`](super::pg_github_pull_request_extraction),
//! its Verifiers run on the queued runtime, and each execution's Check and
//! `COMMENT` review land on the local GitHub, rendered from retained
//! outputs and source, with one outcome in execution inspection.

use std::collections::BTreeMap;
use std::time::Duration;

use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};
use wyrd_testing::github::DeliveryFault;

use super::pg_github_pull_request_extraction::{
    Deployment, HANDLER_A, INSTALLATION, PATCH_A, PR, REPOSITORY, hold_scribe, sha,
};

/// The requirement every Verifier here is judged by: the change touches a
/// file.
const CHANGES: &str = "    - id: changes\n      statement: The reviewed change touches a file\n      check: {kind: assert, path: steps.pr.output.changes.files, operator: is_non_empty}\n";

/// An Agent whose Prompt answers in the standard `code_review` format.
const REVIEWER: &str = "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: reviewer\n  version: 1.0.0\nspec:\n  execution_route: {kind: wyrd_gateway, deployment: openai}\n  run_config: {max_iterations: 1}\n  prompt:\n    provider: openai\n    model: gpt-4o\n    response_format: code_review\n    messages:\n      - role: user\n        content: \"Review pull request {{pr}}.\"\n";

/// The Verifier `name`: extract the pull request, run `steps`, and judge
/// `requirements`.
fn verifier(name: &str, steps: &str, requirements: &str) -> String {
    format!(
        "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: {name}\n  version: 1.0.0\nspec:\n  description: {name} reviews the pull request.\n  steps:\n    - id: pr\n      action: {{kind: extract, extractor: pull_request}}\n{steps}  requirements:\n{requirements}"
    )
}

/// Code step `id` after the extraction, with step options `options`,
/// returning the Python expression `value` from `review(**inputs)`.
fn code(id: &str, options: &str, inputs: &[(&str, &str)], value: &str) -> String {
    let params: Vec<&str> = inputs.iter().map(|(name, _)| *name).collect();
    let bound: Vec<String> = inputs
        .iter()
        .map(|(name, reference)| format!("{name}: {reference}"))
        .collect();
    format!(
        "    - id: {id}\n      depends_on: [pr]\n{options}      action:\n        kind: code\n        entrypoint: review\n        inputs: {{{}}}\n        source: {{inline: \"def review({}):\\n    return {value}\\n\"}}\n",
        bound.join(", "),
        params.join(", ")
    )
}

/// A Python literal `code_review` report summarized by the Python
/// expression `summary`, with complete coverage and no findings.
fn report(summary: &str) -> String {
    format!(
        "{{'format': 'code_review', 'version': 1, 'summary': {summary}, 'coverage': {{'complete': True, 'limitations': []}}, 'findings': []}}"
    )
}

/// The Repository binding each of `verifiers` to pull requests.
fn policy(verifiers: &[&str]) -> String {
    let bound: Vec<String> = verifiers
        .iter()
        .map(|name| {
            format!("    - verifier: ./{name}.yaml\n      runs_on: {{kind: pull_request}}\n")
        })
        .collect();
    let bound = bound.concat();
    format!(
        "apiVersion: wyrd/v1\nkind: Repository\nmetadata:\n  name: policy\n  version: 1.0.0\nspec:\n  repository_id: \"4242\"\n  verified_by:\n{bound}"
    )
}

/// Start with `verifiers` (file stem, Card) bound to pull requests beside
/// the `extra` Cards they use, serving head `a…` of PR [`PR`].
///
/// # Panics
/// Panics when the deployment does not start or its Cards do not declare.
async fn bound(verifiers: &[(&str, String)], extra: &[(&str, &str)]) -> Deployment {
    let names: Vec<&str> = verifiers.iter().map(|(name, _)| *name).collect();
    let files: Vec<(String, String)> = verifiers
        .iter()
        .map(|(name, card)| (format!("{name}.yaml"), card.clone()))
        .chain(
            extra
                .iter()
                .map(|(file, card)| ((*file).to_owned(), (*card).to_owned())),
        )
        .chain([("policy.yaml".to_owned(), policy(&names))])
        .collect();
    let cards: Vec<(&str, &str)> = files
        .iter()
        .map(|(file, card)| (file.as_str(), card.as_str()))
        .collect();
    let deployment = Deployment::start(&cards).await;
    deployment.head(&sha('a'), &sha('8'), HANDLER_A);
    deployment.compare(&sha('c'), &sha('a'), &sha('c'), HANDLER_A, PATCH_A);
    deployment
}

/// One execution's Check and the inspection serving its delivery outcome.
struct Delivered {
    /// The Check as GitHub holds it.
    check: Value,
    /// `GET /v1/verification/executions/{id}` of its execution.
    served: Value,
}

/// Every Check on the repository, by name, once `count` are completed and
/// each execution's outcome for its completed state is in inspection.
///
/// # Panics
/// Panics when that does not happen in two minutes.
async fn delivered(deployment: &Deployment, count: usize) -> BTreeMap<String, Delivered> {
    let deadline = tokio::time::Instant::now() + Duration::from_mins(2);
    loop {
        let checks = deployment.mock.check_runs(REPOSITORY);
        if checks.len() == count && checks.iter().all(|check| check["status"] == "completed") {
            let mut found = BTreeMap::new();
            for check in &checks {
                let served = deployment
                    .get(&format!(
                        "/v1/verification/executions/{}",
                        check["external_id"].as_str().expect("external id")
                    ))
                    .await;
                if served["status"] != "completed"
                    || served["github_delivery"]["status"] == "pending"
                {
                    break;
                }
                let name = check["name"].as_str().expect("name").to_owned();
                found.insert(
                    name,
                    Delivered {
                        check: check.clone(),
                        served,
                    },
                );
            }
            if found.len() == count {
                return found;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{count} Checks were never all delivered: {checks:#?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The reviews on PR [`PR`] carrying `execution`'s marker.
fn reviews_of(deployment: &Deployment, execution: &str) -> Vec<Value> {
    let marker = format!("<!-- wyrd:execution:{execution} -->");
    deployment
        .mock
        .reviews(REPOSITORY, PR)
        .into_iter()
        .filter(|review| {
            review["body"]
                .as_str()
                .is_some_and(|body| body.contains(&marker))
        })
        .collect()
}

/// The reviewer's model answers every review with `report`.
async fn answers_with(deployment: &Deployment, report: &Value) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("Review pull request"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-review", "object": "chat.completion", "created": 1, "model": "gpt-4o",
            "choices": [{"index": 0, "finish_reason": "stop",
                         "message": {"role": "assistant", "content": report.to_string()}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
        })))
        .mount(&deployment.model)
        .await;
}

/// An Agent's `code_review` report becomes the execution's one Check and
/// one `COMMENT` review on the reviewed commit: a finding inside the diff
/// is an inline comment carrying its exact suggestion, one without a
/// location stays in the summary, one in a selected unchanged context span
/// stays in the summary as valid, and anchors the retained source does not
/// hold (a file it never read, another commit, lines past the end) are
/// refused by name, never moved. An empty report is still reviewed. Every
/// severity is shown. The authored requirement alone decides the verdict:
/// a critical finding does not fail it, and an empty report with
/// incomplete coverage does not pass it.
///
/// # Panics
/// Panics when a Check, review or outcome differs.
#[tokio::test(flavor = "multi_thread")]
async fn a_code_review_report_becomes_one_check_and_one_comment_review() {
    let at = |path: &str, commit: &str, start: u32, end: u32| json!({"path": path, "commit": commit, "start_line": start, "end_line": end});
    let head = sha('a');
    let fix = "export const handle = (id: string) => lookupUser(id.trim().toLowerCase());\n";
    let finding = |id: &str, severity: &str, locations: Value, remediation: Value| {
        json!({"id": id, "severity": severity, "confidence": "high", "title": format!("Finding {id}"),
               "explanation": format!("Explained {id}."), "locations": locations, "evidence": [],
               "remediation": remediation})
    };
    let agent_report = json!({
        "format": "code_review", "version": 1, "summary": "The handler normalizes ids.",
        "coverage": {"complete": true, "limitations": []},
        "findings": [
            finding("F-1", "critical", json!([at("src/handler.ts", &head, 3, 3)]),
                    json!({"explanation": "Lower-case it too.",
                           "suggested_edit": {"location": at("src/handler.ts", &head, 3, 3), "replacement": fix}})),
            finding("F-2", "high", json!([at("src/user.ts", &head, 2, 2)]), Value::Null),
            finding("F-3", "medium", json!([]), Value::Null),
            finding("F-4", "low", json!([at("src/handler.ts", &head, 3, 3)]),
                    json!({"explanation": "Edit another commit.",
                           "suggested_edit": {"location": at("src/handler.ts", &sha('b'), 3, 3), "replacement": fix}})),
            finding("F-5", "low", json!([at("src/handler.ts", &head, 40, 41)]), Value::Null),
            finding("F-6", "low", json!([at("src/missing.ts", &head, 1, 1)]), Value::Null),
        ],
    });
    let agent = verifier(
        "agent-review",
        "    - id: review\n      depends_on: [pr]\n      action:\n        kind: agent\n        agent: ./reviewer.yaml\n        inputs: {pr: steps.pr.output.pr}\n",
        CHANGES,
    );
    let incomplete = verifier(
        "incomplete-review",
        &code(
            "review",
            "",
            &[],
            "{'format': 'code_review', 'version': 1, 'summary': 'Nothing found.', 'coverage': {'complete': False, 'limitations': ['Tests were not read.']}, 'findings': []}",
        ),
        &format!(
            "{CHANGES}    - id: covered\n      statement: The review covered the whole change\n      check: {{kind: assert, path: steps.review.output.coverage.complete, operator: equals, expected: true}}\n"
        ),
    );
    let deployment = bound(
        &[("agent-review", agent), ("incomplete-review", incomplete)],
        &[("reviewer.yaml", REVIEWER)],
    )
    .await;
    answers_with(&deployment, &agent_report).await;
    deployment.admit(&head, "opened").await;

    let shown = delivered(&deployment, 2).await;
    let Delivered { check, served } = &shown["wyrd/agent-review"];
    let execution = served["execution_id"].as_str().expect("execution id");
    assert_eq!(check["head_sha"], head.as_str(), "{check}");
    assert_eq!(check["external_id"], execution, "{check}");
    assert_eq!(
        (&check["conclusion"], &check["output"]["title"]),
        (&json!("success"), &json!("Verdict: passed")),
        "a critical finding leaves the authored verdict alone: {check} {served}"
    );
    assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{served}");
    let summary = check["output"]["summary"].as_str().expect("summary");
    for shown in [
        "The handler normalizes ids.",
        "[critical/high confidence] Finding F-1 (`F-1`) — `src/handler.ts:3-3`",
        "[high/high confidence] Finding F-2 (`F-2`) — `src/user.ts:2-2`\n  Explained F-2.",
        "[medium/high confidence] Finding F-3 (`F-3`)\n  Explained F-3.",
        "[low/high confidence] Finding F-4 (`F-4`) — not shown inline: `anchor_commit_mismatch`",
        "[low/high confidence] Finding F-5 (`F-5`) — not shown inline: `anchor_out_of_range`",
        "[low/high confidence] Finding F-6 (`F-6`) — not shown inline: `anchor_path_unknown`",
    ] {
        assert!(summary.contains(shown), "{shown:?} in {summary}");
    }
    assert!(check.get("annotations").is_none() && check["output"].get("annotations").is_none());
    assert_eq!(
        served["github_delivery"],
        json!({"status": "delivered", "problems": [
            {"finding": "F-4", "reason": "anchor_commit_mismatch"},
            {"finding": "F-5", "reason": "anchor_out_of_range"},
            {"finding": "F-6", "reason": "anchor_path_unknown"},
        ]}),
        "{served}"
    );
    let over_mcp = deployment
        .mcp_read(
            "verification.get_execution",
            execution.parse().expect("execution uuid"),
        )
        .await;
    assert_eq!(
        over_mcp["github_delivery"], served["github_delivery"],
        "MCP serves the same delivery outcome: {over_mcp}"
    );

    let reviews = reviews_of(&deployment, execution);
    assert_eq!(
        reviews.len(),
        1,
        "one review for the one report: {reviews:#?}"
    );
    let review = &reviews[0];
    assert_eq!(
        (&review["event"], &review["commit_id"]),
        (&json!("COMMENT"), &json!(head))
    );
    assert!(
        review["body"]
            .as_str()
            .is_some_and(|body| body.contains(&format!("<!-- wyrd:execution:{execution} -->"))),
        "{review}"
    );
    let comments = review["comments"].as_array().expect("comments");
    assert_eq!(comments.len(), 1, "only F-1 lies inside the diff: {review}");
    assert_eq!(
        (
            &comments[0]["path"],
            &comments[0]["line"],
            &comments[0]["side"]
        ),
        (&json!("src/handler.ts"), &json!(3), &json!("RIGHT"))
    );
    let body = comments[0]["body"].as_str().expect("comment body");
    assert!(
        body.contains("**[critical/high confidence] Finding F-1 (`F-1`)**"),
        "{body}"
    );
    assert!(
        body.contains(&format!("```suggestion\n{fix}```")),
        "the suggestion replaces exactly the anchored line: {body}"
    );

    let Delivered { check, served } = &shown["wyrd/incomplete-review"];
    assert_eq!(served["result"]["outcome"]["verdict"], "failed", "{served}");
    assert_eq!(check["conclusion"], "failure", "{check}");
    let summary = check["output"]["summary"].as_str().expect("summary");
    assert!(
        summary.contains("Nothing found.")
            && summary.contains("**Coverage incomplete.**\n- Tests were not read."),
        "{summary}"
    );
    assert_eq!(served["github_delivery"], json!({"status": "delivered"}));
    let reviews = reviews_of(
        &deployment,
        served["execution_id"].as_str().expect("execution id"),
    );
    assert_eq!(
        reviews.len(),
        1,
        "an empty report is still reviewed: {reviews:#?}"
    );
    assert_eq!(reviews[0]["event"], "COMMENT");
    assert_eq!(reviews[0]["comments"], json!([]), "with nothing inline");
}

/// Each execution selects its report from its accepted final outputs: no
/// report renders only the verdict; two envelopes, even when the second
/// consumed the first, and each item of a fanout are ambiguous; a
/// malformed envelope is invalid; only a repeat's final iteration counts;
/// discarded speculation does not count. While the speculating execution
/// waits for approval its Check is `in_progress` with the reason, and the
/// same Check is completed in place once the gate is rejected.
///
/// # Panics
/// Panics when a selection, Check or outcome differs.
#[tokio::test(flavor = "multi_thread")]
async fn each_execution_selects_its_report_from_its_accepted_final_outputs() {
    let none = verifier(
        "report-none",
        &code("review", "", &[], "{'looked': True}"),
        CHANGES,
    );
    let twice = verifier(
        "report-twice",
        &format!(
            "{}{}",
            code("draft", "", &[], &report("'Draft'")),
            code(
                "final",
                "",
                &[("draft", "steps.draft.output")],
                &report("'Final over ' + draft['summary']"),
            )
        ),
        CHANGES,
    );
    let malformed = verifier(
        "report-malformed",
        &code(
            "review",
            "",
            &[],
            &report("'Future'").replace("'version': 1", "'version': 2"),
        ),
        CHANGES,
    );
    let fanout = verifier(
        "report-fanout",
        &code(
            "review",
            "      foreach: {count: 2}\n",
            &[("item", "item_index")],
            &report("'Item ' + str(item)"),
        ),
        CHANGES,
    );
    let repeat = verifier(
        "report-repeat",
        &code(
            "review",
            "      repeat: {max_iterations: 3, until: {path: iteration.output.summary, operator: equals, expected: Final pass}}\n",
            &[("index", "iteration.index")],
            &report("'Final pass' if index >= 1 else 'First pass'"),
        ),
        CHANGES,
    );
    let speculation = verifier(
        "report-speculation",
        &format!(
            "    - id: gate\n      action: {{kind: approval, message: Review deeper?}}\n{}{}",
            code(
                "deeper",
                "      speculative: true\n      when: {path: steps.gate.output.status, operator: equals, expected: approved}\n",
                &[],
                &report("'Speculated'"),
            ),
            code("kept", "", &[], &report("'Kept'"))
        ),
        CHANGES,
    );
    let deployment = bound(
        &[
            ("report-none", none),
            ("report-twice", twice),
            ("report-malformed", malformed),
            ("report-fanout", fanout),
            ("report-repeat", repeat),
            ("report-speculation", speculation),
        ],
        &[],
    )
    .await;
    deployment.admit(&sha('a'), "opened").await;

    let deadline = tokio::time::Instant::now() + Duration::from_mins(2);
    let waiting = loop {
        let waiting = deployment
            .mock
            .check_runs(REPOSITORY)
            .into_iter()
            .find(|check| check["name"] == "wyrd/report-speculation");
        if let Some(check) = waiting {
            break check;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the waiting execution's Check never appeared"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(
        (
            &waiting["status"],
            &waiting["conclusion"],
            &waiting["output"]["title"]
        ),
        (&json!("in_progress"), &Value::Null, &json!("Waiting")),
        "{waiting}"
    );
    assert!(
        waiting["output"]["summary"].as_str().is_some_and(
            |summary| summary.contains("Waiting for approval of step `gate`: Review deeper?")
        ),
        "{waiting}"
    );
    let execution = waiting["external_id"].as_str().expect("external id");
    let uri = format!("/v1/verification/executions/{execution}");
    let served = loop {
        let served = deployment.get(&uri).await;
        if served["github_delivery"]["status"] != "pending" {
            break served;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the waiting outcome was never recorded: {served}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(served["status"], "waiting", "{served}");
    assert_eq!(served["github_delivery"], json!({"status": "delivered"}));
    let approval = served["approvals"][0]["approval_id"]
        .as_str()
        .expect("approval id");
    deployment
        .post(
            &format!("{uri}/approvals/{approval}"),
            &json!({"decision": "rejected"}),
        )
        .await;

    let shown = delivered(&deployment, 6).await;
    let summary = |name: &str| {
        shown[name].check["output"]["summary"]
            .as_str()
            .expect("summary")
            .to_owned()
    };
    for (name, reason) in [
        ("wyrd/report-none", None),
        ("wyrd/report-twice", Some("report_ambiguous")),
        ("wyrd/report-malformed", Some("report_invalid")),
        ("wyrd/report-fanout", Some("report_ambiguous")),
        ("wyrd/report-repeat", None),
        ("wyrd/report-speculation", None),
    ] {
        let Delivered { check, served } = &shown[name];
        assert_eq!(check["conclusion"], "success", "{name}: {check}");
        assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{name}");
        let outcome = match reason {
            Some(reason) => json!({"status": "delivered", "reason": reason}),
            None => json!({"status": "delivered"}),
        };
        assert_eq!(served["github_delivery"], outcome, "{name}: {served}");
    }
    assert_eq!(
        summary("wyrd/report-none"),
        "",
        "no report shows only the verdict"
    );
    assert!(summary("wyrd/report-twice").contains(
        "**Report not shown:** more than one step returned a `code_review` report (`draft`, `final`)."
    ));
    assert!(
        summary("wyrd/report-malformed")
            .contains("**Report not shown:** the `code_review` report of `review` is unusable")
    );
    assert!(summary("wyrd/report-fanout").contains("(`review[0]`, `review[1]`)"));
    assert_eq!(summary("wyrd/report-repeat"), "Final pass\n");
    assert_eq!(summary("wyrd/report-speculation"), "Kept\n");
    assert_eq!(
        shown["wyrd/report-speculation"].check["id"], waiting["id"],
        "the waiting Check is completed in place"
    );
    for name in ["wyrd/report-repeat", "wyrd/report-speculation"] {
        let execution = shown[name].served["execution_id"]
            .as_str()
            .expect("execution id");
        let reviews = reviews_of(&deployment, execution);
        assert_eq!(reviews.len(), 1, "{name}: {reviews:#?}");
        assert_eq!(
            reviews[0]["comments"],
            json!([]),
            "{name} has nothing inline"
        );
    }
    assert_eq!(
        deployment.mock.reviews(REPOSITORY, PR).len(),
        2,
        "only the rendered reports are reviewed"
    );
}

/// The Verifier `review`, whose code step returns a report with one
/// finding inside the diff of the reviewed head.
fn located() -> String {
    verifier(
        "review",
        &code(
            "review",
            "",
            &[("head", "steps.pr.output.pr.head.sha")],
            "{'format': 'code_review', 'version': 1, 'summary': 'One finding.', 'coverage': {'complete': True, 'limitations': []}, 'findings': [{'id': 'F-1', 'severity': 'high', 'confidence': 'high', 'title': 'Trim', 'explanation': 'The id is trimmed.', 'locations': [{'path': 'src/handler.ts', 'commit': head, 'start_line': 3, 'end_line': 3}], 'evidence': [], 'remediation': None}]}",
        ),
        CHANGES,
    )
}

/// `execution` once it completed and its delivery outcome is recorded.
///
/// # Panics
/// Panics when that does not happen in two minutes.
async fn outcome(deployment: &Deployment, execution: Uuid) -> Value {
    let uri = format!("/v1/verification/executions/{execution}");
    let deadline = tokio::time::Instant::now() + Duration::from_mins(2);
    loop {
        let served = deployment.get(&uri).await;
        if served["status"] == "completed" && served["github_delivery"]["status"] != "pending" {
            return served;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{execution} never recorded a delivery outcome: {served}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// How many delivery requests are `request`.
fn sent(deployment: &Deployment, request: &str) -> usize {
    deployment
        .mock
        .delivery_requests()
        .iter()
        .filter(|(_, label)| label.starts_with(request))
        .count()
}

/// GitHub commits the Check create and a review create but loses both
/// answers; a review create is first rate-limited. Delivery looks each up
/// again before writing: the Check is updated in place and the review is
/// found, so nothing exists twice, and the next request waits out the
/// cooldown.
///
/// # Panics
/// Panics when a Check or review is duplicated or the cooldown is cut
/// short.
#[tokio::test(flavor = "multi_thread")]
async fn a_lost_answer_is_found_again_and_a_cooldown_is_waited_out() {
    let deployment = bound(&[("review", located())], &[]).await;
    let review = format!("POST /pulls/{PR}/reviews");
    let mock = &deployment.mock;
    mock.fail_next_delivery_request("POST /check-runs", DeliveryFault::LoseAnswer);
    mock.fail_next_delivery_request(&review, DeliveryFault::RateLimit(2));
    mock.fail_next_delivery_request(&review, DeliveryFault::LoseAnswer);
    let execution = deployment.admit(&sha('a'), "opened").await;

    let served = outcome(&deployment, execution).await;
    assert_eq!(served["github_delivery"], json!({"status": "delivered"}));
    let checks = mock.check_runs(REPOSITORY);
    assert_eq!(checks.len(), 1, "{checks:#?}");
    assert_eq!(checks[0]["status"], "completed");
    assert_eq!(mock.reviews(REPOSITORY, PR).len(), 1);
    assert_eq!(sent(&deployment, "POST /check-runs"), 1);
    assert!(
        sent(&deployment, "PATCH /check-runs/") >= 1,
        "the found Check is updated"
    );
    assert_eq!(
        sent(&deployment, &review),
        2,
        "limited once, then created once"
    );
    let requests = mock.delivery_requests();
    let limited = requests
        .iter()
        .position(|(_, label)| *label == review)
        .expect("the limited create");
    let waited = requests[limited + 1].0 - requests[limited].0;
    assert!(
        waited >= Duration::from_secs(2),
        "Retry-After is honored: {waited:?}"
    );
}

/// The process restarts after the Check and review landed but before the
/// outcome was recorded: the restarted server delivers again from the
/// retained execution, reading only GitHub's Checks and reviews, finds both,
/// creates nothing and records the outcome.
///
/// # Panics
/// Panics when the restart recomputes, creates or misses the outcome.
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_before_the_outcome_finds_what_already_landed() {
    let deployment = bound(&[("review", located())], &[]).await;
    let held = deployment
        .mock
        .stall_next_delivery_request("POST /check-runs");
    let execution = deployment.admit(&sha('a'), "opened").await;
    held.arrived().await;
    let scribe = hold_scribe(&deployment.server).await;
    held.release();
    let deadline = tokio::time::Instant::now() + Duration::from_mins(1);
    while deployment.mock.reviews(REPOSITORY, PR).is_empty() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the review never landed"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let reads = deployment.mock.repository_reads(REPOSITORY).len();
    let posts = sent(&deployment, "POST ");

    let deployment = deployment.restart().await;
    let served = outcome(&deployment, execution).await;
    assert_eq!(served["github_delivery"], json!({"status": "delivered"}));
    assert_eq!(deployment.mock.check_runs(REPOSITORY).len(), 1);
    assert_eq!(deployment.mock.reviews(REPOSITORY, PR).len(), 1);
    assert_eq!(
        sent(&deployment, "POST "),
        posts,
        "nothing is created again"
    );
    let after = deployment.mock.repository_reads(REPOSITORY);
    assert_eq!(
        after.len(),
        reads,
        "the restart reads no source again: {:?}",
        &after[reads..]
    );
    drop(scribe);
}

/// A new head pushed while the Check create is in flight supersedes the
/// execution: GitHub fails that create, and delivery writes nothing more
/// for it, recording it skipped as superseded while the new head's
/// execution is delivered.
///
/// # Panics
/// Panics when the superseded execution writes again or is not skipped.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_head_during_delivery_skips_every_later_write() {
    let deployment = bound(&[("review", located())], &[]).await;
    deployment.serve_b();
    let held = deployment
        .mock
        .stall_next_delivery_request("POST /check-runs");
    let first = deployment.admit(&sha('a'), "opened").await;
    held.arrived().await;
    let second = deployment.admit(&sha('b'), "synchronize").await;
    held.release();

    let served = outcome(&deployment, first).await;
    assert_eq!(
        served["github_delivery"],
        json!({"status": "skipped", "reason": "superseded"}),
        "{served}"
    );
    let served = outcome(&deployment, second).await;
    assert_eq!(served["github_delivery"], json!({"status": "delivered"}));
    let checks = deployment.mock.check_runs(REPOSITORY);
    assert_eq!(checks.len(), 1, "{checks:#?}");
    assert_eq!(checks[0]["head_sha"], sha('b'));
    let reviews = deployment.mock.reviews(REPOSITORY, PR);
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0]["commit_id"], sha('b'));
}

/// The App is uninstalled while the Check create is in flight: GitHub fails
/// that create, and delivery writes nothing more, recording it skipped as
/// access removed.
///
/// # Panics
/// Panics when anything is written after the uninstall or the outcome is
/// not skipped.
#[tokio::test(flavor = "multi_thread")]
async fn removed_access_during_delivery_skips_every_later_write() {
    let deployment = bound(&[("review", located())], &[]).await;
    let held = deployment
        .mock
        .stall_next_delivery_request("POST /check-runs");
    let execution = deployment.admit(&sha('a'), "opened").await;
    held.arrived().await;
    let deleted = json!({"action": "deleted", "installation": {"id": INSTALLATION}});
    let status = deployment
        .mock
        .deliver(
            deployment.server.base_url().expect("bound"),
            "installation",
            &deleted,
        )
        .await
        .expect("delivery sends");
    assert_eq!(status, StatusCode::ACCEPTED);
    held.release();

    let served = outcome(&deployment, execution).await;
    assert_eq!(
        served["github_delivery"],
        json!({"status": "skipped", "reason": "access_removed"}),
        "{served}"
    );
    assert!(deployment.mock.check_runs(REPOSITORY).is_empty());
    assert!(deployment.mock.reviews(REPOSITORY, PR).is_empty());
    assert_eq!(
        sent(&deployment, "POST "),
        1,
        "only the failed create was sent"
    );
}

/// GitHub refuses the Check create permanently: delivery stops, records a
/// failed outcome GitHub refused, and leaves the verdict alone.
///
/// # Panics
/// Panics when delivery retries the refusal, writes the review, or hides
/// the failure.
#[tokio::test(flavor = "multi_thread")]
async fn a_permanent_refusal_is_a_visible_failed_outcome() {
    let deployment = bound(&[("review", located())], &[]).await;
    deployment
        .mock
        .fail_next_delivery_request("POST /check-runs", DeliveryFault::Refuse);
    let execution = deployment.admit(&sha('a'), "opened").await;

    let served = outcome(&deployment, execution).await;
    assert_eq!(
        served["github_delivery"],
        json!({"status": "failed", "reason": "github_refused"}),
        "{served}"
    );
    assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{served}");
    assert_eq!(sent(&deployment, "POST "), 1, "a refusal is not retried");
    assert!(deployment.mock.check_runs(REPOSITORY).is_empty());
    assert!(deployment.mock.reviews(REPOSITORY, PR).is_empty());
}

/// The citations the public `examples/review-a-pull-request` requirement
/// gives every verdict: the reviewed head and the commit it compared from.
fn example_citations() -> Value {
    json!([
        {"slot": "pull_request", "locator": {"json_pointer": "/pr/head/sha"}},
        {"slot": "pull_request", "locator": {"json_pointer": "/review/comparison_base"}},
    ])
}

/// Run the public `examples/review-a-pull-request` policy, declared as
/// published, on PR [`PR`] at head `a…` whose reviewer answers `report`;
/// returns the deployment and the execution's delivered Check.
///
/// # Panics
/// Panics when the example does not declare or its Check is never
/// delivered.
async fn review_example(report: &Value) -> (Deployment, Delivered) {
    let example = |file: &str| {
        std::fs::read_to_string(format!(
            "{}/../../../examples/review-a-pull-request/{file}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("the example file reads")
    };
    let (repository, verifier, reviewer) = (
        example("repository.yaml"),
        example("verifier.yaml"),
        example("reviewer.yaml"),
    );
    let deployment = Deployment::start(&[
        ("policy.yaml", &repository),
        ("verifier.yaml", &verifier),
        ("reviewer.yaml", &reviewer),
    ])
    .await;
    let head = sha('a');
    deployment.head(&head, &sha('8'), HANDLER_A);
    deployment.compare(&sha('c'), &head, &sha('c'), HANDLER_A, PATCH_A);
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("Active changes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-review", "object": "chat.completion", "created": 1, "model": "gpt-4o",
            "choices": [{"index": 0, "finish_reason": "stop",
                         "message": {"role": "assistant", "content": report.to_string()}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
        })))
        .mount(&deployment.model)
        .await;
    deployment.admit(&head, "opened").await;
    let mut shown = delivered(&deployment, 1).await;
    let check = shown
        .remove("wyrd/code-review")
        .expect("the example's Check");
    (deployment, check)
}

/// The example requirement's verdict, reason and citations from `served`,
/// the inspection of `check`'s execution, with each
/// citation's JSON pointer resolved against the execution's retained
/// `pr` extraction.
///
/// # Panics
/// Panics when the outcome holds no requirement or a citation does not
/// resolve.
async fn example_decision(
    deployment: &Deployment,
    check: &Value,
    served: &Value,
) -> (Value, Value, Vec<Value>) {
    let requirement = &served["result"]["outcome"]["requirements"][0];
    assert_eq!(requirement["citations"], example_citations(), "{served}");
    let execution: Uuid = check["external_id"]
        .as_str()
        .and_then(|id| id.parse().ok())
        .expect("execution id");
    let extraction = deployment.extraction(execution).await;
    let resolved = example_citations()
        .as_array()
        .expect("citations")
        .iter()
        .map(|citation| {
            let pointer = citation["locator"]["json_pointer"]
                .as_str()
                .expect("pointer");
            extraction
                .pointer(pointer)
                .cloned()
                .expect("the citation resolves")
        })
        .collect();
    (
        served["result"]["outcome"]["verdict"].clone(),
        requirement["reason"].clone(),
        resolved,
    )
}

/// A complete `code_review` report with `findings`.
fn example_report(complete: bool, findings: &Value) -> Value {
    json!({
        "format": "code_review", "version": 1, "summary": "Reviewed.",
        "coverage": {"complete": complete, "limitations": if complete { json!([]) } else { json!(["Tests were not read."]) }},
        "findings": findings,
    })
}

/// A clean, complete report passes the public example with a `success`
/// Check, and an incomplete one is `inconclusive` with `action_required`;
/// each verdict cites the pinned pull request through the extraction's own
/// `pull_request` slot, resolving to the reviewed head `a…` and the
/// comparison base `c…` in that execution's retained extraction.
///
/// # Panics
/// Panics when a verdict, Check, citation, or resolved commit differs.
#[tokio::test(flavor = "multi_thread")]
async fn the_review_example_cites_the_pinned_pull_request_it_passes() {
    for (complete, verdict, conclusion, reason) in [
        (
            true,
            "passed",
            "success",
            "The review is complete with no critical or high issues.",
        ),
        (
            false,
            "inconclusive",
            "action_required",
            "The review did not cover the whole change.",
        ),
    ] {
        let (deployment, Delivered { check, served }) =
            review_example(&example_report(complete, &json!([]))).await;
        assert_eq!(
            example_decision(&deployment, &check, &served).await,
            (
                json!(verdict),
                json!(reason),
                vec![json!(sha('a')), json!(sha('c'))]
            ),
            "{served}"
        );
        assert_eq!(
            (&check["head_sha"], &check["conclusion"]),
            (&json!(sha('a')), &json!(conclusion)),
            "{check}"
        );
        assert_eq!(served["github_delivery"], json!({"status": "delivered"}));
    }
}

/// The public example, declared as published: the pull request is
/// extracted, its Agent's `code_review` report with one high finding fails
/// the example's requirement, still citing the pinned pull request, and the
/// Check concludes `failure` beside one `COMMENT` review whose inline
/// comment carries the exact suggestion.
///
/// # Panics
/// Panics when the example does not declare, or its Check, review,
/// citations or outcome differs.
#[tokio::test(flavor = "multi_thread")]
async fn the_review_example_fails_on_a_high_finding_and_comments_its_fix() {
    let head = sha('a');
    let line = json!({"path": "src/handler.ts", "commit": head, "start_line": 3, "end_line": 3});
    let fix = "export const handle = (id: string) => lookupUser(id.trim().toLowerCase());\n";
    let findings = json!([{"id": "case", "severity": "high", "confidence": "medium",
                      "title": "Mixed-case ids miss their user",
                      "explanation": "lookupUser compares lower-case ids.",
                      "locations": [line], "evidence": [],
                      "remediation": {"explanation": "Lower-case the id.",
                                      "suggested_edit": {"location": line, "replacement": fix}}}]);
    let (deployment, Delivered { check, served }) =
        review_example(&example_report(true, &findings)).await;
    assert_eq!(
        example_decision(&deployment, &check, &served).await,
        (
            json!("failed"),
            json!("The review found critical or high issues."),
            vec![json!(head), json!(sha('c'))]
        ),
        "{served}"
    );
    assert_eq!(
        (&check["head_sha"], &check["conclusion"]),
        (&json!(head), &json!("failure")),
        "{check}"
    );
    assert_eq!(served["github_delivery"], json!({"status": "delivered"}));
    let reviews = deployment.mock.reviews(REPOSITORY, PR);
    assert_eq!(reviews.len(), 1, "{reviews:#?}");
    let comments = reviews[0]["comments"].as_array().expect("comments");
    assert_eq!(comments.len(), 1, "{reviews:#?}");
    let body = comments[0]["body"].as_str().expect("comment body");
    assert!(
        body.contains("[high/medium confidence] Mixed-case ids miss their user (`case`)")
            && body.contains(&format!("```suggestion\n{fix}```")),
        "{body}"
    );
}

/// A code requirement judged after an approval resumes the execution cites
/// the extraction it pinned before waiting, though GitHub now serves head
/// `b…`: the pinned head still validates and is the one the check reads.
/// A valid uncited pass is kept, while a pointer the extraction does not
/// hold and a slot the check does not reach are explicit invalid
/// citations, never dropped. A Verifier declaring its own `pull_request`
/// input is refused, so no caller can replace the source.
///
/// # Panics
/// Panics when a requirement outcome or reason differs, the pinned head
/// moves, or the colliding Verifier is accepted.
#[tokio::test(flavor = "multi_thread")]
async fn a_resumed_check_cites_the_pull_request_it_pinned() {
    let judged = |id: &str, outcome: &str, citations: &str| {
        format!(
            "    - id: {id}\n      statement: {id}\n      check:\n        kind: code\n        entrypoint: judge\n        inputs: {{pr: steps.pr.output.pr, approved: steps.hold.output}}\n        source: {{inline: \"def judge(pr, approved):\\n    return {{'outcome': {outcome}, 'reason': '{id}', 'citations': {citations}}}\\n\"}}\n"
        )
    };
    let head = format!(
        "'passed' if pr['head']['sha'] == '{}' else 'failed'",
        sha('a')
    );
    let requirements = [
        judged(
            "pinned",
            &head,
            "[{'slot': 'pull_request', 'locator': {'json_pointer': '/pr/head/sha'}}]",
        ),
        judged("uncited", "'passed'", "[]"),
        judged(
            "forged",
            "'passed'",
            "[{'slot': 'pull_request', 'locator': {'json_pointer': '/pr/forged'}}]",
        ),
        judged(
            "elsewhere",
            "'passed'",
            "[{'slot': 'attachment_0', 'locator': {'json_pointer': '/pr'}}]",
        ),
    ]
    .concat();
    let steps = "    - id: hold\n      depends_on: [pr]\n      action: {kind: approval, message: Judge the review?}\n";
    let deployment = bound(
        &[("resumed", verifier("resumed", steps, &requirements))],
        &[],
    )
    .await;
    let execution = deployment.admit(&sha('a'), "opened").await;
    let uri = format!("/v1/verification/executions/{execution}");
    let (_, approval) = waiting_for(&deployment, execution, "hold").await;
    deployment.serve_b();
    deployment
        .post(
            &format!("{uri}/approvals/{approval}"),
            &json!({"decision": "approved"}),
        )
        .await;

    let shown = delivered(&deployment, 1).await;
    let Delivered { check, served } = &shown["wyrd/resumed"];
    let decided: BTreeMap<String, (Value, Value)> = served["result"]["outcome"]["requirements"]
        .as_array()
        .expect("requirements")
        .iter()
        .map(|requirement| {
            (
                requirement["id"].as_str().expect("id").to_owned(),
                (
                    requirement["outcome"].clone(),
                    requirement["reason"].clone(),
                ),
            )
        })
        .collect();
    let expected = |outcome: &str, reason: &str| (json!(outcome), json!(reason));
    assert_eq!(
        decided,
        BTreeMap::from([
            ("pinned".to_owned(), expected("passed", "pinned")),
            ("uncited".to_owned(), expected("passed", "uncited")),
            (
                "forged".to_owned(),
                expected(
                    "inconclusive",
                    "invalid citation: slot pull_request has no location /pr/forged"
                )
            ),
            (
                "elsewhere".to_owned(),
                expected(
                    "inconclusive",
                    "invalid citation: slot attachment_0 is not evidence this check reaches"
                )
            ),
        ]),
        "{served}"
    );
    assert_eq!(
        deployment.extraction(execution).await["pr"]["head"]["sha"],
        json!(sha('a'))
    );
    assert_eq!(check["head_sha"], json!(sha('a')), "{check}");

    let colliding = verifier("colliding", "", CHANGES);
    let (status, refusal) = deployment
        .declaring(
            &[
                ("policy.yaml", &policy(&["colliding"])),
                ("colliding.yaml", &colliding),
            ],
            "colliding",
            |body| {
                for submission in body["submissions"].as_array_mut().expect("submissions") {
                    if submission["kind"] == "Verifier" {
                        submission["spec"]["inputs"] = json!({"pull_request": {"type": "json"}});
                    }
                }
            },
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refusal}");
    assert_eq!(
        (&refusal["code"], &refusal["details"]["rule"]),
        (&json!("WYRD_SPEC_400_VALIDATION"), &json!("source_slot")),
        "{refusal}"
    );
}

/// A Verifier whose `gates` approval steps run one after another before a
/// code step returns an empty report.
fn gated(gates: &[&str]) -> String {
    use std::fmt::Write as _;
    let mut steps = String::new();
    let mut after = "pr";
    for gate in gates {
        let _ = write!(
            steps,
            "    - id: {gate}\n      depends_on: [{after}]\n      action: {{kind: approval, message: Ship {gate}?}}\n"
        );
        after = gate;
    }
    steps.push_str(
        &code("review", "", &[], &report("'Clean'"))
            .replace("depends_on: [pr]", &format!("depends_on: [{after}]")),
    );
    verifier("gated", &steps, CHANGES)
}

/// `execution` once it waits for approval step `step` and the outcome of
/// that waiting state is recorded; returns the inspection and the pending
/// approval's id.
///
/// # Panics
/// Panics when that does not happen in two minutes.
async fn waiting_for(deployment: &Deployment, execution: Uuid, step: &str) -> (Value, String) {
    let uri = format!("/v1/verification/executions/{execution}");
    let deadline = tokio::time::Instant::now() + Duration::from_mins(2);
    loop {
        let served = deployment.get(&uri).await;
        let pending = served["approvals"].as_array().and_then(|approvals| {
            approvals
                .iter()
                .find(|approval| approval["step_id"] == step && approval["status"] == "pending")
        });
        if let Some(approval) = pending
            && served["status"] == "waiting"
            && served["github_delivery"]["status"] != "pending"
        {
            let id = approval["approval_id"]
                .as_str()
                .expect("approval id")
                .to_owned();
            return (served, id);
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{execution} never waited on `{step}` with an outcome: {served}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// A waiting delivery creates the Check but never updates one: when the
/// execution waits again on a second approval the Check is left as the
/// first wait wrote it, with no update sent, and only the terminal delivery
/// completes it in place. A late waiting update can therefore never set a
/// completed Check back to `in_progress`.
///
/// # Panics
/// Panics when a waiting delivery updates the Check or the terminal one
/// does not complete it.
#[tokio::test(flavor = "multi_thread")]
async fn a_waiting_delivery_never_updates_an_existing_check() {
    let deployment = bound(&[("gated", gated(&["first", "second"]))], &[]).await;
    let execution = deployment.admit(&sha('a'), "opened").await;
    let uri = format!("/v1/verification/executions/{execution}");

    let (_, first) = waiting_for(&deployment, execution, "first").await;
    let created = deployment.mock.check_runs(REPOSITORY);
    assert_eq!(created.len(), 1);
    assert_eq!(created[0]["status"], "in_progress");
    deployment
        .post(
            &format!("{uri}/approvals/{first}"),
            &json!({"decision": "approved"}),
        )
        .await;

    let (served, second) = waiting_for(&deployment, execution, "second").await;
    assert_eq!(served["github_delivery"], json!({"status": "delivered"}));
    assert_eq!(
        sent(&deployment, "PATCH "),
        0,
        "waiting never updates the Check"
    );
    assert_eq!(deployment.mock.check_runs(REPOSITORY), created);
    deployment
        .post(
            &format!("{uri}/approvals/{second}"),
            &json!({"decision": "approved"}),
        )
        .await;

    let shown = delivered(&deployment, 1).await;
    let Delivered { check, served } = &shown["wyrd/gated"];
    assert_eq!(
        (&check["id"], &check["conclusion"]),
        (&created[0]["id"], &json!("success"))
    );
    assert_eq!(served["github_delivery"], json!({"status": "delivered"}));
    assert_eq!(sent(&deployment, "PATCH "), 1);
}

/// Cancelling a waiting execution delivers once more: its same Check is
/// completed `cancelled` in place, inspection serves the cancelled
/// outcome, and a restart delivers nothing again.
///
/// # Panics
/// Panics when the Check stays `in_progress`, a second Check appears, the
/// outcome differs, or the restart writes again.
#[tokio::test(flavor = "multi_thread")]
async fn cancelling_a_waiting_execution_completes_its_check_cancelled() {
    let deployment = bound(&[("gated", gated(&["first"]))], &[]).await;
    let execution = deployment.admit(&sha('a'), "opened").await;
    let uri = format!("/v1/verification/executions/{execution}");
    let (_, _) = waiting_for(&deployment, execution, "first").await;
    let waiting = deployment.mock.check_runs(REPOSITORY);
    assert_eq!(waiting[0]["status"], "in_progress");

    let cancelled = deployment.post(&format!("{uri}/cancel"), &json!({})).await;
    assert_eq!(cancelled["status"], "cancelled", "{cancelled}");
    let deadline = tokio::time::Instant::now() + Duration::from_mins(2);
    let served = loop {
        let served = deployment.get(&uri).await;
        if served["github_delivery"]["status"] != "pending" {
            break served;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the cancellation was never delivered: {served}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(served["status"], "cancelled", "{served}");
    assert_eq!(served["github_delivery"], json!({"status": "delivered"}));
    let checks = deployment.mock.check_runs(REPOSITORY);
    assert_eq!(checks.len(), 1, "{checks:#?}");
    assert_eq!(
        (
            &checks[0]["id"],
            &checks[0]["status"],
            &checks[0]["conclusion"]
        ),
        (&waiting[0]["id"], &json!("completed"), &json!("cancelled"))
    );
    let writes = sent(&deployment, "PATCH ") + sent(&deployment, "POST ");

    let deployment = deployment.restart().await;
    let served = deployment.get(&uri).await;
    assert_eq!(served["github_delivery"], json!({"status": "delivered"}));
    assert_eq!(
        sent(&deployment, "PATCH ") + sent(&deployment, "POST "),
        writes
    );
}
