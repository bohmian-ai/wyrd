//! Compiled-CLI journeys for `wyrd workflow`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use assert_cmd::prelude::*;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Secret header value the local external gateway binding reads from a file.
const SECRET: &str = "cli-workflow-binding-secret";

/// Absolute path of a repository file.
fn repo(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(relative)
}

/// Runs `wyrd workflow <args>` with `config_home` as the only client
/// configuration and no ambient credential or endpoint.
///
/// # Panics
/// Panics when the compiled binary cannot be run.
fn workflow(config_home: &Path, args: &[&str]) -> Output {
    Command::cargo_bin("wyrd")
        .expect("wyrd binary")
        .arg("workflow")
        .args(args)
        .env("WYRD_CONFIG_HOME", config_home)
        .env_remove("WYRD_SERVER_URL")
        .env_remove("WYRD_ACCESS_TOKEN")
        .env_remove("WYRD_WORKLOAD_TOKEN")
        .env_remove("WYRD_TENANT")
        .env_remove("WYRD_API_KEY")
        .output()
        .expect("compiled workflow command runs")
}

/// Both output streams, for leak and diagnostic assertions.
fn streams(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The structured problem the CLI writes to stderr.
///
/// # Panics
/// Panics when stderr carries no problem JSON line.
fn problem(output: &Output) -> Value {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .find_map(|line| serde_json::from_str(line).ok())
        .unwrap_or_else(|| panic!("CLI emits structured problem JSON: {}", streams(output)))
}

/// The run snapshot the CLI writes to stdout in JSON mode.
///
/// # Panics
/// Panics when stdout is not JSON.
fn run_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| panic!("stdout is a run snapshot: {}", streams(output)))
}

/// Copy the checked-in code-review example into a temporary directory with
/// its Workflow route replaced by `route`.
///
/// # Panics
/// Panics when the example cannot be copied.
fn code_review_with_route(route: &str) -> tempfile::TempDir {
    let bundle = repo("examples/workflows/code-review");
    let temp = tempfile::tempdir().expect("bundle tempdir");
    for dir in ["agents", "prompts"] {
        std::fs::create_dir(temp.path().join(dir)).expect("bundle directory");
        for file in ["security", "correctness", "final-reviewer"] {
            std::fs::copy(
                bundle.join(format!("{dir}/{file}.yaml")),
                temp.path().join(format!("{dir}/{file}.yaml")),
            )
            .expect("bundle file copies");
        }
    }
    let yaml = std::fs::read_to_string(bundle.join("workflow.yaml")).expect("workflow reads");
    assert!(
        yaml.contains("    kind: wyrd_gateway"),
        "example route moved"
    );
    std::fs::write(
        temp.path().join("workflow.yaml"),
        yaml.replacen("    kind: wyrd_gateway", route, 1),
    )
    .expect("workflow writes");
    temp
}

/// The `ext_gateway` route for `upstream`, naming the `review-gateway`
/// binding.
fn external_route(upstream: &MockServer) -> String {
    format!(
        "    kind: ext_gateway\n    protocol: openai_chat\n    base_url: {}/v1\n    credential_binding: review-gateway",
        upstream.uri()
    )
}

/// Write a client configuration whose `review-gateway` binding sends the
/// owner-only secret file as `x-review-secret` to `upstream`.
///
/// # Panics
/// Panics when the files cannot be written.
fn configure_binding(config_home: &Path, upstream: &MockServer) {
    let secret = config_home.join("review-secret");
    std::fs::write(&secret, SECRET).expect("secret writes");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600))
            .expect("secret restricts");
    }
    std::fs::write(
        config_home.join("config.toml"),
        format!(
            "[workflow.external_gateway_bindings.review-gateway]\n\
             protocol = \"openai_chat\"\n\
             origin = \"{}\"\n\
             secret_headers = {{ x-review-secret = {{ source = \"file\", path = \"{}\" }} }}\n",
            upstream.uri(),
            secret.display()
        ),
    )
    .expect("config writes");
}

/// A Chat Completions upstream answering every call with `status` and `body`.
async fn chat_upstream(status: u16, body: Value) -> MockServer {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&upstream)
        .await;
    upstream
}

/// A Chat Completions success whose assistant message is `text`.
fn chat_completion(text: &str) -> Value {
    json!({
        "id": "resp",
        "object": "chat.completion",
        "created": 0,
        "model": "gpt-5-5",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": text },
            "finish_reason": "stop"
        }]
    })
}

/// Every `wyrd workflow` choice is unambiguous and validated before any side
/// effect, and local runs print the portable run snapshot.
///
/// Malformed, mixed, or incomplete selectors, both inputs, non-object input,
/// a file source on the server, `--detach` locally, `--server` with a file,
/// and a malformed run ID all fail with exit code 64; the ones that parse
/// carry `WYRD_CLI_400_INVALID_ARGUMENT`, which proves no client was built.
/// The actual code-review bundle then runs on a local `ext_gateway` binding
/// whose secret is read from a file at run time and never printed; a
/// Native mock Workflow renders named outputs and, only with `--steps`, its
/// step results. An unconfigured binding is refused before dispatch and a
/// failed run exits 2 with its snapshot.
///
/// # Panics
/// Panics when any invocation's exit code, problem, output, or upstream
/// traffic differs from the asserted contract.
#[tokio::test(flavor = "multi_thread")]
async fn workflow_cli_contract() {
    let config_home = tempfile::tempdir().expect("config home");
    let home = config_home.path();
    let local = repo("tests/fixtures/workflow-loading/shadowed/local-workflow.yaml");
    let local = local.to_str().expect("utf-8 fixture path");
    let uid = "0190d6a0-0000-7000-8000-000000000000";

    let usage: [&[&str]; 9] = [
        &["run"],
        &["run", "--file", local, "--uid", uid],
        &[
            "run",
            "--uid",
            uid,
            "--space",
            "s",
            "--name",
            "n",
            "--version",
            "1.0.0",
        ],
        &["run", "--space", "s", "--name", "n"],
        &["run", "--name", "n", "--version", "1.0.0"],
        &[
            "run",
            "--file",
            local,
            "--input",
            "{}",
            "--input-file",
            local,
        ],
        &["run", "--file", local, "--server", "http://127.0.0.1:9"],
        &["run", "--file", local, "--execution", "remote"],
        &["status"],
    ];
    for args in usage {
        let output = workflow(home, args);
        assert_eq!(
            output.status.code(),
            Some(64),
            "{args:?}: {}",
            streams(&output)
        );
    }

    let invalid: [(&[&str], &str); 7] = [
        (&["run", "--file", local, "--input", "[1]"], "input"),
        (&["run", "--file", local, "--input", "{not json"], "input"),
        (&["run", "--file", local, "--execution", "server"], "file"),
        (&["run", "--file", local, "--detach"], "detach"),
        (&["run", "--uid", "not-a-uid"], "uid"),
        (&["status", "not-a-run"], "run-id"),
        (&["cancel", "not-a-run"], "run-id"),
    ];
    for (args, field) in invalid {
        let output = workflow(home, args);
        assert_eq!(
            output.status.code(),
            Some(64),
            "{args:?}: {}",
            streams(&output)
        );
        let problem = problem(&output);
        assert_eq!(
            problem["code"], "WYRD_CLI_400_INVALID_ARGUMENT",
            "{problem}"
        );
        assert!(streams(&output).contains(field), "{args:?} names {field}");
        assert!(
            !streams(&output).contains("not json"),
            "input is never echoed"
        );
    }

    let native = workflow(
        home,
        &["run", "--file", local, "--input", r#"{"code":"diff"}"#],
    );
    assert_eq!(native.status.code(), Some(0), "{}", streams(&native));
    let text = String::from_utf8_lossy(&native.stdout);
    assert!(text.contains(" succeeded"), "{text}");
    assert!(
        text.contains("output security: local security review of diff"),
        "{text}"
    );
    assert!(!text.contains("step "), "steps only on request: {text}");
    let native = workflow(
        home,
        &[
            "run",
            "--file",
            local,
            "--input",
            r#"{"code":"diff"}"#,
            "--steps",
        ],
    );
    let text = String::from_utf8_lossy(&native.stdout);
    for step in ["security", "correctness", "final_review"] {
        assert!(text.contains(&format!("step {step}: succeeded")), "{text}");
    }

    let upstream = chat_upstream(200, chat_completion("REVIEWED")).await;
    let bundle = code_review_with_route(&external_route(&upstream));
    let entry = bundle.path().join("workflow.yaml");
    let entry = entry.to_str().expect("utf-8 bundle path");
    let input = repo("examples/workflows/code-review/input.json");
    let input = input.to_str().expect("utf-8 input path");

    let unconfigured = workflow(home, &["run", "--file", entry, "--input-file", input]);
    assert_eq!(
        problem(&unconfigured)["code"],
        "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE"
    );
    assert!(
        upstream
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );

    configure_binding(home, &upstream);
    let external = workflow(
        home,
        &[
            "run",
            "--file",
            entry,
            "--input-file",
            input,
            "--format",
            "json",
        ],
    );
    assert_eq!(external.status.code(), Some(0), "{}", streams(&external));
    let run = run_json(&external);
    assert_eq!(run["status"], "succeeded");
    assert_eq!(run["workflow"], Value::Null);
    assert_eq!(run["outputs"], json!({ "review": "REVIEWED" }));
    for step in ["security", "correctness", "final_review"] {
        assert_eq!(run["steps"][step]["status"], "succeeded", "{run}");
    }
    let requests = upstream.received_requests().await.unwrap_or_default();
    assert_eq!(requests.len(), 3);
    assert!(requests.iter().all(|request| {
        request
            .headers
            .get("x-review-secret")
            .map(|value| value.as_bytes())
            == Some(SECRET.as_bytes())
    }));
    assert!(!streams(&external).contains(SECRET));

    let refusing = chat_upstream(400, json!({ "error": { "message": "refused" } })).await;
    let refused_bundle = code_review_with_route(&external_route(&refusing));
    configure_binding(home, &refusing);
    let refused_entry = refused_bundle.path().join("workflow.yaml");
    let failed = workflow(
        home,
        &[
            "run",
            "--file",
            refused_entry.to_str().expect("utf-8 bundle path"),
            "--input-file",
            input,
            "--format",
            "json",
        ],
    );
    assert_eq!(failed.status.code(), Some(2), "{}", streams(&failed));
    let run = run_json(&failed);
    assert_eq!(run["status"], "failed", "{run}");
    assert_eq!(run["outputs"], json!({}));
    assert!(!streams(&failed).contains(SECRET));
}
