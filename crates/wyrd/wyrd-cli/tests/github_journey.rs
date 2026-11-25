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
    use wyrd_testing::github::{GithubAppMock, GithubBrowser, PullRequest};

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

    /// Run `wyrd <arguments>` with the server from `WYRD_SERVER_URL` and `key`
    /// from `WYRD_API_KEY`.
    ///
    /// # Panics
    /// Panics when the CLI cannot run or its subprocess cannot join.
    async fn wyrd(arguments: &[&str], base: &str, key: &str) -> Output {
        let arguments: Vec<String> = arguments.iter().map(ToString::to_string).collect();
        let (base, key) = (base.to_owned(), key.to_owned());
        tokio::task::spawn_blocking(move || {
            let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
            run_cli_with_env(
                &arguments,
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
    /// shipped binary; a viewer is refused every verb. A pull request on a
    /// declared repository admits an execution `wyrd execution status` shows
    /// pinned to its base and head, and disconnecting cancels it with
    /// `repository_access_lost`, pins kept.
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

        let declared = json(
            "declare",
            &wyrd(
                &[
                    "declare",
                    concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../../fixtures/cards/repository_declaration/repository.yaml"
                    ),
                    "--format",
                    "json",
                ],
                &base,
                &admin,
            )
            .await,
        );
        let uid = declared["root"]["uid"]
            .as_str()
            .expect("the root has a uid")
            .to_owned();
        let card = json(
            "get",
            &wyrd(
                &[
                    "get",
                    "--kind",
                    "Repository",
                    "--uid",
                    &uid,
                    "--format",
                    "json",
                ],
                &base,
                &admin,
            )
            .await,
        );
        let binding = card["status"]["verification"]["binding_ids"][0]
            .as_str()
            .expect("the Repository serves its binding")
            .to_owned();
        mock.set_pull_request(2, 7, PullRequest::open(2, "b1", "a1"));
        let opened = mock.pull_request_event(101, 2, 7, "opened");
        assert_eq!(
            mock.deliver(&base, "pull_request", &opened)
                .await
                .expect("delivery sends"),
            202
        );
        let token = server
            .exchange_api_key(&admin.clone().into())
            .await
            .expect("the key exchanges");
        let status: Value = reqwest::Client::new()
            .get(format!("{base}/v1/verification/bindings/{binding}"))
            .header("x-wyrd-access-token", format!("Bearer {token}"))
            .send()
            .await
            .expect("binding reads")
            .json()
            .await
            .expect("binding is JSON");
        let execution = status["last_execution_id"]
            .as_str()
            .expect("the pull request admitted an execution")
            .to_owned();
        let inspect = async || {
            json(
                "execution status",
                &wyrd(
                    &["execution", "status", "--execution-id", &execution],
                    &base,
                    &admin,
                )
                .await,
            )
        };
        let admitted = inspect().await;
        assert_eq!(admitted["status"], "pending");
        assert_eq!(admitted["pull_request"]["number"], 7);
        assert_eq!(admitted["pull_request"]["repository_id"], "2");
        assert_eq!(
            admitted["pull_request"]["base"],
            serde_json::json!({ "ref": "main", "sha": "b1" })
        );
        assert_eq!(admitted["pull_request"]["head"]["sha"], "a1");
        assert_eq!(admitted["pull_request"]["head"]["ref"], "feature");
        assert_eq!(admitted["pull_request"]["head_repository_id"], "2");
        assert!(
            admitted["pull_request"]["delivery_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        );
        let released = json("disconnect", &github("disconnect", &base, &admin).await);
        assert_eq!(released["state"], "not_connected");
        assert_eq!(released["installation_id"], Value::Null);
        let cancelled = inspect().await;
        assert_eq!(cancelled["status"], "cancelled");
        assert_eq!(cancelled["error"]["code"], "repository_access_lost");
        assert_eq!(cancelled["pull_request"]["head"]["sha"], "a1");
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

    /// A bound deployment whose tenant connected installation 101
    /// (`acme/api`, `acme/web`) and declared a Repository, with pull request
    /// 7 of `acme/web` opened at [`Waiting::HEAD`] and its execution waiting.
    struct Waiting {
        /// The running server.
        server: WyrdTestServer,
        /// The GitHub mock, kept alive with the server.
        mock: GithubAppMock,
        /// The server's base URL.
        base: String,
        /// The administrator's API key.
        admin: String,
        /// The pull request's execution.
        execution: String,
    }

    impl Waiting {
        /// The pull request's head commit.
        const HEAD: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";

        /// Connect through the CLI, declare `repository`, open the pull
        /// request, and wait until its execution waits for exactly `input`.
        ///
        /// # Panics
        /// Panics when a step fails or the execution does not wait for
        /// `input`.
        async fn open(repository: &str, input: &str) -> Self {
            let mock = GithubAppMock::start().expect("the GitHub mock starts");
            mock.add_installation(101, &[(1, "acme/api"), (2, "acme/web")]);
            mock.add_user("octo", &[101]);
            mock.sign_in("octo");
            mock.install_target(101);
            let server = WyrdTestServer::builder()
                .with_github_app_for_test(&mock)
                .with_verification_runtime_for_test()
                .start_bound()
                .await
                .expect("test server starts");
            let base = server.base_url().expect("bound").to_owned();
            let admin = key(&server, "evidence_cli_admin", &["admin"]).await;
            let receipt = json("connect", &github("connect", &base, &admin).await);
            let mut browser =
                GithubBrowser::sign_in(&base, server.pg_fixture().tenant_slug(), &admin)
                    .await
                    .expect("browser signs in");
            let url = receipt["authorization_url"]
                .as_str()
                .expect("connect prints the install URL");
            assert_eq!(
                browser.open(url).await.expect("browser navigates").status,
                200
            );
            let declared = json(
                "declare",
                &wyrd(&["declare", repository, "--format", "json"], &base, &admin).await,
            );
            let uid = declared["root"]["uid"]
                .as_str()
                .expect("the root has a uid");
            let card = json(
                "get",
                &wyrd(
                    &[
                        "get",
                        "--kind",
                        "Repository",
                        "--uid",
                        uid,
                        "--format",
                        "json",
                    ],
                    &base,
                    &admin,
                )
                .await,
            );
            let binding = card["status"]["verification"]["binding_ids"][0]
                .as_str()
                .expect("the Repository serves its binding")
                .to_owned();
            mock.set_pull_request(2, 7, PullRequest::open(2, &"b1".repeat(20), Self::HEAD));
            let opened = mock.pull_request_event(101, 2, 7, "opened");
            assert_eq!(
                mock.deliver(&base, "pull_request", &opened)
                    .await
                    .expect("delivery sends"),
                202
            );
            let token = server
                .exchange_api_key(&admin.clone().into())
                .await
                .expect("the key exchanges");
            let status: Value = reqwest::Client::new()
                .get(format!("{base}/v1/verification/bindings/{binding}"))
                .header("x-wyrd-access-token", format!("Bearer {token}"))
                .send()
                .await
                .expect("binding reads")
                .json()
                .await
                .expect("binding is JSON");
            let execution = status["last_execution_id"]
                .as_str()
                .expect("the pull request admitted an execution")
                .to_owned();
            let waiting = Self {
                server,
                mock,
                base,
                admin,
                execution,
            };
            assert_eq!(
                waiting.until("waiting").await["waiting"],
                serde_json::json!([input])
            );
            waiting
        }

        /// Poll `wyrd execution status` until the execution is `wanted`, for
        /// at most 30 seconds, and return it.
        ///
        /// # Panics
        /// Panics when it does not reach `wanted` in time.
        async fn until(&self, wanted: &str) -> Value {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            loop {
                let state = json(
                    "execution status",
                    &wyrd(
                        &["execution", "status", "--execution-id", &self.execution],
                        &self.base,
                        &self.admin,
                    )
                    .await,
                );
                if state["status"] == wanted {
                    return state;
                }
                assert!(std::time::Instant::now() < deadline, "{state}");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }

        /// The execution's `github_delivery` as the MCP tool
        /// `verification.get_execution` serves it, once it is no longer
        /// `pending`, polled for at most 30 seconds.
        ///
        /// # Panics
        /// Panics when the call fails or delivery stays pending.
        async fn delivered(&self) -> Value {
            let token = self
                .server
                .exchange_api_key(&self.admin.clone().into())
                .await
                .expect("the key exchanges");
            let version = "2026-07-28";
            let body = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "verification.get_execution",
                    "arguments": {"execution_id": self.execution},
                    "_meta": {
                        "io.modelcontextprotocol/protocolVersion": version,
                        "io.modelcontextprotocol/clientInfo": {"name": "journey", "version": "1"},
                        "io.modelcontextprotocol/clientCapabilities": {},
                    },
                },
            });
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            loop {
                let text = reqwest::Client::new()
                    .post(format!("{}/mcp", self.base))
                    .header("x-wyrd-access-token", format!("Bearer {token}"))
                    .header("accept", "application/json, text/event-stream")
                    .header("mcp-protocol-version", version)
                    .header("mcp-method", "tools/call")
                    .header("mcp-name", "verification.get_execution")
                    .json(&body)
                    .send()
                    .await
                    .expect("MCP responds")
                    .text()
                    .await
                    .expect("MCP answers");
                let message = text
                    .lines()
                    .find_map(|line| line.strip_prefix("data:"))
                    .unwrap_or(&text);
                let answer: Value = serde_json::from_str(message.trim()).expect("JSON-RPC answer");
                let delivery = answer["result"]["structuredContent"]["github_delivery"].clone();
                if delivery["status"] != "pending" && !delivery.is_null() {
                    return delivery;
                }
                assert!(std::time::Instant::now() < deadline, "{answer}");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }

        /// Run `command` in `dir` as a CI job would: the server and key
        /// from `WYRD_SERVER_URL`/`WYRD_API_KEY`, the repository from
        /// `WYRD_REPOSITORY`, and the commit from `WYRD_COMMIT` when given.
        ///
        /// # Panics
        /// Panics when the subprocess cannot run or join.
        async fn ci(
            &self,
            mut command: std::process::Command,
            dir: &std::path::Path,
            commit: Option<&str>,
        ) -> Output {
            command.current_dir(dir);
            let mut sources = vec![
                ("WYRD_SERVER_URL", self.base.clone()),
                ("WYRD_API_KEY", self.admin.clone()),
                ("WYRD_REPOSITORY", "acme/web".to_owned()),
            ];
            if let Some(commit) = commit {
                sources.push(("WYRD_COMMIT", commit.to_owned()));
            } else {
                command.env_remove("WYRD_COMMIT");
            }
            tokio::task::spawn_blocking(move || {
                let sources: Vec<(&str, &str)> = sources
                    .iter()
                    .map(|(name, value)| (*name, value.as_str()))
                    .collect();
                run_scrubbed(command, &sources)
            })
            .await
            .expect("CLI subprocess joins")
        }
    }

    /// CI publishes a deferred input with `wyrd evidence publish`, naming the
    /// repository and commit only through `WYRD_REPOSITORY` and `WYRD_COMMIT`
    /// from a directory that is no checkout: the waiting pull request
    /// execution resumes and passes, the same publication replays, and a
    /// missing commit is a local error naming its flag and variable.
    ///
    /// # Panics
    /// Panics when a command fails, the execution does not wait and resume,
    /// or a receipt or refusal differs.
    #[tokio::test]
    async fn ci_publishes_deferred_evidence_through_the_cli() {
        let waiting = Waiting::open(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../fixtures/cards/deferred_evidence/repository.yaml"
            ),
            "ci",
        )
        .await;
        let ci = tempfile::tempdir().expect("a directory that is no checkout");
        std::fs::write(ci.path().join("results.json"), r#"{"status": "green"}"#)
            .expect("results write");
        let publish = |commit: Option<&'static str>| {
            let mut command = std::process::Command::new(assert_cmd::cargo::cargo_bin("wyrd"));
            command.args(["evidence", "publish", "ci", "--file", "results.json"]);
            waiting.ci(command, ci.path(), commit)
        };
        let accepted = json("evidence publish", &publish(Some(Waiting::HEAD)).await);
        assert_eq!(accepted["status"], "accepted");
        assert_eq!(accepted["matched_subscriptions"], 1);
        assert_eq!(accepted["tags"]["commit"], Waiting::HEAD);
        assert_eq!(waiting.until("completed").await["status"], "completed");
        let replayed = json(
            "evidence publish again",
            &publish(Some(Waiting::HEAD)).await,
        );
        assert_eq!(replayed["status"], "replayed");
        assert_eq!(replayed["evidence_id"], accepted["evidence_id"]);
        let missing = publish(None).await;
        failed_with(
            "evidence publish without a commit",
            &missing,
            "WYRD_VERIFICATION_400_INPUT_INVALID",
        );
        assert!(
            String::from_utf8_lossy(&missing.stderr).contains("WYRD_COMMIT"),
            "the refusal names the variable: {missing:?}"
        );
        waiting
            .server
            .shutdown()
            .await
            .expect("test server shuts down");
    }

    /// The guide's `examples/publish-deferred-evidence/publish.sh`, run as
    /// published from the example directory, uploads `report.csv` for the
    /// waiting `report` input, and the execution reads it and passes.
    ///
    /// # Panics
    /// Panics when the guide does not embed the script, the script fails, or
    /// the execution does not pass.
    #[tokio::test]
    async fn ci_uploads_a_report_file_through_the_guide_script() {
        const DIR: &str = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../examples/publish-deferred-evidence"
        );
        assert!(
            include_str!(
                "../../../../docs/src/content/docs/build-and-verify/verify/publish-deferred-evidence.md"
            )
            .contains(
                "<CodeFromFile file=\"examples/publish-deferred-evidence/publish.sh\" lang=\"bash\" />"
            ),
            "the guide embeds publish.sh"
        );
        let waiting = Waiting::open(&format!("{DIR}/repository.yaml"), "report").await;
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
        bash.args(["-euo", "pipefail", "publish.sh"])
            .env("PATH", path);
        let accepted = json(
            "publish.sh",
            &waiting
                .ci(bash, std::path::Path::new(DIR), Some(Waiting::HEAD))
                .await,
        );
        assert_eq!(accepted["status"], "accepted");
        assert_eq!(accepted["input"], "report");
        let completed = waiting.until("completed").await;
        assert_eq!(completed["status"], "completed", "{completed}");
        waiting
            .server
            .shutdown()
            .await
            .expect("test server shuts down");
    }

    /// A person approves a waiting pull request's pending signoff with
    /// `wyrd execution approve` and the same execution passes.
    ///
    /// `wyrd execution status` lists the pending `signoff` occurrence. Once
    /// CI publishes `ci`, a viewer's approval is refused; an approver's prints
    /// the durable receipt and replays, and a later rejection of the decided
    /// occurrence is refused as not pending. The passed execution then
    /// reaches GitHub as one `success` Check on the head commit, and the MCP
    /// tool `verification.get_execution` reads its delivery as `delivered`.
    ///
    /// # Panics
    /// Panics when a command fails, the occurrence is not pending, a
    /// receipt or refusal differs, or the execution does not pass.
    #[tokio::test]
    async fn a_person_approves_a_pending_signoff_through_the_cli() {
        let waiting = Waiting::open(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../fixtures/cards/approvals/repository.yaml"
            ),
            "ci",
        )
        .await;
        let state = waiting.until("waiting").await;
        let pending = &state["approvals"][0];
        assert_eq!(pending["step_id"], "signoff");
        assert_eq!(pending["status"], "pending");
        let approval = pending["approval_id"]
            .as_str()
            .expect("the occurrence has an approval_id")
            .to_owned();
        let ci = tempfile::tempdir().expect("a CI directory");
        std::fs::write(ci.path().join("results.json"), r#"{"status": "green"}"#)
            .expect("results write");
        let mut publish = std::process::Command::new(assert_cmd::cargo::cargo_bin("wyrd"));
        publish.args(["evidence", "publish", "ci", "--file", "results.json"]);
        json(
            "evidence publish",
            &waiting.ci(publish, ci.path(), Some(Waiting::HEAD)).await,
        );

        let decide = |verb: &'static str, key: String| {
            let arguments = [
                "execution",
                verb,
                "--execution-id",
                waiting.execution.as_str(),
                "--approval-id",
                approval.as_str(),
                "--reason",
                "reviewed the diff",
            ];
            let base = waiting.base.clone();
            async move { wyrd(&arguments, &base, &key).await }
        };
        let viewer = key(&waiting.server, "signoff_cli_viewer", &["viewer"]).await;
        failed_with(
            "a viewer's approval",
            &decide("approve", viewer).await,
            "WYRD_PERMISSION_403_DENIED_RBAC",
        );
        let approver = key(
            &waiting.server,
            "signoff_cli_approver",
            &["approver", "viewer"],
        )
        .await;
        let receipt = json(
            "execution approve",
            &decide("approve", approver.clone()).await,
        );
        assert_eq!(receipt["approval_id"], approval.as_str());
        assert_eq!(receipt["status"], "approved");
        assert_eq!(receipt["reason"], "reviewed the diff");
        assert_eq!(
            json(
                "execution approve again",
                &decide("approve", approver.clone()).await
            ),
            receipt,
            "the same decision replays its receipt"
        );
        failed_with(
            "a rejection after the approval",
            &decide("reject", approver).await,
            "WYRD_VERIFICATION_409_APPROVAL_NOT_PENDING",
        );
        let completed = waiting.until("completed").await;
        assert_eq!(
            completed["approvals"][0]["status"], "approved",
            "{completed}"
        );
        let delivery = waiting.delivered().await;
        assert_eq!(delivery, serde_json::json!({"status": "delivered"}));
        let checks = waiting.mock.check_runs(2);
        assert_eq!(checks.len(), 1, "one Check for the execution: {checks:#?}");
        assert_eq!(
            (
                &checks[0]["name"],
                &checks[0]["head_sha"],
                &checks[0]["conclusion"]
            ),
            (
                &serde_json::json!("wyrd/signoff"),
                &serde_json::json!(Waiting::HEAD),
                &serde_json::json!("success")
            ),
            "{checks:#?}"
        );
        assert_eq!(checks[0]["external_id"], waiting.execution.as_str());
        waiting
            .server
            .shutdown()
            .await
            .expect("test server shuts down");
    }

    /// The "Approve a pull request" guide shows the Cards and the approve
    /// command that [`a_person_approves_a_pending_signoff_through_the_cli`]
    /// runs.
    ///
    /// # Panics
    /// Panics when the guide does not embed a fixture or show the command.
    #[test]
    fn the_approval_guide_shows_the_tested_cards_and_command() {
        let guide = include_str!(
            "../../../../docs/src/content/docs/build-and-verify/verify/approve-a-pull-request.md"
        );
        for embedded in [
            "<CodeFromFile file=\"fixtures/cards/approvals/signoff.yaml\"",
            "<CodeFromFile file=\"fixtures/cards/approvals/repository.yaml\"",
            "wyrd execution approve --execution-id <execution_id> --approval-id <approval_id> --reason \"reviewed the diff\"",
        ] {
            assert!(guide.contains(embedded), "the guide shows {embedded}");
        }
    }
}
