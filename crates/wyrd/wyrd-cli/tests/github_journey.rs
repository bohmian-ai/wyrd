//! Real-server CLI journey for a tenant's GitHub connection.
//!
//! The tenant guide connects GitHub with `wyrd github connect`, opens the
//! printed URL in a browser signed in to Wyrd, then checks and releases the
//! connection with `wyrd github status` and `wyrd github disconnect`. This
//! runs the guide's own connect and status shell blocks, with its example
//! address and credential placeholder filled in, and drives the remaining
//! commands against a bound server configured with the GitHub App mock.

/// Real-server and PostgreSQL cases, placed for the `pg-servers` test group.
mod pg_tests {
    use std::process::Output;

    use secrecy::ExposeSecret as _;
    use serde_json::Value;
    use wyrd_testing::WyrdTestServer;
    use wyrd_testing::github::{GithubAppMock, GithubBrowser};

    use crate::principal_journey::{run_cli_with_env, run_scrubbed};

    /// Run `wyrd github <verb>` as the guide does: the server from
    /// `WYRD_SERVER_URL` and `key` from `WYRD_API_KEY`.
    ///
    /// # Panics
    /// Panics when the CLI cannot run or its subprocess cannot join.
    async fn github(verb: &str, base: &str, key: &str) -> Output {
        let (verb, base, key) = (verb.to_owned(), base.to_owned(), key.to_owned());
        tokio::task::spawn_blocking(move || {
            run_cli_with_env(
                &["github", &verb],
                &[("WYRD_SERVER_URL", &base), ("WYRD_API_KEY", &key)],
            )
        })
        .await
        .expect("CLI subprocess joins")
    }

    /// The "Connect GitHub" guide, which embeds the `examples/connect-github`
    /// scripts.
    const GUIDE: &str = include_str!("../../../../docs/src/content/docs/operate/connect-github.md");

    /// Run the guide's embedded `examples/connect-github/<name>` script as a
    /// reader would: the shipped `wyrd` on `PATH`, `base` in place of the
    /// example server address, `key` in place of `<admin_credential>`, and
    /// `carried` set as exports left by earlier steps.
    ///
    /// # Panics
    /// Panics when the guide does not embed the script, `bash` cannot run, or
    /// the script as published, placeholders included and `wyrd` stubbed,
    /// fails.
    async fn guide_step(
        name: &str,
        published: &str,
        base: &str,
        key: &str,
        carried: &[(&str, &str)],
    ) -> Output {
        assert!(
            GUIDE.contains(&format!(
                "<CodeFromFile file=\"examples/connect-github/{name}\" lang=\"bash\" />"
            )),
            "the guide embeds {name}"
        );
        let literal = std::process::Command::new("bash")
            .args([
                "-euo",
                "pipefail",
                "-c",
                &format!("wyrd() {{ :; }}\n{published}"),
            ])
            .env_clear()
            .output()
            .expect("bash runs the published step");
        assert!(
            literal.status.success(),
            "{name} runs as published: {literal:?}"
        );
        let script = published
            .replace("https://wyrd.example.com", base)
            .replace("<admin_credential>", key);
        let carried: Vec<(String, String)> = carried
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        tokio::task::spawn_blocking(move || {
            let wyrd = assert_cmd::cargo::cargo_bin("wyrd");
            let ambient = std::env::var_os("PATH").unwrap_or_default();
            let path = std::env::join_paths(
                std::iter::once(
                    wyrd.parent()
                        .expect("the binary has a directory")
                        .to_path_buf(),
                )
                .chain(std::env::split_paths(&ambient)),
            )
            .expect("PATH joins");
            let mut bash = std::process::Command::new("bash");
            bash.args(["-euo", "pipefail", "-c", &script])
                .env("PATH", path);
            let carried: Vec<(&str, &str)> = carried
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect();
            run_scrubbed(bash, &carried)
        })
        .await
        .expect("guide step joins")
    }

    /// Bootstrap Service `name` holding `roles` in the fixture tenant and return
    /// its API key.
    ///
    /// # Panics
    /// Panics when the principal cannot be bootstrapped or holds no API key.
    async fn key(server: &WyrdTestServer, name: &str, roles: &[&str]) -> String {
        server
            .bootstrap_service_in_tenant(server.data_tenant_id(), name, roles)
            .await
            .expect("principal bootstraps")
            .api_key()
            .expect("a machine principal")
            .expose_secret()
            .to_owned()
    }

    /// Fail unless `output` succeeded, and return its JSON stdout.
    ///
    /// # Panics
    /// Panics when the command failed or printed no JSON.
    fn json(what: &str, output: &Output) -> Value {
        assert!(
            output.status.success(),
            "{what} failed: stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("the command prints JSON")
    }

    /// Fail unless `output` failed with the stable error `code`.
    ///
    /// # Panics
    /// Panics when the command succeeded or failed with another code.
    fn failed_with(what: &str, output: &Output, code: &str) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success() && stderr.contains(code),
            "{what} must fail with {code}: stderr={stderr}"
        );
    }

    /// A tenant administrator connects, inspects and releases GitHub through the
    /// shipped binary; a viewer is refused every verb.
    ///
    /// # Panics
    /// Panics when a command fails, prints an unexpected view, leaks a secret,
    /// or is not refused with its stable code.
    #[tokio::test]
    async fn tenant_connects_github_through_the_cli() {
        let mock = GithubAppMock::start().expect("the GitHub mock starts");
        mock.add_installation(101, &[(1, "acme/api"), (2, "acme/web")]);
        mock.add_user("octo", &[101]);
        mock.sign_in("octo");
        mock.install_target(101);
        let server = WyrdTestServer::builder()
            .with_github_app_for_test(&mock)
            .start_bound()
            .await
            .expect("test server starts");
        let base = server.base_url().expect("bound").to_owned();
        let admin = key(&server, "github_cli_admin", &["admin"]).await;
        let viewer = key(&server, "github_cli_viewer", &["viewer"]).await;

        let before = json("status", &github("status", &base, &admin).await);
        assert_eq!(before["state"], "not_connected");

        let receipt = json(
            "the guide's connect step",
            &guide_step(
                "connect.sh",
                include_str!("../../../../examples/connect-github/connect.sh"),
                &base,
                &admin,
                &[],
            )
            .await,
        );
        let url = receipt["authorization_url"]
            .as_str()
            .expect("connect prints the install URL");
        assert!(receipt["expires_at"].is_string(), "{receipt}");
        let mut browser = GithubBrowser::sign_in(&base, server.pg_fixture().tenant_slug(), &admin)
            .await
            .expect("browser signs in");
        let answer = browser.open(url).await.expect("browser navigates");
        assert_eq!(answer.status, 200, "{:?}", answer.body);

        let exported = [
            ("WYRD_SERVER_URL", base.as_str()),
            ("WYRD_API_KEY", admin.as_str()),
        ];
        let status = json(
            "the guide's status step",
            &guide_step(
                "status.sh",
                include_str!("../../../../examples/connect-github/status.sh"),
                &base,
                &admin,
                &exported,
            )
            .await,
        );
        assert_eq!(status["state"], "active");
        assert_eq!(status["installation_id"], 101);
        assert_eq!(
            status["repositories"],
            serde_json::json!([
                { "repository_id": "1", "full_name": "acme/api" },
                { "repository_id": "2", "full_name": "acme/web" },
            ])
        );

        for verb in ["connect", "status", "disconnect"] {
            failed_with(
                verb,
                &github(verb, &base, &viewer).await,
                "WYRD_PERMISSION_403_DENIED_RBAC",
            );
        }

        let released = json("disconnect", &github("disconnect", &base, &admin).await);
        assert_eq!(released["state"], "not_connected");
        assert_eq!(released["installation_id"], Value::Null);
        failed_with(
            "second disconnect",
            &github("disconnect", &base, &admin).await,
            "WYRD_GITHUB_404_NOT_CONNECTED",
        );

        let printed = format!("{receipt}{status}{released}");
        for secret in mock.secrets() {
            assert!(!printed.contains(&secret), "secret leaked: {printed}");
        }
        server.shutdown().await.expect("test server shuts down");
    }
}
