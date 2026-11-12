//! Tenant GitHub App connection, shared by HTTP and MCP.
//!
//! [`GithubConnections`] is the one owner of connect, read, disconnect,
//! GitHub's setup and OAuth callback redirects, installation webhooks and
//! narrowed installation tokens. Every caller operation evaluates
//! `identity_connections:write` and stages the decision on the process audit
//! outbox before it runs. A connection transaction is one-use login state
//! bound to the beginning principal, its tenant and, from GitHub's setup
//! redirect on, one browser; the callback consumes it before any provider IO
//! and associates only after GitHub confirms the installation and the
//! authorizing user's access, conditionally on the slot revision the
//! transaction began from. Saved state is honored only under the App
//! settings it was made with. No user token is stored, and no token, key or
//! secret is returned or logged.

use std::time::Duration;

use base64::Engine as _;
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;
use uuid::Uuid;
use wyrd_auth::browser_sessions::random_secret;
use wyrd_auth::login::LOGIN_STATE_TTL;
use wyrd_runtime::Permission;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{AbsoluteUrl, ConnectionTester, Sha256Hex};
use wyrd_spec::card::repository::RepositoryRemote;
use wyrd_spec::error::WyrdError;
use wyrd_spec::github_connection::{
    GithubAppIdentity, GithubCallbackQuery, GithubConnectResponse, GithubConnectionState,
    GithubConnectionView, GithubRepositoryAccess, GithubSetupQuery,
};
use wyrd_spec::ids::RepositoryId;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    GithubLoginState, bind_github_login_state, consume_github_login_state,
    insert_github_login_state,
};
use wyrd_sql::queries::github_installations::{
    GithubAssociation, GithubAssociationOutcome, GithubInstallationRow, GithubSlotBinding,
    associate_github_installation, github_installation, lock_github_installation,
    release_github_installation, remove_github_repositories, set_github_installation_suspended,
};
use wyrd_sql::queries::verifier_executions::VerifierExecutionQueue;

use super::app::{GithubApp, refused, unavailable};
use super::pull_requests::PullRequestEvent;
use crate::audit;
use crate::components::auth::Caller;
use crate::state::{AppState, registry_db_error};

/// Audit operation of a connect.
const CONNECT: &str = "github_connection.connect";
/// Audit operation of a read.
const READ: &str = "github_connection.read";
/// Audit operation of a disconnect.
const DISCONNECT: &str = "github_connection.disconnect";
/// Audit operation of GitHub's setup redirect.
const SETUP: &str = "github_connection.setup";
/// Audit operation of GitHub's OAuth callback.
const CALLBACK: &str = "github_connection.callback";
/// Audited resource of every operation.
const RESOURCE: &str = "github_connection";

/// Lifetime of a connection transaction and its browser binding cookie.
pub(crate) const TRANSACTION_TTL: Duration = LOGIN_STATE_TTL;

/// The refusal when the deployment configures no GitHub App.
pub(crate) fn not_configured() -> WyrdError {
    WyrdError::GithubNotConfigured {
        message: "this deployment has no GitHub App configured".to_owned(),
        details: serde_json::json!({}),
    }
}

/// A conflict naming why the association was refused.
fn conflict(reason: &str, message: &str) -> WyrdError {
    WyrdError::GithubConnectionConflict {
        message: message.to_owned(),
        details: serde_json::json!({ "reason": reason }),
    }
}

/// The refusal for a transaction that is unknown, expired, consumed, or not
/// this principal's, browser's or tenant's, which are indistinguishable.
fn transaction_invalid() -> WyrdError {
    refused(
        "transaction_invalid",
        "this GitHub connection transaction is not valid for this browser session",
    )
}

/// The refusal for an installation token or provider observation whose slot
/// changed while GitHub answered; the result is discarded.
fn connection_changed() -> WyrdError {
    conflict(
        "connection_changed",
        "the tenant's GitHub connection changed while GitHub answered; retry",
    )
}

/// A provider id as stored.
///
/// # Errors
/// Returns [`WyrdError::GithubRequestInvalid`] for an id beyond `i64`.
fn stored_id(id: u64, field: &str) -> Result<i64, WyrdError> {
    i64::try_from(id).map_err(|_| WyrdError::GithubRequestInvalid {
        message: format!("{field} is out of range"),
        details: serde_json::json!({ "field": field }),
    })
}

/// The S256 PKCE challenge of `verifier` (RFC 7636 §4.2).
fn code_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256Hex::digest(verifier.as_bytes()).as_bytes())
}

/// A webhook's body, as far as Wyrd reads it.
#[derive(Deserialize)]
struct WebhookBody {
    /// Event action.
    action: Option<String>,
    /// The installation the event concerns.
    installation: Option<WebhookId>,
    /// Repositories removed from the installation.
    #[serde(default)]
    repositories_removed: Vec<WebhookId>,
}

/// An object carrying a provider id.
#[derive(Deserialize)]
struct WebhookId {
    /// Provider id.
    id: u64,
}

/// A verified setup redirect: where the browser goes next and the binding
/// secret its cookie carries.
pub(crate) struct SetupRedirect {
    /// GitHub's OAuth authorization page for this transaction.
    pub(crate) authorize_url: url::Url,
    /// The browser's transaction secret, set only in its callback cookie.
    pub(crate) binding: SecretString,
}

/// Owner of the tenant GitHub connection for one request.
pub struct GithubConnections<'a> {
    /// Server state providing tenant connections, authorization and audit.
    pub(super) state: &'a AppState,
}

impl<'a> GithubConnections<'a> {
    /// Build the owner over this process's state.
    pub const fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// The configured App.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubNotConfigured`] without one.
    pub(super) fn app(&self) -> Result<&'a GithubApp, WyrdError> {
        self.state.github.as_deref().ok_or_else(not_configured)
    }

    /// Evaluate `identity_connections:write` for `operation` and stage the
    /// decision on the process audit outbox.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without it.
    fn authorize(&self, caller: &Caller, operation: &str) -> Result<(), WyrdError> {
        audit::authorize(
            self.state,
            caller,
            &Permission::identity_connections_write(),
            operation,
            RESOURCE,
        )
    }

    /// A tenant connection for `tenant`.
    ///
    /// # Errors
    /// Returns the registry unavailability error.
    pub(super) async fn conn(&self, tenant: DataTenantId) -> Result<TenantConn<'a>, WyrdError> {
        self.state.registry_tenant_conn(tenant).await
    }

    /// `slot` when it is associated on the configured GitHub host; a slot
    /// saved under another host grants nothing until it is released.
    fn associated<'r>(
        app: &GithubApp,
        slot: Option<&'r GithubInstallationRow>,
    ) -> Option<&'r GithubInstallationRow> {
        let host = app.settings().host();
        slot.filter(|row| row.installation_id.is_some() && row.host.as_deref() == Some(&host))
    }

    /// Whether `transaction` was begun under the App's current issuer,
    /// OAuth client and callback.
    fn begun_here(app: &GithubApp, transaction: &GithubLoginState) -> bool {
        transaction.issuer == app.settings().web_origin.as_str()
            && transaction.client_id == app.settings().oauth_client_id
            && transaction.redirect_uri == app.callback_uri()
    }

    /// The redacted view of `slot`.
    fn view(app: &GithubApp, slot: Option<&GithubInstallationRow>) -> GithubConnectionView {
        let settings = app.settings();
        let associated = Self::associated(app, slot);
        GithubConnectionView {
            host: settings.host(),
            app: GithubAppIdentity {
                id: settings.app_id,
                slug: settings.slug.clone(),
            },
            state: match associated {
                None => GithubConnectionState::NotConnected,
                Some(row) if row.suspended_at.is_some() => GithubConnectionState::Suspended,
                Some(_) => GithubConnectionState::Active,
            },
            installation_id: associated
                .and_then(|row| row.installation_id)
                .and_then(|id| u64::try_from(id).ok()),
            repositories: associated
                .map(|row| row.repositories.0.clone())
                .unwrap_or_default(),
            connected_at: associated.and_then(|row| row.connected_at),
            updated_at: associated.map(|row| row.updated_at),
        }
    }

    /// Begin a connection transaction for the caller and return GitHub's
    /// install URL for it.
    ///
    /// The receipt is returned only after the transaction row commits.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubNotConfigured`], the permission denial, or
    /// a registry error.
    pub(crate) async fn connect(
        &self,
        caller: &Caller,
    ) -> Result<GithubConnectResponse, WyrdError> {
        let app = self.app()?;
        self.authorize(caller, CONNECT)?;
        let state = random_secret();
        let mut conn = self.conn(caller.data_tenant_id).await?;
        let (expires_at, _) = self
            .begin(&mut conn, app, caller, &state, None, None)
            .await?;
        conn.commit().await.map_err(registry_db_error)?;
        let authorization_url = AbsoluteUrl::new(app.install_url(&state).to_string())
            .map_err(|_| unavailable("the install URL"))?;
        Ok(GithubConnectResponse {
            authorization_url,
            expires_at,
        })
    }

    /// Record a transaction for `caller` under `state`, bound to the slot as
    /// it is now, optionally already bound to `browser` with GitHub's
    /// installation `hint`, and return its expiry and PKCE verifier.
    ///
    /// # Errors
    /// Returns a registry error.
    async fn begin(
        &self,
        conn: &mut TenantConn<'_>,
        app: &GithubApp,
        caller: &Caller,
        state: &str,
        browser: Option<Sha256Hex>,
        hint: Option<i64>,
    ) -> Result<(chrono::DateTime<chrono::Utc>, SecretString), WyrdError> {
        let slot = github_installation(conn)
            .await
            .map_err(registry_db_error)?
            .map_or_else(
                || GithubSlotBinding {
                    connection_id: Uuid::now_v7(),
                    connection_revision: 0,
                },
                |row| row.binding(),
            );
        let row = GithubLoginState {
            slot,
            issuer: app.settings().web_origin.to_string(),
            client_id: app.settings().oauth_client_id.clone(),
            redirect_uri: app.callback_uri(),
            code_verifier: SecretString::from(random_secret()),
            actor: ConnectionTester {
                principal_id: caller.principal.id,
                principal_kind: caller.principal.kind.tag(),
            },
            browser_binding: browser,
            installation_hint: hint,
        };
        let expires_at = insert_github_login_state(
            conn,
            &Sha256Hex::digest(state.as_bytes()),
            &row,
            TRANSACTION_TTL,
        )
        .await
        .map_err(registry_db_error)?
        .ok_or_else(transaction_invalid)?;
        Ok((expires_at, row.code_verifier))
    }

    /// Read the tenant's redacted connection.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubNotConfigured`], the permission denial, or
    /// a registry error.
    pub(crate) async fn get(&self, caller: &Caller) -> Result<GithubConnectionView, WyrdError> {
        let app = self.app()?;
        self.authorize(caller, READ)?;
        let mut conn = self.conn(caller.data_tenant_id).await?;
        let slot = github_installation(&mut conn)
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        Ok(Self::view(app, slot.as_ref()))
    }

    /// Disconnect the tenant: fence its installation's work and release the
    /// association in one transaction, then return the released view.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubNotConfigured`], the permission denial,
    /// [`WyrdError::GithubNotConnected`] when nothing is associated, or a
    /// registry error.
    pub(crate) async fn disconnect(
        &self,
        caller: &Caller,
    ) -> Result<GithubConnectionView, WyrdError> {
        let app = self.app()?;
        self.authorize(caller, DISCONNECT)?;
        let mut conn = self.conn(caller.data_tenant_id).await?;
        let Some(released) = Self::fence_and_release(&mut conn, None).await? else {
            conn.commit().await.map_err(registry_db_error)?;
            return Err(WyrdError::GithubNotConnected {
                message: "this tenant has no GitHub installation associated".to_owned(),
                details: serde_json::json!({}),
            });
        };
        conn.commit().await.map_err(registry_db_error)?;
        tracing::info!(revision = released.revision, "GitHub connection released");
        Ok(Self::view(app, Some(&released)))
    }

    /// Fence the slot's work and release its association, optionally only
    /// when it holds exactly `only`.
    ///
    /// The slot stays locked until the caller's transaction ends, so no
    /// association or webhook change interleaves. The release advances the
    /// slot revision and, in the same transaction, cancels the unfinished
    /// pull-request executions of every repository it reached with
    /// `repository_access_lost`, keeping their history. Once it commits,
    /// [`Self::installation_token`] refuses every repository of the
    /// installation and discards any token GitHub was still minting under
    /// the released revision.
    ///
    /// # Errors
    /// Returns a registry error.
    async fn fence_and_release(
        conn: &mut TenantConn<'_>,
        only: Option<(&str, i64)>,
    ) -> Result<Option<GithubInstallationRow>, WyrdError> {
        let held = lock_github_installation(conn)
            .await
            .map_err(registry_db_error)?;
        let released = release_github_installation(conn, only)
            .await
            .map_err(registry_db_error)?;
        if released.is_some()
            && let Some(host) = held.as_ref().and_then(|row| row.host.as_deref())
        {
            Self::access_lost(conn, host, None).await?;
        }
        Ok(released)
    }

    /// Cancel the unfinished pull-request executions of `host`'s
    /// `repositories`, or of every repository when `None`, with
    /// `repository_access_lost`, in the caller's transaction.
    ///
    /// # Errors
    /// Returns a registry error.
    async fn access_lost(
        conn: &mut TenantConn<'_>,
        host: &str,
        repositories: Option<&[String]>,
    ) -> Result<(), WyrdError> {
        let cancelled = VerifierExecutionQueue::default()
            .cancel_pull_request_repositories(conn, host, repositories)
            .await
            .map_err(registry_db_error)?;
        if cancelled > 0 {
            tracing::info!(
                cancelled,
                "pull-request work cancelled on repository access loss"
            );
        }
        Ok(())
    }

    /// GitHub's setup redirect reached the caller's signed-in browser: bind
    /// the transaction to it, or begin one when the installation was begun on
    /// GitHub, and return GitHub's OAuth authorization page.
    ///
    /// The installation id is only recorded as a hint. A transaction begun
    /// under other App settings is refused and left unbound.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubNotConfigured`], the permission denial,
    /// [`WyrdError::GithubConnectionRefused`] (`transaction_invalid`) for a
    /// state this browser session cannot bind, or a registry error.
    pub(crate) async fn setup(
        &self,
        caller: &Caller,
        query: &GithubSetupQuery,
    ) -> Result<SetupRedirect, WyrdError> {
        let app = self.app()?;
        self.authorize(caller, SETUP)?;
        let hint = query
            .installation_id
            .map(|id| stored_id(id, "installation_id"))
            .transpose()?;
        let binding = SecretString::from(random_secret());
        let binding_hash = Sha256Hex::digest(binding.expose_secret().as_bytes());
        let mut conn = self.conn(caller.data_tenant_id).await?;
        let (state, verifier) = if let Some(state) = &query.state {
            let bound = bind_github_login_state(
                &mut conn,
                &Sha256Hex::digest(state.as_bytes()),
                caller.principal.id,
                &binding_hash,
                hint,
            )
            .await
            .map_err(registry_db_error)?;
            let Some(bound) = bound.filter(|bound| Self::begun_here(app, bound)) else {
                return Err(transaction_invalid());
            };
            (state.clone(), bound.code_verifier)
        } else {
            let state = random_secret();
            let (_, verifier) = self
                .begin(&mut conn, app, caller, &state, Some(binding_hash), hint)
                .await?;
            (state, verifier)
        };
        conn.commit().await.map_err(registry_db_error)?;
        Ok(SetupRedirect {
            authorize_url: app.authorize_url(&state, &code_challenge(verifier.expose_secret())),
            binding,
        })
    }

    /// GitHub's OAuth callback reached the caller's browser presenting
    /// `binding`: consume the transaction, verify the authorization and the
    /// installation, and associate it with the caller's tenant.
    ///
    /// The transaction is consumed and committed before any provider IO, so a
    /// callback cancelled or failed after that point leaves it spent and the
    /// tenant begins again; one begun under other App settings is refused
    /// before it is consumed or its code exchanged. The
    /// installation is GitHub's hint when the transaction or query carries
    /// one, which the authorizing user must be able to access, or else the
    /// user's only accessible installation of this App. The selection is the
    /// installation's repositories the user can access.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubNotConfigured`], the permission denial,
    /// [`WyrdError::GithubRequestInvalid`] without a code or state,
    /// [`WyrdError::GithubConnectionRefused`] for a denied authorization, an
    /// invalid transaction, or an installation the user cannot access, is
    /// not this App's, is ambiguous or suspended,
    /// [`WyrdError::GithubConnectionConflict`] when the slot changed since
    /// the transaction began, the tenant holds another installation, or
    /// another tenant holds this one, [`WyrdError::GithubUnavailable`], or a
    /// registry error.
    pub(crate) async fn callback(
        &self,
        caller: &Caller,
        query: &GithubCallbackQuery,
        binding: Option<&str>,
    ) -> Result<GithubConnectionView, WyrdError> {
        let app = self.app()?;
        if query.error.is_some() {
            return Err(refused(
                "authorization_denied",
                "the GitHub user did not authorize the App",
            ));
        }
        let (Some(code), Some(state)) = (&query.code, &query.state) else {
            return Err(WyrdError::GithubRequestInvalid {
                message: "the callback requires code and state".to_owned(),
                details: serde_json::json!({ "field": "code" }),
            });
        };
        let binding = binding.ok_or_else(transaction_invalid)?;
        self.authorize(caller, CALLBACK)?;
        let mut conn = self.conn(caller.data_tenant_id).await?;
        let consumed = consume_github_login_state(
            &mut conn,
            &Sha256Hex::digest(state.as_bytes()),
            caller.principal.id,
            &Sha256Hex::digest(binding.as_bytes()),
        )
        .await
        .map_err(registry_db_error)?;
        let Some(transaction) = consumed.filter(|consumed| Self::begun_here(app, consumed)) else {
            return Err(transaction_invalid());
        };
        conn.commit().await.map_err(registry_db_error)?;

        let token = app.exchange_code(code, &transaction.code_verifier).await?;
        let accessible = app.user_installations(&token).await?;
        let hint = match transaction.installation_hint {
            Some(hint) => u64::try_from(hint).ok(),
            None => query.installation_id,
        };
        let installation_id = match hint {
            Some(id) if accessible.contains(&id) => id,
            Some(_) => {
                return Err(refused(
                    "installation_inaccessible",
                    "the authorizing GitHub user cannot access this installation of the App",
                ));
            }
            None => match accessible.as_slice() {
                [only] => *only,
                [] => {
                    return Err(refused(
                        "installation_inaccessible",
                        "the authorizing GitHub user can access no installation of the App",
                    ));
                }
                _ => {
                    return Err(refused(
                        "installation_ambiguous",
                        "the authorizing GitHub user can access several installations; \
                         install or configure the App from Wyrd to choose one",
                    ));
                }
            },
        };
        let installation = app.installation(installation_id).await?.ok_or_else(|| {
            refused(
                "installation_inaccessible",
                "this installation does not belong to the configured App",
            )
        })?;
        if installation.suspended {
            return Err(refused(
                "installation_suspended",
                "GitHub has suspended this installation",
            ));
        }
        let repositories = app.user_repositories(&token, installation.id).await?;
        drop(token);

        let host = app.settings().host();
        let mut conn = self.conn(caller.data_tenant_id).await?;
        let outcome = associate_github_installation(
            &mut conn,
            transaction.slot,
            GithubAssociation {
                host: &host,
                installation_id: stored_id(installation.id, "installation_id")?,
                repositories: &repositories,
            },
        )
        .await
        .map_err(registry_db_error)?;
        let row = match outcome {
            GithubAssociationOutcome::Associated(row) => row,
            GithubAssociationOutcome::Stale => {
                return Err(conflict(
                    "connection_changed",
                    "the tenant's GitHub connection changed since this transaction began",
                ));
            }
            GithubAssociationOutcome::TenantOccupied => {
                return Err(conflict(
                    "tenant_connected",
                    "this tenant is associated with another installation; disconnect it first",
                ));
            }
            GithubAssociationOutcome::InstallationClaimed => {
                return Err(conflict(
                    "installation_claimed",
                    "this installation is associated with another tenant",
                ));
            }
        };
        conn.commit().await.map_err(registry_db_error)?;
        tracing::info!(revision = row.revision, "GitHub connection associated");
        Ok(Self::view(app, Some(&row)))
    }

    /// Apply one installation webhook whose `signature` the caller has not
    /// yet checked.
    ///
    /// Deletion fences and releases the association, suspension and
    /// unsuspension apply GitHub's current state read back from GitHub, and
    /// repository removal drops those repositories from the selection. Every
    /// other event changes nothing. Routing to the owning tenant grants no
    /// permission.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubNotConfigured`],
    /// [`WyrdError::GithubWebhookRefused`] for a missing or wrong signature,
    /// [`WyrdError::GithubRequestInvalid`] for a body that is not an event,
    /// [`WyrdError::GithubUnavailable`] when GitHub cannot report a
    /// suspension, [`WyrdError::GithubConnectionConflict`]
    /// (`connection_changed`) when the slot changed while it did, so the
    /// delivery is retried, or a registry error.
    pub(crate) async fn webhook(
        &self,
        event: Option<&str>,
        delivery: Option<&str>,
        signature: Option<&str>,
        body: &[u8],
    ) -> Result<(), WyrdError> {
        let app = self.app()?;
        if !app.verify_webhook(body, signature) {
            return Err(WyrdError::GithubWebhookRefused {
                message: "the webhook signature does not verify".to_owned(),
                details: serde_json::json!({}),
            });
        }
        let payload: WebhookBody =
            serde_json::from_slice(body).map_err(|_| WyrdError::GithubRequestInvalid {
                message: "the webhook body is not a GitHub event".to_owned(),
                details: serde_json::json!({ "field": "body" }),
            })?;
        let (Some(event), Some(installation)) = (event, payload.installation) else {
            return Ok(());
        };
        let host = app.settings().host();
        let installation_id = stored_id(installation.id, "installation.id")?;
        let Some(tenant) = self
            .state
            .postgres
            .wyrd()
            .github_installation_tenant(&host, installation_id)
            .await
            .map_err(registry_db_error)?
        else {
            return Ok(());
        };
        if event == "pull_request" {
            let event: PullRequestEvent =
                serde_json::from_slice(body).map_err(|_| WyrdError::GithubRequestInvalid {
                    message: "the webhook body is not a pull_request event".to_owned(),
                    details: serde_json::json!({ "field": "body" }),
                })?;
            let delivery = delivery.ok_or_else(|| WyrdError::GithubRequestInvalid {
                message: "the pull_request delivery carries no X-GitHub-Delivery id".to_owned(),
                details: serde_json::json!({ "field": "x-github-delivery" }),
            })?;
            self.pull_request(app, tenant, installation_id, delivery, &event)
                .await?;
            return Ok(());
        }
        if let ("installation", Some("suspend" | "unsuspend")) = (event, payload.action.as_deref())
        {
            return self
                .observe_suspension(app, tenant, installation.id, installation_id)
                .await;
        }
        let mut conn = self.conn(tenant).await?;
        let changed = match (event, payload.action.as_deref()) {
            ("installation", Some("deleted")) => {
                Self::fence_and_release(&mut conn, Some((&host, installation_id))).await?
            }
            ("installation_repositories", Some("removed")) => {
                let removed: Vec<String> = payload
                    .repositories_removed
                    .iter()
                    .map(|repository| repository.id.to_string())
                    .collect();
                let changed =
                    remove_github_repositories(&mut conn, &host, installation_id, &removed)
                        .await
                        .map_err(registry_db_error)?;
                if changed.is_some() {
                    Self::access_lost(&mut conn, &host, Some(&removed)).await?;
                }
                changed
            }
            _ => None,
        };
        conn.commit().await.map_err(registry_db_error)?;
        if let Some(row) = changed {
            tracing::info!(
                event,
                revision = row.revision,
                "GitHub installation synchronized"
            );
        }
        Ok(())
    }

    /// Apply GitHub's current suspension of `tenant`'s associated
    /// installation, whatever the delivered action said.
    ///
    /// Deliveries can arrive out of order, so the state is read back from
    /// GitHub outside any SQL transaction and written only while the slot is
    /// still at the revision read before asking. Every applied observation
    /// advances the revision, so an older one still in flight is refused.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubUnavailable`] when GitHub cannot be read,
    /// [`WyrdError::GithubConnectionConflict`] (`connection_changed`) when
    /// the slot changed meanwhile, or a registry error.
    async fn observe_suspension(
        &self,
        app: &GithubApp,
        tenant: DataTenantId,
        provider_id: u64,
        installation_id: i64,
    ) -> Result<(), WyrdError> {
        let mut conn = self.conn(tenant).await?;
        let slot = github_installation(&mut conn)
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        let Some(observed) = Self::associated(app, slot.as_ref())
            .filter(|row| row.installation_id == Some(installation_id))
            .map(GithubInstallationRow::binding)
        else {
            return Ok(());
        };
        let Some(current) = app.installation(provider_id).await? else {
            return Ok(());
        };
        let mut conn = self.conn(tenant).await?;
        let row = set_github_installation_suspended(&mut conn, observed, current.suspended)
            .await
            .map_err(registry_db_error)?
            .ok_or_else(connection_changed)?;
        if current.suspended {
            Self::access_lost(&mut conn, &app.settings().host(), None).await?;
        }
        conn.commit().await.map_err(registry_db_error)?;
        tracing::info!(
            suspended = current.suspended,
            revision = row.revision,
            "GitHub installation suspension synchronized"
        );
        Ok(())
    }

    /// Lock `conn`'s tenant connection slot for the rest of its transaction
    /// and refuse unless it is associated on the configured host.
    ///
    /// A Card declaration holds this lock while it admits Repository
    /// identities, so concurrent admissions serialize on the slot that
    /// connect, release and access changes also lock.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubNotConfigured`],
    /// [`WyrdError::GithubNotConnected`] when nothing is associated on the
    /// configured host, or a registry error.
    pub(crate) async fn lock_association(
        &self,
        conn: &mut TenantConn<'_>,
    ) -> Result<(), WyrdError> {
        let app = self.app()?;
        let slot = lock_github_installation(conn)
            .await
            .map_err(registry_db_error)?;
        if Self::associated(app, slot.as_ref()).is_none() {
            return Err(WyrdError::GithubNotConnected {
                message: "this tenant has no GitHub installation associated".to_owned(),
                details: serde_json::json!({}),
            });
        }
        Ok(())
    }

    /// Verify a declared Repository's identity against `tenant`'s connection
    /// and return the configured host and the connected repository.
    ///
    /// The connection must be associated on the configured host and not
    /// suspended. An explicit `repository_id` wins and must be selected;
    /// otherwise the `remote` hint's `owner/name` is matched, ignoring case,
    /// against the selected repositories. A remote or explicit host other
    /// than the configured one never matches. Declaration needs no
    /// connection permission: nothing about the connection is returned
    /// beyond the declared repository.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubNotConfigured`],
    /// [`WyrdError::GithubNotConnected`] when nothing is associated on the
    /// configured host, [`WyrdError::RegistryRepositoryIdentityMissing`]
    /// when neither an id nor a remote names the repository,
    /// [`WyrdError::GithubRepositoryInaccessible`] when it is not selected
    /// for an active installation, or a registry error.
    pub(crate) async fn verify_repository(
        &self,
        tenant: DataTenantId,
        host: Option<&str>,
        repository_id: Option<&RepositoryId>,
        remote: Option<&RepositoryRemote>,
    ) -> Result<(String, GithubRepositoryAccess), WyrdError> {
        let app = self.app()?;
        let mut conn = self.conn(tenant).await?;
        let slot = github_installation(&mut conn)
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        let Some(slot) = Self::associated(app, slot.as_ref()) else {
            return Err(WyrdError::GithubNotConnected {
                message: "this tenant has no GitHub installation associated".to_owned(),
                details: serde_json::json!({}),
            });
        };
        let configured = app.settings().host();
        let found = match (repository_id, remote) {
            (None, None) => {
                return Err(WyrdError::RegistryRepositoryIdentityMissing {
                    message: "the Repository names no repository_id and no remote".to_owned(),
                    details: serde_json::json!({}),
                });
            }
            (Some(id), _) => slot
                .repositories
                .0
                .iter()
                .find(|entry| entry.repository_id == id.as_str()),
            (None, Some(remote)) => slot.repositories.0.iter().find(|entry| {
                remote.host == configured && entry.full_name.eq_ignore_ascii_case(&remote.full_name)
            }),
        };
        let found = found
            .filter(|_| slot.suspended_at.is_none() && host.is_none_or(|host| host == configured));
        let Some(found) = found else {
            return Err(WyrdError::GithubRepositoryInaccessible {
                message: "this repository is not selected for an active installation".to_owned(),
                details: serde_json::json!({
                    "repository_id": repository_id.map(RepositoryId::as_str),
                    "remote": remote,
                }),
            });
        };
        Ok((configured, found.clone()))
    }

    /// A short-lived installation token for `tenant`'s repository
    /// `repository_id`, narrowed to that repository and to Contents read,
    /// Pull requests write and Checks write.
    ///
    /// Background work calls this instead of holding any user token. The
    /// tenant must be associated on the configured host with an unsuspended
    /// installation whose selection includes the repository, and GitHub
    /// must still let the installation reach it. No SQL transaction is open
    /// while GitHub mints; the token is handed out only if the slot is still
    /// at the revision it was minted under, so a disconnect, release,
    /// suspension or selection change committed meanwhile discards it. A
    /// new request is decided on current access.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubNotConfigured`],
    /// [`WyrdError::GithubNotConnected`] when nothing is associated on the
    /// configured host, [`WyrdError::GithubRepositoryInaccessible`] when the
    /// installation is suspended, the repository is not selected, or GitHub
    /// refuses it, [`WyrdError::GithubConnectionConflict`]
    /// (`connection_changed`) when the slot changed while GitHub minted,
    /// [`WyrdError::GithubUnavailable`], or a registry error.
    pub async fn installation_token(
        &self,
        tenant: DataTenantId,
        repository_id: u64,
    ) -> Result<SecretString, WyrdError> {
        self.installation_access(tenant, None, repository_id)
            .await
            .map(|(token, _)| token)
    }

    /// An installation token as [`Self::installation_token`] mints it, and
    /// the slot binding it was minted under; when `installation` is given,
    /// the slot must still be associated with that installation.
    ///
    /// A webhook passes the installation it was routed by, so a delivery
    /// for an installation the tenant no longer holds is never served by
    /// the one that replaced it.
    ///
    /// # Errors
    /// The errors of [`Self::installation_token`], and
    /// [`WyrdError::GithubRepositoryInaccessible`] when the slot holds
    /// another installation.
    pub(super) async fn installation_access(
        &self,
        tenant: DataTenantId,
        installation: Option<i64>,
        repository_id: u64,
    ) -> Result<(SecretString, GithubSlotBinding), WyrdError> {
        let app = self.app()?;
        let mut conn = self.conn(tenant).await?;
        let slot = github_installation(&mut conn)
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        let Some(slot) = Self::associated(app, slot.as_ref()) else {
            return Err(WyrdError::GithubNotConnected {
                message: "this tenant has no GitHub installation associated".to_owned(),
                details: serde_json::json!({}),
            });
        };
        let repository = repository_id.to_string();
        let selected = slot
            .repositories
            .0
            .iter()
            .any(|entry| entry.repository_id == repository);
        let routed = installation.is_none_or(|routed| slot.installation_id == Some(routed));
        let installation = slot
            .installation_id
            .and_then(|id| u64::try_from(id).ok())
            .filter(|_| routed && selected && slot.suspended_at.is_none());
        let Some(installation) = installation else {
            return Err(WyrdError::GithubRepositoryInaccessible {
                message: "this repository is not selected for an active installation".to_owned(),
                details: serde_json::json!({ "repository_id": repository }),
            });
        };
        let token = app.installation_token(installation, repository_id).await?;
        let mut conn = self.conn(tenant).await?;
        let current = github_installation(&mut conn)
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        if current.map(|row| row.binding()) != Some(slot.binding()) {
            drop(token);
            return Err(connection_changed());
        }
        Ok((token, slot.binding()))
    }

    /// Lock `conn`'s tenant connection slot for the rest of its transaction
    /// and refuse unless it is still at `binding`.
    ///
    /// Every change to the association or its access advances the slot
    /// revision under this lock, so work written after this check commits
    /// only under the access it was decided with, and an access change that
    /// commits first makes this refuse.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubConnectionConflict`]
    /// (`connection_changed`) when the slot moved past `binding`, or a
    /// registry error.
    pub(super) async fn hold_binding(
        conn: &mut TenantConn<'_>,
        binding: GithubSlotBinding,
    ) -> Result<(), WyrdError> {
        let slot = lock_github_installation(conn)
            .await
            .map_err(registry_db_error)?;
        if slot.map(|row| row.binding()) != Some(binding) {
            return Err(connection_changed());
        }
        Ok(())
    }
}
