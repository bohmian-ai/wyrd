//! CLI Card lifecycle journeys.

use assert_cmd::prelude::*;
use base64::Engine;
use serde_json::Value;
use sha2::Digest;
use std::process::{Command, Output};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
use wyrd_storage::settings::{BackendConfig, StorageSettings};
use wyrd_testing::WyrdTestServer;

fn run_cli(arguments: &[&str]) -> Output {
    Command::cargo_bin("wyrd")
        .expect("wyrd binary builds")
        .args(arguments)
        .output()
        .expect("wyrd command runs")
}

async fn run_cli_owned(arguments: Vec<String>) -> Output {
    tokio::task::spawn_blocking(move || {
        let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        run_cli(&arguments)
    })
    .await
    .expect("CLI subprocess joins")
}

macro_rules! run_cli_async {
    ($($argument:expr),* $(,)?) => {
        run_cli_owned(vec![$($argument.to_owned()),*]).await
    };
}

fn write_prompt(temp: &TempDir) -> std::path::PathBuf {
    let artifact = b"cli-card-artifact";
    let digest = base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(artifact));
    std::fs::write(temp.path().join("prompt.txt"), artifact).expect("artifact writes");
    let path = temp.path().join("prompt.yaml");
    std::fs::write(
        &path,
        format!(
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: cli-prompt\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-4o\n  messages: [hello]\nartifacts:\n  - relative_path: prompt.txt\n    sha256: {digest}\n    size_bytes: {}\n    content_type: text/plain\n",
            artifact.len()
        ),
    )
    .expect("prompt card writes");
    path
}

fn first_stderr_json(output: &Output) -> Value {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .next()
        .expect("CLI emits a machine-readable error first");
    serde_json::from_str(line).expect("CLI error is JSON")
}

#[test]
fn plan_is_local_and_returns_machine_readable_card_projection() {
    let temp = tempfile::tempdir().expect("tempdir creates");
    let path = write_prompt(&temp);
    let output = run_cli(&[
        "plan",
        path.to_str().expect("prompt path is UTF-8"),
        "--format",
        "json",
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("plan output is JSON");
    assert_eq!(report["ok"], true);
    assert_eq!(report["cards"][0]["kind"], "Prompt");
    assert_eq!(report["cards"][0]["name"], "cli-prompt");
    assert!(report["cards"][0].get("source_path").is_none());
}

#[test]
fn invalid_local_reference_returns_card_load_error_without_credentials() {
    let temp = tempfile::tempdir().expect("tempdir creates");
    let path = temp.path().join("missing.yaml");
    let output = run_cli(&[
        "plan",
        path.to_str().expect("prompt path is UTF-8"),
        "--format",
        "json",
    ]);

    assert_eq!(output.status.code(), Some(64));
    let report: Value = serde_json::from_slice(&output.stdout).expect("plan error is JSON");
    assert_eq!(report["ok"], false);
    assert_eq!(first_stderr_json(&output)["code"], "WYRD_CLI_400_CARD_LOAD");
}

#[test]
fn apply_rejects_an_unreadable_artifact_manifest_before_authentication() {
    let temp = tempfile::tempdir().expect("tempdir creates");
    let path = write_prompt(&temp);
    let contents = std::fs::read_to_string(&path).expect("prompt reads");
    std::fs::write(
        &path,
        contents.replace("relative_path: prompt.txt", "relative_path: missing.txt"),
    )
    .expect("invalid prompt writes");

    let output = run_cli(&[
        "apply",
        path.to_str().expect("prompt path is UTF-8"),
        "--format",
        "json",
    ]);

    assert_eq!(output.status.code(), Some(64));
    assert_eq!(first_stderr_json(&output)["code"], "WYRD_CLI_400_CARD_LOAD");
}

#[test]
fn delete_rejects_ambiguous_named_selector_before_client_construction() {
    let output = run_cli(&[
        "delete",
        "--kind",
        "Prompt",
        "--space",
        "default",
        "--name",
        "cli-prompt",
        "--format",
        "json",
    ]);

    assert_eq!(output.status.code(), Some(64));
    assert_eq!(
        first_stderr_json(&output)["code"],
        "WYRD_CLI_400_DELETE_SELECTOR_EXACT"
    );
}

#[cfg(test)]
mod pg_tests {
    use super::*;
    use wyrd_testing::Bootstrap;

    async fn start_cli_server() -> (
        WyrdTestServer,
        String,
        TempDir,
        CancellationToken,
        tokio::task::JoinHandle<()>,
    ) {
        let socket = std::net::TcpListener::bind("127.0.0.1:0").expect("CLI test listener binds");
        socket
            .set_nonblocking(true)
            .expect("CLI test listener becomes nonblocking");
        let address = socket
            .local_addr()
            .expect("CLI test listener has an address");
        let base_url = format!("http://{address}");
        let storage_root = tempfile::tempdir().expect("CLI storage root creates");
        let server = WyrdTestServer::builder()
            .with_storage_settings(StorageSettings {
                backend: BackendConfig::Local {
                    root: storage_root.path().to_path_buf(),
                },
                require_encryption: false,
                presign_ttl: std::time::Duration::from_secs(600),
                part_size_bytes: 16 * 1024 * 1024,
                multipart_threshold_bytes: 100 * 1024 * 1024,
                public_base_url: Some(base_url.clone()),
            })
            .start_in_process()
            .await
            .expect("CLI test server starts");
        let router = wyrd_server::build_router(server.state().clone());
        let listener =
            tokio::net::TcpListener::from_std(socket).expect("CLI test listener converts to Tokio");
        let shutdown = CancellationToken::new();
        let serve_shutdown = shutdown.clone();
        let serve_handle = tokio::spawn(async move {
            let _ = wyrd_server::app::serve::serve(router, listener, serve_shutdown).await;
        });
        (server, base_url, storage_root, shutdown, serve_handle)
    }

    async fn stop_cli_server(
        server: WyrdTestServer,
        shutdown: CancellationToken,
        serve_handle: tokio::task::JoinHandle<()>,
    ) {
        shutdown.cancel();
        serve_handle.await.expect("CLI test server joins");
        server.shutdown().await.expect("test server shuts down");
    }

    #[tokio::test]
    async fn card_lifecycle_cli_journey() {
        if std::env::var("WYRD_CLI_E2E").as_deref() != Ok("1") {
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir creates");
        let path = write_prompt(&temp);
        let (server, base_url, _storage_root, shutdown, serve_handle) = start_cli_server().await;
        let Bootstrap::User { jwt, .. } = server
            .bootstrap_user("cli-lifecycle-writer", &["writer"])
            .await
            .expect("writer bootstraps")
        else {
            panic!("writer bootstrap returned a non-user principal");
        };
        let Bootstrap::User {
            jwt: denied_jwt, ..
        } = server
            .bootstrap_user("cli-lifecycle-denied", &[])
            .await
            .expect("denied user bootstraps")
        else {
            panic!("denied bootstrap returned a non-user principal");
        };

        let path_string = path.to_str().expect("prompt path is UTF-8");
        let apply_arguments = vec![
            "apply".to_owned(),
            path_string.to_owned(),
            "--server".to_owned(),
            base_url.clone(),
            "--token".to_owned(),
            jwt.clone(),
            "--format".to_owned(),
            "json".to_owned(),
        ];
        let plan = run_cli_async!("plan", path_string, "--format", "json");
        assert!(
            plan.status.success(),
            "{}",
            String::from_utf8_lossy(&plan.stderr)
        );

        let denied = run_cli_async!(
            "apply",
            path_string,
            "--server",
            &base_url,
            "--token",
            &denied_jwt,
            "--format",
            "json",
        );
        assert_eq!(denied.status.code(), Some(77));
        assert_eq!(first_stderr_json(&denied)["status"], 403);

        let applied = run_cli_owned(apply_arguments).await;
        assert!(
            applied.status.success(),
            "{}",
            String::from_utf8_lossy(&applied.stderr)
        );
        let receipt: Value = serde_json::from_slice(&applied.stdout).expect("receipt is JSON");
        let uid = receipt["root"]["uid"]
            .as_str()
            .expect("receipt contains a root UID")
            .to_owned();

        let get = run_cli_async!(
            "get", "--kind", "Prompt", "--uid", &uid, "--server", &base_url, "--token", &jwt,
            "--format", "json",
        );
        assert!(
            get.status.success(),
            "{}",
            String::from_utf8_lossy(&get.stderr)
        );
        let card: Value = serde_json::from_slice(&get.stdout).expect("get output is JSON");
        assert_eq!(card["card_ref"]["uid"], uid);
        assert_eq!(card["card"]["kind"], "Prompt");

        let list = run_cli_async!(
            "list",
            "--kind",
            "Prompt",
            "--name",
            "cli-prompt",
            "--server",
            &base_url,
            "--token",
            &jwt,
            "--format",
            "json",
        );
        assert!(
            list.status.success(),
            "{}",
            String::from_utf8_lossy(&list.stderr)
        );
        let listed: Value = serde_json::from_slice(&list.stdout).expect("list output is JSON");
        assert_eq!(listed["items"].as_array().expect("list items").len(), 1);

        let latest = run_cli_async!(
            "latest",
            "--kind",
            "Prompt",
            "--space",
            "default",
            "--name",
            "cli-prompt",
            "--server",
            &base_url,
            "--token",
            &jwt,
            "--format",
            "json",
        );
        assert!(
            latest.status.success(),
            "{}",
            String::from_utf8_lossy(&latest.stderr)
        );
        let resolved: Value =
            serde_json::from_slice(&latest.stdout).expect("latest output is JSON");
        assert_eq!(resolved["card_ref"]["uid"], uid);

        let destination = temp.path().join("loaded");
        let destination_string = destination.to_str().expect("destination path is UTF-8");
        let loaded = run_cli_async!(
            "load",
            "--kind",
            "Prompt",
            "--uid",
            &uid,
            "--path",
            destination_string,
            "--server",
            &base_url,
            "--token",
            &jwt,
            "--format",
            "json",
        );
        assert!(
            loaded.status.success(),
            "{}",
            String::from_utf8_lossy(&loaded.stderr)
        );
        let loaded_json: Value =
            serde_json::from_slice(&loaded.stdout).expect("load output is JSON");
        assert_eq!(loaded_json["materialized"], true);
        assert_eq!(
            std::fs::read(destination.join("prompt.txt")).expect("artifact reads"),
            b"cli-card-artifact"
        );

        let deleted = run_cli_async!(
            "delete", "--kind", "Prompt", "--uid", &uid, "--server", &base_url, "--token", &jwt,
            "--format", "json",
        );
        assert!(
            deleted.status.success(),
            "{}",
            String::from_utf8_lossy(&deleted.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&deleted.stdout).expect("delete output is JSON")["deleted"],
            true
        );

        stop_cli_server(server, shutdown, serve_handle).await;
    }

    #[tokio::test]
    async fn apply_surfaces_backend_completion_failure() {
        if std::env::var("WYRD_CLI_E2E").as_deref() != Ok("1") {
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir creates");
        let path = write_prompt(&temp);
        let (server, base_url, _storage_root, shutdown, serve_handle) = start_cli_server().await;
        let Bootstrap::User { jwt, .. } = server
            .bootstrap_user("cli-completion-failure", &["writer"])
            .await
            .expect("writer bootstraps")
        else {
            panic!("writer bootstrap returned a non-user principal");
        };
        let superuser = server
            .pg_fixture()
            .superuser_pool()
            .await
            .expect("superuser pool opens");
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION vala.test_fail_cli_card_completion_audit()
               RETURNS trigger LANGUAGE plpgsql AS $$
               BEGIN
                 IF NEW.operation = 'card.registration.complete' THEN
                   RAISE EXCEPTION 'injected CLI completion audit failure';
                 END IF;
                 RETURN NEW;
               END;
               $$;"#,
        )
        .execute(&superuser)
        .await
        .expect("failure function installs");
        sqlx::query(
            r#"CREATE TRIGGER test_fail_cli_card_completion_audit
               BEFORE INSERT ON vala.audit_outbox
               FOR EACH ROW EXECUTE FUNCTION vala.test_fail_cli_card_completion_audit()"#,
        )
        .execute(&superuser)
        .await
        .expect("failure trigger installs");

        let output = run_cli_async!(
            "apply",
            path.to_str().expect("prompt path is UTF-8"),
            "--server",
            &base_url,
            "--token",
            &jwt,
            "--format",
            "json",
        );

        assert_eq!(
            output.status.code(),
            Some(69),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(first_stderr_json(&output)["status"], 502);
        assert_eq!(
            first_stderr_json(&output)["code"],
            "WYRD_SPEC_502_UPSTREAM_FAILURE"
        );
        stop_cli_server(server, shutdown, serve_handle).await;
    }
}
