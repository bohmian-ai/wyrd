//! Test-tier napi projection of the loopback GitHub App and the signed-in
//! browser that completes a connection, for TypeScript journeys only.

use napi::Result;
use napi_derive::napi;
use serde_json::Value;
use wyrd_testing::github::{BrowserAnswer, GithubAppMock, GithubBrowser};

use crate::reason;

/// The loopback GitHub App a test server was started with.
#[napi]
pub struct NativeGithubAppMock {
    /// The shared mock the server is configured against.
    pub(crate) mock: GithubAppMock,
}

/// One repository an installation selects.
#[napi(object)]
pub struct NativeGithubRepository {
    /// Provider repository id.
    pub id: u32,
    /// `owner/name`.
    pub full_name: String,
}

#[napi]
impl NativeGithubAppMock {
    /// The host Wyrd reports in its connection view.
    #[napi(getter)]
    #[must_use]
    pub fn host(&self) -> String {
        self.mock.host()
    }

    /// Install the App as `id` with `repositories` selected.
    #[napi]
    pub fn add_installation(&self, id: u32, repositories: Vec<NativeGithubRepository>) {
        let repositories: Vec<(u64, &str)> = repositories
            .iter()
            .map(|repository| (u64::from(repository.id), repository.full_name.as_str()))
            .collect();
        self.mock.add_installation(u64::from(id), &repositories);
    }

    /// Create GitHub user `login` with access to `installations`.
    #[napi]
    pub fn add_user(&self, login: String, installations: Vec<u32>) {
        let installations: Vec<u64> = installations.into_iter().map(u64::from).collect();
        self.mock.add_user(&login, &installations);
    }

    /// Deny user `login` repository `repository_id`.
    #[napi]
    pub fn hide_repository(&self, login: String, repository_id: u32) {
        self.mock.hide_repository(&login, u64::from(repository_id));
    }

    /// Sign the browser in to GitHub as `login`.
    #[napi]
    pub fn sign_in(&self, login: String) {
        self.mock.sign_in(&login);
    }

    /// Make the install page select installation `id`.
    #[napi]
    pub fn install_target(&self, id: u32) {
        self.mock.install_target(u64::from(id));
    }

    /// Every code, token and secret the mock issued, for leak assertions.
    #[napi]
    #[must_use]
    pub fn secrets(&self) -> Vec<String> {
        self.mock.secrets()
    }
}

/// The final answer of one browser navigation.
#[napi(object)]
pub struct NativeBrowserAnswer {
    /// Final HTTP status.
    pub status: u32,
    /// Final body, parsed as JSON when it is JSON.
    pub body: Value,
    /// The last Wyrd callback URL visited, for replay.
    pub callback_url: Option<String>,
}

impl From<BrowserAnswer> for NativeBrowserAnswer {
    /// Project the harness answer, keeping only what a journey asserts on.
    fn from(answer: BrowserAnswer) -> Self {
        Self {
            status: u32::from(answer.status),
            callback_url: answer.callback_url().map(ToString::to_string),
            body: answer.body,
        }
    }
}

/// A browser signed in to one Wyrd tenant that follows redirects across
/// Wyrd and the GitHub mock.
#[napi]
pub struct NativeGithubBrowser {
    /// The harness browser holding Wyrd's cookies.
    browser: GithubBrowser,
}

#[napi]
impl NativeGithubBrowser {
    /// Navigate to `url` and follow redirects until a final answer.
    ///
    /// # Errors
    ///
    /// Returns a napi error when a hop cannot be sent or the redirect chain
    /// is too long.
    #[napi]
    pub fn open(&mut self, url: String) -> Result<NativeBrowserAnswer> {
        wyrd_runtime::runtime()
            .block_on(self.browser.open(&url))
            .map(NativeBrowserAnswer::from)
            .map_err(reason)
    }
}

/// Sign a browser in to tenant `tenant` at `server_base` with `api_key`
/// through the production browser API-key sign-in.
///
/// # Errors
///
/// Returns a napi error when sign-in refuses, sets no session cookie, or
/// cannot be sent.
#[napi]
pub fn sign_in_github_browser(
    server_base: String,
    tenant: String,
    api_key: String,
) -> Result<NativeGithubBrowser> {
    wyrd_runtime::runtime()
        .block_on(GithubBrowser::sign_in(&server_base, &tenant, &api_key))
        .map(|browser| NativeGithubBrowser { browser })
        .map_err(reason)
}
