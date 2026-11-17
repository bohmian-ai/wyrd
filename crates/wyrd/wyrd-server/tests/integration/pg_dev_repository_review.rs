//! `mise run dev:full`'s pull request review: the development harness's own
//! `scripts/dev/example-app.sh setup` and `register` against a bound server
//! with the Verifier runtime, the GitHub App mock, and the development
//! MockServer serving `tests/fixtures/mockserver/expectations.json`.
//!
//! `setup` declares the pinned wyrd-example-py checkout's reusable
//! `code-review` Verifier. `register` waits until the tenant's connection
//! covers `bohmian-ai/wyrd-example-py`, then declares its Repository through
//! the Cards API. Pull requests then run extraction, the `code-reviewer`
//! Agent against MockServer's scripted report, and the code requirement, and
//! deliver the `wyrd/code-review` Check and a `COMMENT` review to GitHub.
//!
//! Needs Docker for MockServer on its fixed development port 1080 and the
//! network for the pinned checkout; `example-app.sh prepare` builds the
//! production wheel the scripts run.

use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use secrecy::ExposeSecret as _;
use serde_json::{Value, json};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::github::{GithubAppMock, PullRequest};

use super::pg_github_pull_request_extraction::{blob, sha, tarball};
use super::pg_github_pull_requests::install;

/// The demo repository's GitHub id; the harness reads it from the connection.
const DEMO: u64 = 8_675_309;
/// Another repository of the same installation.
const OTHER: u64 = 2;
/// The connected installation.
const INSTALLATION: u64 = 101;
/// The module every pull request changes.
const MODULE: &str = "src/desk.py";
/// The Check summary of MockServer's ordinary, finding-free report.
const ORDINARY_SUMMARY: &str =
    "Scripted development review from MockServer: no findings. No model read this change.\n";
/// The module on `main`.
const MAIN: &[u8] = b"def answer(question):\n    return question\n";

/// The repository root.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// MockServer from the repository's compose file, removed on drop.
struct Fixtures {
    /// The compose project.
    project: String,
}

impl Fixtures {
    /// Start MockServer and wait until it holds every checked-in expectation.
    ///
    /// # Panics
    /// Panics when Docker cannot start it or it loads too few expectations.
    fn start() -> Self {
        let project = format!("wyrd-dev-review-{}", std::process::id());
        let fixtures = Self { project };
        let up = fixtures
            .compose()
            .args(["up", "--detach", "--wait", "mockserver"])
            .output()
            .expect("docker compose runs");
        assert!(up.status.success(), "MockServer starts: {up:?}");
        let expectations: Value = serde_json::from_slice(
            &std::fs::read(root().join("tests/fixtures/mockserver/expectations.json"))
                .expect("expectations read"),
        )
        .expect("expectations parse");
        let want = expectations.as_array().expect("an array").len();
        let deadline = Instant::now() + Duration::from_mins(1);
        while fixtures.retrieve("ACTIVE_EXPECTATIONS").len() != want {
            assert!(
                Instant::now() < deadline,
                "MockServer loads {want} expectations"
            );
            std::thread::sleep(Duration::from_millis(500));
        }
        fixtures
    }

    /// `docker compose` for this project.
    fn compose(&self) -> Command {
        let mut command = Command::new("docker");
        command.current_dir(root()).args([
            "compose",
            "--project-name",
            &self.project,
            "--file",
            "docker-compose.yml",
        ]);
        command
    }

    /// MockServer's retrieved `kind` records.
    ///
    /// # Panics
    /// Panics when MockServer does not answer with a JSON array.
    fn retrieve(&self, kind: &str) -> Vec<Value> {
        let answer = Command::new("curl")
            .args([
                "-sf",
                "-X",
                "PUT",
                &format!("http://127.0.0.1:1080/mockserver/retrieve?type={kind}&format=JSON"),
            ])
            .output()
            .expect("curl runs");
        serde_json::from_slice(&answer.stdout).unwrap_or_default()
    }

    /// Every chat completion request the review Agent sent, in order.
    fn review_requests(&self) -> Vec<Value> {
        self.retrieve("REQUESTS")
            .into_iter()
            .filter_map(|request| {
                let body = &request["body"]["json"];
                let messages = body["messages"].as_array()?;
                messages
                    .iter()
                    .any(|message| {
                        message["role"] == "user"
                            && message["content"]
                                .as_str()
                                .is_some_and(|text| text.starts_with("Pull request:"))
                    })
                    .then(|| body.clone())
            })
            .collect()
    }
}

impl Drop for Fixtures {
    /// Remove MockServer.
    fn drop(&mut self) {
        let _ = self
            .compose()
            .args(["down", "--remove-orphans"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// `scripts/dev/example-app.sh <verb>` against `base` as the principal of
/// `key`, with a config home of its own.
fn harness(verb: &str, base: &str, key: &str, config: &Path) -> Command {
    let mut command = Command::new(root().join("scripts/dev/example-app.sh"));
    command
        .arg(verb)
        .env("WYRD_SERVER_URL", base)
        .env("WYRD_API_KEY", key)
        .env("WYRD_CONFIG_HOME", config)
        .env_remove("WYRD_ACCESS_TOKEN")
        .env_remove("WYRD_WORKLOAD_TOKEN")
        .env_remove("WYRD_TENANT");
    command
}

/// A running `register` and the lines it printed.
struct Register {
    /// The process.
    child: Child,
    /// Its stdout and stderr lines.
    lines: Receiver<String>,
    /// Every line received so far.
    seen: Vec<String>,
}

impl Register {
    /// Start `register`, polling the connection every 200 ms.
    fn start(mut command: Command) -> Self {
        let mut child = command
            .arg("0.2")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("register starts");
        let (sender, lines) = channel();
        for stream in [
            Box::new(child.stdout.take().expect("stdout")) as Box<dyn std::io::Read + Send>,
            Box::new(child.stderr.take().expect("stderr")),
        ] {
            let sender = sender.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let _ = sender.send(line);
                }
            });
        }
        Self {
            child,
            lines,
            seen: Vec::new(),
        }
    }

    /// Wait up to two minutes for a line containing `text`.
    ///
    /// # Panics
    /// Panics when none arrives.
    fn until(&mut self, text: &str) -> String {
        let deadline = Instant::now() + Duration::from_mins(2);
        loop {
            if let Some(line) = self.seen.iter().find(|line| line.contains(text)) {
                return line.clone();
            }
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) => self.seen.push(line),
                Err(error) => panic!("register printed no {text:?} ({error}): {:#?}", self.seen),
            }
        }
    }

    /// Wait for the process to exit and return whether it succeeded.
    fn exit(&mut self) -> bool {
        self.child.wait().expect("register exits").success()
    }
}

impl Drop for Register {
    /// Stop a still-running `register`, as `dev:full` does on exit.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The connected deployment the harness runs against.
struct Deployment {
    /// GitHub.
    mock: GithubAppMock,
    /// Wyrd, running queued work against MockServer.
    server: WyrdTestServer,
    /// The development principal's API key.
    key: String,
    /// Its access token.
    token: String,
    /// The harness's config home.
    config: tempfile::TempDir,
}

impl Deployment {
    /// The server's base URL.
    fn base(&self) -> &str {
        self.server.base_url().expect("bound")
    }

    /// `GET uri` as the development principal, answering `wanted`.
    ///
    /// # Panics
    /// Panics on another status.
    async fn get(&self, uri: &str, wanted: StatusCode) -> Value {
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Body::empty())
            .expect("request builds");
        let response = self
            .server
            .oneshot_authenticated(&self.token, request)
            .await
            .expect("read responds");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1 << 24)
            .await
            .expect("body reads");
        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        assert_eq!(status, wanted, "{uri}: {body}");
        body
    }

    /// The exact Card `kind/name@version`.
    async fn card(&self, kind: &str, name: &str, version: &str) -> Value {
        self.get(
            &format!("/v1/cards/by-ref?kind={kind}&name={name}&version={version}"),
            StatusCode::OK,
        )
        .await
    }

    /// Every declared Repository Card.
    async fn repositories(&self) -> Vec<Value> {
        let page = self.get("/v1/cards?kind=Repository", StatusCode::OK).await;
        page["items"]
            .as_array()
            .unwrap_or_else(|| panic!("a Card page: {page}"))
            .clone()
    }

    /// The one `wyrd/code-review` Check run delivered for `head`.
    fn check(&self, head: &str) -> Value {
        let runs: Vec<Value> = self
            .mock
            .check_runs(DEMO)
            .into_iter()
            .filter(|run| run["head_sha"] == head)
            .collect();
        assert_eq!(runs.len(), 1, "{runs:#?}");
        assert_eq!(runs[0]["name"], "wyrd/code-review");
        assert_eq!(runs[0]["status"], "completed");
        runs[0].clone()
    }

    /// Assert pull request `number` got one inline-free `COMMENT` review of `head`.
    fn commented(&self, number: u64, head: &str) {
        let reviews = self.mock.reviews(DEMO, number);
        assert_eq!(reviews.len(), 1, "{reviews:#?}");
        assert_eq!(reviews[0]["event"], "COMMENT");
        assert_eq!(reviews[0]["commit_id"], head);
        assert_eq!(reviews[0]["comments"], json!([]));
    }

    /// Serve `value` at `path` below the demo repository.
    fn json(&self, path: &str, value: &Value) {
        self.mock
            .serve_repository(DEMO, path, StatusCode::OK, value.to_string().into_bytes());
    }

    /// Serve `commit` with root `tree` holding `module`.
    fn commit(&self, commit: &str, tree: &str, module: &[u8]) {
        self.json(
            &format!("/git/commits/{commit}"),
            &json!({"sha": commit, "tree": {"sha": tree}, "committer": {"date": "2026-03-01T00:00:00Z"}}),
        );
        self.mock.serve_repository(
            DEMO,
            &format!("/git/blobs/{}", blob(module)),
            StatusCode::OK,
            module.to_vec(),
        );
        self.json(
            &format!("/git/trees/{tree}?recursive=1"),
            &json!({"sha": tree, "truncated": false, "tree": [
                {"path": MODULE, "mode": "100644", "type": "blob", "sha": blob(module), "size": module.len()}]}),
        );
    }

    /// Serve the demo's `main` at commit `c…` (tree `7…`).
    fn serve_main(&self) {
        self.json(
            "",
            &json!({"id": DEMO, "full_name": "bohmian-ai/wyrd-example-py", "default_branch": "main", "private": false}),
        );
        self.json(
            "/pulls?state=closed&base=main&sort=updated&direction=desc&per_page=100&page=1",
            &json!([]),
        );
        self.json(
            "/branches/main",
            &json!({"name": "main", "commit": {"sha": sha('c'), "commit": {
                "tree": {"sha": sha('7')}, "committer": {"date": "2026-02-01T12:00:00Z"}}}}),
        );
        self.commit(&sha('c'), &sha('7'), MAIN);
        self.mock.serve_repository(
            DEMO,
            &format!("/tarball/{}", sha('c')),
            StatusCode::OK,
            tarball(&[(MODULE, MAIN)]),
        );
    }

    /// Open pull request `number` titled `title` whose head commit `head`
    /// (tree `tree`), each a repeated hex digit, changes the module to `module`, and return its terminal
    /// execution once GitHub delivery settles.
    ///
    /// # Panics
    /// Panics when the delivery is refused, nothing is admitted, or the
    /// execution or its delivery does not settle in two minutes.
    async fn review(
        &self,
        binding: &str,
        number: u64,
        title: &str,
        (head, tree): (char, char),
        module: &[u8],
    ) -> Value {
        let (head, tree) = (sha(head), sha(tree));
        self.commit(&head, &tree, module);
        let patch = format!(
            "@@ -1,2 +1,2 @@\n-{}\n+{}",
            String::from_utf8_lossy(MAIN)
                .lines()
                .next()
                .unwrap_or_default(),
            String::from_utf8_lossy(module)
                .lines()
                .next()
                .unwrap_or_default()
        );
        self.json(
            &format!("/compare/{}...{head}", sha('c')),
            &json!({"status": "ahead", "base_commit": {"sha": sha('c')},
                    "merge_base_commit": {"sha": sha('c')}, "commits": [{"sha": head}],
                    "files": [{"filename": MODULE, "status": "modified", "sha": blob(module), "patch": patch}]}),
        );
        let mut pull = PullRequest::open(DEMO, &sha('c'), &head);
        title.clone_into(&mut pull.title);
        self.mock.set_pull_request(DEMO, number, pull);
        let payload = self
            .mock
            .pull_request_event(INSTALLATION, DEMO, number, "opened");
        let (status, _) = self
            .mock
            .deliver_new(self.base(), "pull_request", &payload)
            .await
            .expect("delivery sends");
        assert_eq!(status, StatusCode::ACCEPTED);
        let deadline = Instant::now() + Duration::from_mins(2);
        loop {
            let status = self
                .get(
                    &format!("/v1/verification/bindings/{binding}"),
                    StatusCode::OK,
                )
                .await;
            if let Some(execution) = status["last_execution_id"].as_str() {
                let execution = self
                    .get(
                        &format!("/v1/verification/executions/{execution}"),
                        StatusCode::OK,
                    )
                    .await;
                let delivery = &execution["github_delivery"]["status"];
                if execution["pull_request"]["head"]["sha"] == head.as_str()
                    && !execution["result"].is_null()
                    && !delivery.is_null()
                    && delivery != "pending"
                {
                    return execution;
                }
                assert!(Instant::now() < deadline, "{execution}");
            }
            assert!(
                Instant::now() < deadline,
                "nothing admitted for {head}: {status}"
            );
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

/// The development harness seeds the review Verifier, waits for GitHub to
/// cover the demo repository, registers it through the Cards API, and every
/// subsequent pull request reaches a terminal verdict with its Check and
/// `COMMENT` review delivered: an ordinary title passes, a
/// `[wyrd-demo-fail]` title fails with the scripted finding, and the marker
/// in source context alone does not.
///
/// # Panics
/// Panics when a step or observation differs.
#[tokio::test(flavor = "multi_thread")]
async fn dev_harness_registers_the_demo_and_reviews_its_pull_requests() {
    let prepare = Command::new(root().join("scripts/dev/example-app.sh"))
        .arg("prepare")
        .output()
        .expect("prepare runs");
    assert!(prepare.status.success(), "{prepare:?}");
    let fixtures = Fixtures::start();

    let mock = GithubAppMock::start().expect("the GitHub mock starts");
    mock.add_installation(INSTALLATION, &[(OTHER, "acme/web")]);
    mock.add_user("octo", &[INSTALLATION]);
    mock.sign_in("octo");
    mock.install_target(INSTALLATION);
    let server = WyrdTestServer::builder()
        .with_github_app_for_test(&mock)
        .with_verification_runtime_for_test()
        .with_gateway_provider_root_for_test(
            "http://127.0.0.1:1080"
                .parse()
                .expect("MockServer URL parses"),
        )
        .start_bound()
        .await
        .expect("bound test server starts");
    let principal = server
        .bootstrap_service("development", &["admin"])
        .await
        .expect("the development principal bootstraps");
    let key = principal
        .api_key()
        .expect("a machine principal")
        .expose_secret()
        .to_owned();
    let token = server
        .exchange_api_key(&key.clone().into())
        .await
        .expect("the key exchanges");
    let dev = Deployment {
        mock,
        server,
        key,
        token,
        config: tempfile::tempdir().expect("config home creates"),
    };

    // 1. Setup seeds the pinned checkout's reusable review policy; no Repository yet.
    let setup = harness("setup", dev.base(), &dev.key, dev.config.path())
        .output()
        .expect("setup runs");
    assert!(setup.status.success(), "{setup:?}");
    let verifier = dev.card("Verifier", "code-review", "1.0.0").await["card"].clone();
    let agent = dev.card("Agent", "code-reviewer", "1.0.0").await["card"].clone();
    assert_eq!(
        agent["spec"]["execution_route"],
        json!({"kind": "wyrd_gateway", "deployment": "openai"})
    );
    assert_eq!(agent["spec"]["tool_names"], json!(["fs.read"]));
    assert_eq!(
        agent["relationships"]["inbound_refs"],
        json!([{"ref": {"kind": "Verifier", "name": "code-review", "version": "1.0.0",
                        "uid": verifier["metadata"]["uid"]}}]),
        "the Verifier references this exact Agent"
    );
    let prompt = &agent["spec"]["prompt"];
    let prompt = dev
        .card(
            "Prompt",
            prompt["name"].as_str().expect("a Prompt name"),
            prompt["version"].as_str().expect("a Prompt version"),
        )
        .await["card"]
        .clone();
    assert_eq!(prompt["spec"]["model"], "gpt-4o");
    assert_eq!(prompt["spec"]["response_format"], "code_review");
    assert!(
        prompt["spec"]["messages"][0]["content"]
            .as_str()
            .is_some_and(|text| text.starts_with("Pull request:\n{{pr}}")),
        "{prompt}"
    );
    assert!(dev.repositories().await.is_empty());

    // 2. Unconnected and then not covering the demo, register waits and declares nothing.
    let mut register =
        Register::start(harness("register", dev.base(), &dev.key, dev.config.path()));
    register.until("waiting: GitHub connection is not_connected");
    install(
        &dev.server,
        &dev.mock,
        dev.server.data_tenant_id(),
        dev.server.pg_fixture().tenant_slug(),
        "github_admin",
    )
    .await;
    register.until("does not cover bohmian-ai/wyrd-example-py");
    assert!(dev.repositories().await.is_empty());

    // Covering it, register declares the Repository with the seeded Verifier and exits.
    dev.mock.add_installation(
        INSTALLATION,
        &[(OTHER, "acme/web"), (DEMO, "bohmian-ai/wyrd-example-py")],
    );
    install(
        &dev.server,
        &dev.mock,
        dev.server.data_tenant_id(),
        dev.server.pg_fixture().tenant_slug(),
        "github_admin_again",
    )
    .await;
    let registered = register.until("registered bohmian-ai/wyrd-example-py");
    assert!(register.exit(), "{:#?}", register.seen);
    let repositories = dev.repositories().await;
    assert_eq!(repositories.len(), 1, "{repositories:?}");
    let version = repositories[0]["version"]
        .as_str()
        .expect("a version")
        .to_owned();
    let repository = dev.card("Repository", "wyrd-example-py", &version).await["card"].clone();
    assert_eq!(repository["spec"]["host"], dev.mock.host());
    assert_eq!(repository["spec"]["repository_id"], DEMO.to_string());
    assert_eq!(
        repository["spec"]["verified_by"],
        json!([{"verifier": {"kind": "Verifier", "name": "code-review", "version": "1.0.0",
                             "uid": verifier["metadata"]["uid"]},
                "runs_on": {"kind": "pull_request"}}])
    );
    assert_eq!(repository["status"]["repository"]["selected"], true);
    assert_eq!(
        repository["status"]["repository"]["full_name"],
        "bohmian-ai/wyrd-example-py"
    );
    assert!(
        registered.contains(&format!("Repository/wyrd-example-py@{version}")),
        "{registered}"
    );
    let bindings = repository["status"]["verification"]["binding_ids"]
        .as_array()
        .expect("the Repository serves its bindings");
    assert_eq!(bindings.len(), 1, "{repository}");
    let binding = bindings[0].as_str().expect("a binding id").to_owned();

    // An unchanged declaration reuses the selected version.
    let mut again = Register::start(harness("register", dev.base(), &dev.key, dev.config.path()));
    assert!(
        again
            .until("registered bohmian-ai/wyrd-example-py")
            .contains(&format!("@{version},"))
    );
    assert!(again.exit());
    assert_eq!(dev.repositories().await.len(), 1);

    dev.serve_main();

    // 4. A `[wyrd-demo-fail]` title fails with the scripted high finding in the Check summary.
    let failed = dev
        .review(
            &binding,
            2,
            "[wyrd-demo-fail] Shout the answer",
            ('b', '9'),
            b"def answer(question):\n    return question.upper()\n",
        )
        .await;
    assert_eq!(failed["status"], "completed", "{failed}");
    assert_eq!(failed["result"]["outcome"]["verdict"], "failed", "{failed}");
    assert_eq!(failed["github_delivery"]["status"], "delivered", "{failed}");
    let check = dev.check(&sha('b'));
    assert_eq!(check["conclusion"], "failure", "{check}");
    let summary = check["output"]["summary"].as_str().unwrap_or_default();
    assert!(
        summary.contains("Scripted high-severity finding (development data)"),
        "{check}"
    );
    dev.commented(2, &sha('b'));

    // The marker in source context under an ordinary title gets the ordinary report.
    let marked = dev
        .review(
            &binding,
            3,
            "Quote the title marker",
            ('d', '6'),
            b"def answer(question):\n    return '\"title\":\"[wyrd-demo-fail]'\n",
        )
        .await;
    assert_ne!(marked["result"]["outcome"]["verdict"], "failed", "{marked}");
    let check = dev.check(&sha('d'));
    assert_eq!(check["output"]["summary"], ORDINARY_SUMMARY, "{check}");
    dev.commented(3, &sha('d'));

    // Each review was one final model turn that read no file.
    let requests = fixtures.review_requests();
    assert_eq!(requests.len(), 2, "{requests:#?}");
    for request in &requests {
        let messages = request["messages"].as_array().expect("messages");
        assert!(
            messages.iter().all(|message| message["role"] != "tool"),
            "{request}"
        );
    }
    let marked_prompt = requests[1]["messages"].to_string();
    assert!(
        marked_prompt.contains(r#"\"title\":\"Quote the title marker\""#),
        "{marked_prompt}"
    );
    assert!(
        marked_prompt.contains("[wyrd-demo-fail]"),
        "{marked_prompt}"
    );

    // 3. An ordinary pull request passes with a success Check and a COMMENT review.
    let passed = dev
        .review(
            &binding,
            1,
            "Trim the question",
            ('a', '8'),
            b"def answer(question):\n    return question.strip()\n",
        )
        .await;
    assert_eq!(passed["status"], "completed", "{passed}");
    assert_eq!(passed["github_delivery"]["status"], "delivered", "{passed}");
    let check = dev.check(&sha('a'));
    assert_eq!(check["output"]["summary"], ORDINARY_SUMMARY, "{check}");
    dev.commented(1, &sha('a'));
    assert_eq!(fixtures.review_requests().len(), 3);
    assert_eq!(passed["result"]["outcome"]["verdict"], "passed", "{passed}");
    assert_eq!(check["conclusion"], "success", "{check}");
}

/// Without a GitHub App, `register` reports review off, exits successfully,
/// and declares nothing.
///
/// # Panics
/// Panics when `register` fails, prints otherwise, or declares a Card.
#[tokio::test(flavor = "multi_thread")]
async fn dev_harness_leaves_review_off_without_a_github_app() {
    let prepare = Command::new(root().join("scripts/dev/example-app.sh"))
        .arg("prepare")
        .output()
        .expect("prepare runs");
    assert!(prepare.status.success(), "{prepare:?}");
    let server = WyrdTestServer::builder()
        .start_bound()
        .await
        .expect("bound test server starts");
    let principal = server
        .bootstrap_service("development", &["admin"])
        .await
        .expect("the development principal bootstraps");
    let key = principal
        .api_key()
        .expect("a machine principal")
        .expose_secret()
        .to_owned();
    let config = tempfile::tempdir().expect("config home creates");
    let base = server.base_url().expect("bound");
    let mut register = Register::start(harness("register", base, &key, config.path()));
    register.until("review       off: this server has no GitHub App");
    assert!(register.exit(), "{:#?}", register.seen);
    let token = server
        .exchange_api_key(&key.clone().into())
        .await
        .expect("the key exchanges");
    let request = Request::builder()
        .uri("/v1/cards?kind=Repository")
        .body(Body::empty())
        .expect("request builds");
    let response = server
        .oneshot_authenticated(&token, request)
        .await
        .expect("list responds");
    assert_eq!(response.status(), StatusCode::OK);
    let page: Value = serde_json::from_slice(
        &to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("body reads"),
    )
    .expect("JSON");
    assert_eq!(page["items"], json!([]), "{page}");
}
