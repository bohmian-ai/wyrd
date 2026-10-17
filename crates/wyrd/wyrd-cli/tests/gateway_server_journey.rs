//! Compiled-CLI journey for tenant gateway administration against a real server.

use assert_cmd::prelude::*;
use serde_json::{Value, json};
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::server::TEST_GATEWAY_CREDENTIAL_BINDING;

/// Runs `wyrd gateway <args>` against `endpoint`, supplying `token` through
/// the installed credential resolver's `WYRD_ACCESS_TOKEN` environment input
/// with an empty configuration home and no competing credentials.
fn gateway(endpoint: &str, token: &str, args: &[&str]) -> Output {
    gateway_env(endpoint, token, args, &[])
}

/// Runs [`gateway`] with `vars` added to the command's environment, the way
/// an administrator exports provider keys a document names.
fn gateway_env(endpoint: &str, token: &str, args: &[&str], vars: &[(&str, &str)]) -> Output {
    let empty_config = tempfile::tempdir().expect("empty config home");
    Command::cargo_bin("wyrd")
        .expect("wyrd binary")
        .arg("gateway")
        .args(args)
        .args(["--server", endpoint])
        .envs(vars.iter().copied())
        .env("WYRD_CONFIG_HOME", empty_config.path())
        .env("WYRD_ACCESS_TOKEN", token)
        .env_remove("WYRD_WORKLOAD_TOKEN")
        .env_remove("WYRD_TENANT")
        .env_remove("WYRD_API_KEY")
        .output()
        .expect("compiled gateway command runs")
}

/// A bearer token passed as an argument is refused before any request and is
/// never echoed.
#[test]
fn gateway_rejects_token_argument() {
    let output = Command::cargo_bin("wyrd")
        .expect("wyrd binary")
        .args(["gateway", "credential", "list", "--token", "secret-value"])
        .output()
        .expect("compiled gateway command runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(64), "{stderr}");
    assert!(stderr.contains("unexpected argument '--token'"), "{stderr}");
    assert!(!stderr.contains("secret-value"));
}

/// Runs `wyrd gateway <args>` with `stdin` piped to the command, the shape a
/// tenant administrator uses to submit a managed secret without placing it in
/// process arguments or a checked-in document.
///
/// # Panics
///
/// Panics when the binary cannot be spawned, its stdin cannot be written, or
/// it does not exit.
fn gateway_stdin(endpoint: &str, token: &str, args: &[&str], stdin: &str) -> Output {
    let empty_config = tempfile::tempdir().expect("empty config home");
    let mut child = Command::cargo_bin("wyrd")
        .expect("wyrd binary")
        .arg("gateway")
        .args(args)
        .args(["--server", endpoint])
        .env("WYRD_CONFIG_HOME", empty_config.path())
        .env("WYRD_ACCESS_TOKEN", token)
        .env_remove("WYRD_WORKLOAD_TOKEN")
        .env_remove("WYRD_TENANT")
        .env_remove("WYRD_API_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("compiled gateway command spawns");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(stdin.as_bytes())
        .expect("submission writes");
    child.wait_with_output().expect("compiled command exits")
}

/// Asserts success and decodes stdout JSON, or `null` for an empty body.
fn ok_json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "gateway command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&stdout).expect("stdout is JSON")
    }
}

/// Asserts failure and returns the structured problem from stderr.
fn problem(output: &Output) -> Value {
    assert!(!output.status.success(), "command unexpectedly succeeded");
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .find_map(|line| serde_json::from_str(line).ok())
        .expect("CLI emits structured problem JSON")
}

/// Writes one JSON request document and returns its path as a string.
fn document(dir: &Path, name: &str, value: &Value) -> String {
    let path = dir.join(name);
    std::fs::write(&path, value.to_string()).expect("document writes");
    path.display().to_string()
}

/// An administrator drives the credential, deployment, and capture lifecycle
/// through the compiled CLI; a reader is denied and a referenced credential
/// cannot be deleted.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the serialized Postgres-backed CLI journey lane"]
async fn gateway_commands_manage_redacted_tenant_configuration() {
    // A first attempt at the body gets the source field's shape wrong. The
    // command refuses it, names the flag and the position, and repeats no part
    // of the document — the key the administrator just typed included.
    const MISTYPED: &str = "sk-live-cli-mistyped";
    const SUBMITTED: &str = "sk-live-cli-submission";
    const ROTATED: &str = "sk-live-cli-rotation";
    let server = WyrdTestServer::start_bound()
        .await
        .expect("test server starts");
    let endpoint = server.base_url().expect("server is bound").to_owned();
    let admin = server
        .bootstrap_user("cli-gateway-admin", &["admin"])
        .await
        .expect("admin bootstraps");
    let token = admin.jwt().expect("admin jwt").to_owned();
    let dir = tempfile::tempdir().expect("tempdir");

    let credential = document(
        dir.path(),
        "credential.json",
        &json!({
            "name": "primary",
            "provider": "openai",
            "source": {"environment": {"binding": TEST_GATEWAY_CREDENTIAL_BINDING}},
        }),
    );
    let created = ok_json(&gateway(
        &endpoint,
        &token,
        &["credential", "put", "--file", &credential],
    ));
    assert_eq!(created["state"], "active");
    let again = ok_json(&gateway(
        &endpoint,
        &token,
        &["credential", "put", "--file", &credential],
    ));
    assert_eq!(again["created_at"], created["created_at"]);
    assert_eq!(
        ok_json(&gateway(
            &endpoint,
            &token,
            &["credential", "get", "primary"]
        )),
        again
    );

    let deployment = document(
        dir.path(),
        "deployment.json",
        &json!({
            "name": "primary",
            "adapter": "openai",
            "auth": {"bearer": {"credential": "primary"}},
            "capabilities": ["chat_completions"],
        }),
    );
    ok_json(&gateway(
        &endpoint,
        &token,
        &["deployment", "put", "--file", &deployment],
    ));
    let listed = ok_json(&gateway(&endpoint, &token, &["deployment", "list"]));
    assert_eq!(listed.as_array().map(Vec::len), Some(1));

    let conflict = problem(&gateway(
        &endpoint,
        &token,
        &["credential", "delete", "primary"],
    ));
    assert_eq!(conflict["status"], 409, "{conflict}");

    ok_json(&gateway(
        &endpoint,
        &token,
        &["deployment", "delete", "primary"],
    ));
    ok_json(&gateway(
        &endpoint,
        &token,
        &["credential", "delete", "primary"],
    ));
    ok_json(&gateway(
        &endpoint,
        &token,
        &["credential", "delete", "primary"],
    ));
    let missing = problem(&gateway(
        &endpoint,
        &token,
        &["credential", "get", "primary"],
    ));
    assert_eq!(missing["status"], 404, "{missing}");

    let capture = document(
        dir.path(),
        "capture.json",
        &json!({"mode": "metadata", "payload_fields": []}),
    );
    let written = ok_json(&gateway(
        &endpoint,
        &token,
        &["capture-policy", "put", "--file", &capture],
    ));
    assert_eq!(
        ok_json(&gateway(&endpoint, &token, &["capture-policy", "get"])),
        written
    );

    // A managed secret is submitted on stdin, rotated the same way, and is
    // never echoed by the command or the server's answer.
    let submission = |secret: &str| {
        json!({
            "name": "managed",
            "provider": "openai",
            "source": {"managed_secret": {"secret": secret}},
        })
        .to_string()
    };
    let submitted = gateway_stdin(
        &endpoint,
        &token,
        &["credential", "put", "--file", "-"],
        &submission(SUBMITTED),
    );
    let created = ok_json(&submitted);
    assert_eq!(created["source"], "managed_secret");
    assert_eq!(created["state"], "active");
    let rotated = gateway_stdin(
        &endpoint,
        &token,
        &["credential", "put", "--file", "-"],
        &submission(ROTATED),
    );
    let rotated_view = ok_json(&rotated);
    assert_eq!(rotated_view["created_at"], created["created_at"]);
    assert_ne!(rotated_view["rotated_at"], created["rotated_at"]);
    assert_eq!(
        ok_json(&gateway(
            &endpoint,
            &token,
            &["credential", "get", "managed"]
        ))["source"],
        "managed_secret"
    );
    for (output, secret) in [(&submitted, SUBMITTED), (&rotated, ROTATED)] {
        let diagnostics = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !diagnostics.contains(secret),
            "neither stream repeats the submitted value: {diagnostics}"
        );
    }

    let mistyped = gateway_stdin(
        &endpoint,
        &token,
        &["credential", "put", "--file", "-"],
        &json!({
            "name": "managed",
            "provider": "openai",
            "source": {"managed_secret": MISTYPED},
        })
        .to_string(),
    );
    assert!(!mistyped.status.success(), "a mistyped body is refused");
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&mistyped.stdout),
        String::from_utf8_lossy(&mistyped.stderr)
    );
    assert!(
        !diagnostics.contains(MISTYPED),
        "no stream repeats the submitted value: {diagnostics}"
    );
    assert!(diagnostics.contains("--file"), "{diagnostics}");
    assert!(
        diagnostics.contains("decode failed at line"),
        "{diagnostics}"
    );

    let reader = server
        .bootstrap_user("cli-gateway-reader", &[])
        .await
        .expect("reader bootstraps");
    let denied = problem(&gateway(
        &endpoint,
        reader.jwt().expect("reader jwt"),
        &["credential", "list"],
    ));
    assert_eq!(denied["status"], 403, "{denied}");
    server.shutdown().await.expect("test server shuts down");
}

/// One `deployment put` reads a single deployment or a list: a malformed,
/// empty, duplicated, or invalid list writes nothing; a server refusal stops
/// the run with ordered completed/failed/unattempted outcomes and earlier
/// writes kept; rerunning converges without deleting unnamed deployments; and
/// standard input carries a list the same way.
///
/// # Panics
/// Panics when the test server, a CLI run, or an assertion fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the serialized Postgres-backed CLI journey lane"]
async fn deployment_file_accepts_one_or_many() {
    let server = WyrdTestServer::start_bound()
        .await
        .expect("test server starts");
    let endpoint = server.base_url().expect("server is bound").to_owned();
    let admin = server
        .bootstrap_user("cli-deployment-admin", &["admin"])
        .await
        .expect("admin bootstraps");
    let token = admin.jwt().expect("admin jwt").to_owned();
    let dir = tempfile::tempdir().expect("tempdir");
    let credential = document(
        dir.path(),
        "credential.json",
        &json!({
            "name": "openai-key",
            "provider": "openai",
            "source": {"environment": {"binding": TEST_GATEWAY_CREDENTIAL_BINDING}},
        }),
    );
    ok_json(&gateway(
        &endpoint,
        &token,
        &["credential", "put", "--file", &credential],
    ));
    let entry = |name: &str, credential: &str| json!({"name": name, "adapter": "openai", "auth": {"bearer": {"credential": credential}}});
    let put = |file: &str| gateway(&endpoint, &token, &["deployment", "put", "--file", file]);
    let names = || {
        let mut names: Vec<String> = ok_json(&gateway(&endpoint, &token, &["deployment", "list"]))
            .as_array()
            .expect("deployments list")
            .iter()
            .map(|deployment| deployment["name"].as_str().expect("name").to_owned())
            .collect();
        names.sort();
        names
    };

    let invalid = json!({"name": "bad", "adapter": "openai", "auth": "none", "models": []});
    for (file, refused, expected) in [
        ("empty.json", json!([]), "a nonempty list of deployments"),
        (
            "duplicate.json",
            json!([entry("openai", "openai-key"), entry("openai", "openai-key")]),
            "[1] repeats an earlier name",
        ),
        (
            "invalid.json",
            json!([entry("openai", "openai-key"), invalid]),
            "[1].gateway field models is invalid",
        ),
    ] {
        let problem = problem(&put(&document(dir.path(), file, &refused)));
        assert_eq!(problem["status"], 400, "{problem}");
        assert!(problem.to_string().contains(expected), "{file}: {problem}");
    }
    assert!(names().is_empty(), "a refused file writes no entry");

    let partial = put(&document(
        dir.path(),
        "partial.json",
        &json!([
            entry("openai", "openai-key"),
            entry("missing", "absent-key"),
            entry("later", "openai-key"),
        ]),
    ));
    let refusal = problem(&partial);
    assert_eq!(
        serde_json::from_slice::<Value>(&partial.stdout).expect("stdout is the outcome list"),
        json!([
            {"name": "openai", "status": "completed"},
            {"name": "missing", "status": "failed", "code": refusal["code"]},
            {"name": "later", "status": "unattempted"},
        ])
    );
    assert_eq!(names(), ["openai"], "earlier writes stay committed");

    let fixed = document(
        dir.path(),
        "fixed.json",
        &json!([
            entry("openai", "openai-key"),
            entry("missing", "openai-key"),
            entry("later", "openai-key"),
        ]),
    );
    for _ in 0..2 {
        assert_eq!(
            ok_json(&put(&fixed)),
            json!([
                {"name": "openai", "status": "completed"},
                {"name": "missing", "status": "completed"},
                {"name": "later", "status": "completed"},
            ])
        );
        assert_eq!(names(), ["later", "missing", "openai"], "reruns converge");
    }

    let single = ok_json(&put(&document(
        dir.path(),
        "single.json",
        &entry("openai", "openai-key"),
    )));
    assert_eq!(
        single["name"], "openai",
        "one object answers with the deployment"
    );
    let piped = gateway_stdin(
        &endpoint,
        &token,
        &["deployment", "put", "--file", "-"],
        &json!([entry("piped", "openai-key")]).to_string(),
    );
    assert_eq!(
        ok_json(&piped),
        json!([{"name": "piped", "status": "completed"}])
    );
    assert_eq!(
        names(),
        ["later", "missing", "openai", "piped"],
        "names absent from a file are never deleted"
    );
    server.shutdown().await.expect("test server shuts down");
}

/// A composite document naming a local environment variable applies the
/// credential and its deployments in one request: unresolvable or malformed
/// documents are refused before any request, a server refusal of a later
/// deployment commits nothing, a valid document commits together with only
/// redacted output, and reapplying converges.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the serialized Postgres-backed CLI journey lane"]
async fn deployment_document_applies_local_environment() {
    const VARIABLE: &str = "WYRD_JOURNEY_OPENAI_KEY";
    const SECRET: &str = "sk-live-local-env-sentinel";
    let server = WyrdTestServer::start_bound()
        .await
        .expect("test server starts");
    let endpoint = server.base_url().expect("server is bound").to_owned();
    let admin = server
        .bootstrap_user("cli-apply-admin", &["admin"])
        .await
        .expect("admin bootstraps");
    let token = admin.jwt().expect("admin jwt").to_owned();
    let dir = tempfile::tempdir().expect("tempdir");
    let entry = |name: &str, credential: &str| json!({"name": name, "adapter": "openai", "auth": {"bearer": {"credential": credential}}});
    let local = |variable: &str| json!({"name": "openai-key", "provider": "openai", "source": {"local_env": variable}});
    let apply = |file: &str, vars: &[(&str, &str)]| {
        gateway_env(
            &endpoint,
            &token,
            &["deployment", "put", "--file", file],
            vars,
        )
    };
    let listed = |resource: &str| {
        let mut names: Vec<String> = ok_json(&gateway(&endpoint, &token, &[resource, "list"]))
            .as_array()
            .expect("list")
            .iter()
            .map(|item| item["name"].as_str().expect("name").to_owned())
            .collect();
        names.sort();
        names
    };
    let quiet = |output: &Output| {
        let rendered = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !rendered.contains(SECRET),
            "output discloses the key: {rendered}"
        );
    };

    for (file, refused, vars, expected) in [
        (
            "unset.json",
            json!({"credentials": [local(VARIABLE)], "deployments": [entry("openai", "openai-key")]}),
            &[][..],
            "unset or empty environment variable",
        ),
        (
            "blank.json",
            json!({"credentials": [local(VARIABLE)], "deployments": [entry("openai", "openai-key")]}),
            &[(VARIABLE, "  ")][..],
            "unset or empty environment variable",
        ),
        (
            "shell.json",
            json!({"credentials": [local("$(cat key)")], "deployments": [entry("openai", "openai-key")]}),
            &[(VARIABLE, SECRET)][..],
            "must be an environment variable name",
        ),
        (
            "unused.json",
            json!({"credentials": [local(VARIABLE)], "deployments": [entry("openai", "other-key")]}),
            &[(VARIABLE, SECRET)][..],
            "not referenced by a submitted deployment",
        ),
        (
            "empty.json",
            json!({"deployments": []}),
            &[][..],
            "must list at least one deployment",
        ),
    ] {
        let output = apply(&document(dir.path(), file, &refused), vars);
        quiet(&output);
        let problem = problem(&output);
        assert_eq!(problem["status"], 400, "{file}: {problem}");
        assert!(problem.to_string().contains(expected), "{file}: {problem}");
    }
    assert!(
        listed("credential").is_empty() && listed("deployment").is_empty(),
        "local refusals send nothing"
    );

    let output = apply(
        &document(
            dir.path(),
            "partial.json",
            &json!({
                "credentials": [local(VARIABLE)],
                "deployments": [entry("openai", "openai-key"), entry("missing", "absent-key")],
            }),
        ),
        &[(VARIABLE, SECRET)],
    );
    quiet(&output);
    assert_eq!(
        problem(&output)["code"],
        "WYRD_GATEWAY_400_INVALID_CONFIGURATION"
    );
    assert!(
        listed("credential").is_empty() && listed("deployment").is_empty(),
        "a refused document commits nothing"
    );

    let file = document(
        dir.path(),
        "gateway.json",
        &json!({
            "credentials": [local(VARIABLE)],
            "deployments": [entry("openai", "openai-key"), entry("openai-backup", "openai-key")],
        }),
    );
    assert!(
        !std::fs::read_to_string(&file)
            .expect("document")
            .contains(SECRET)
    );
    for _ in 0..2 {
        let output = apply(&file, &[(VARIABLE, SECRET)]);
        quiet(&output);
        let applied = ok_json(&output);
        assert_eq!(
            applied["credentials"][0]["source"], "managed_secret",
            "{applied}"
        );
        assert_eq!(applied["deployments"][1]["name"], "openai-backup");
        assert_eq!(listed("credential"), ["openai-key"]);
        assert_eq!(
            listed("deployment"),
            ["openai", "openai-backup"],
            "reapplying converges"
        );
    }
    server.shutdown().await.expect("test server shuts down");
}
