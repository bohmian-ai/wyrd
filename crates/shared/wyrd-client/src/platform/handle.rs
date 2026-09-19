//! The platform control-plane handle.

use std::sync::Arc;

use reqwest::Method;
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use serde::de::DeserializeOwned;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    ConfigurePlatformOidcRequest, CreateTenantRequest, CreateTenantResponse,
    PlatformOidcConnectionView, PlatformPrincipalListResponse, PlatformTokenRequest, PrincipalId,
    ProvisionedTenantAdmin, RecoverTenantAdminRequest, RegisterPlatformAdminRequest,
    RegisterPlatformAdminResponse, SecretBearer, SetPlatformPrincipalStatusRequest,
};
use wyrd_spec::error::WyrdError;

use crate::auth::{AuthMiddleware, TokenExchange};
use crate::client::WyrdClient;
use crate::config::ClientConfig;
use crate::transport::config::HttpConfig;
use crate::transport::credential::ResolvedCredential;
use crate::transport::http::HttpTransport;

/// A short-lived platform session token.
///
/// Held rather than re-exchanged per call, because the exchange is the one
/// operation that reads credential material and doing it repeatedly would move
/// a secret through the process far more often than the contract requires.
#[derive(Clone)]
pub struct PlatformSession(SecretString);

impl std::fmt::Debug for PlatformSession {
    /// Prints the session without its token.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PlatformSession")
            .field(&"[REDACTED]")
            .finish()
    }
}

/// Platform control-plane client.
///
/// Owns one authenticated client bound to one session. Cheap to clone; every
/// clone presents the same session, so they expire together.
#[derive(Clone, Debug)]
pub struct Platform {
    /// Shared client whose credential is the platform session, so every request
    /// travels the same transport, retry policy, and error mapping as any other
    /// Wyrd call.
    client: Arc<WyrdClient>,
}

impl Platform {
    /// Exchange a platform credential for a session and bind a client to it.
    ///
    /// This is the only call that reads credential material. The session it
    /// returns becomes the client's bearer, so every later request presents it
    /// on `X-Wyrd-Access-Token` like any other Wyrd token — the plane is
    /// separated by the session's scope marker and by the extractor its routes
    /// declare, never by which header carried the token.
    ///
    /// # Errors
    /// Returns [`WyrdError::Unauthenticated`] for every credential rejection,
    /// indistinguishably, and a transport error when the server cannot be
    /// reached.
    pub async fn connect(base_url: &str, credential: &SecretString) -> Result<Self, WyrdError> {
        let config = ClientConfig {
            http: HttpConfig {
                base_url: base_url.trim_end_matches('/').to_owned(),
                ..HttpConfig::default()
            },
            ..ClientConfig::default()
        };
        let exchange = TokenExchange::new(&config.http.base_url, config.http.timeout_ms)
            .map_err(WyrdError::from)?;
        let response = exchange
            .platform_session(&PlatformTokenRequest {
                credential: SecretBearer::new(credential.expose_secret().to_owned()),
            })
            .await
            .map_err(crate::auth::AuthError::into_wyrd)?;
        let session = PlatformSession(SecretString::from(
            response.access_token.expose().to_owned(),
        ));
        Self::with_session(config, session)
    }

    /// Bind a client to an already minted session.
    ///
    /// # Errors
    /// Returns a transport error when the HTTP client cannot be assembled.
    fn with_session(config: ClientConfig, session: PlatformSession) -> Result<Self, WyrdError> {
        let auth = AuthMiddleware::new(&config, ResolvedCredential::BearerToken(session.0.clone()))
            .map_err(WyrdError::from)?;
        let http = HttpTransport::new(&config.http, Arc::clone(&auth)).map_err(WyrdError::from)?;
        Ok(Self {
            client: Arc::new(WyrdClient::from_parts(auth, http, config.grpc)),
        })
    }

    /// Provision a tenant and receive its one-time administrative credential.
    ///
    /// The credential is the only way into the new tenant and is returned
    /// exactly once. Store it before dropping the response.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks tenant creation, the slug is
    /// taken, or provisioning fails.
    pub async fn create_tenant(
        &self,
        request: &CreateTenantRequest,
    ) -> Result<CreateTenantResponse, WyrdError> {
        self.call(Method::POST, "/platform/tenants", Some(request))
            .await
    }

    /// Issue a replacement credential for a tenant that has lost every one.
    ///
    /// Restores the existing administrative principal rather than creating a
    /// second, so the tenant's roles and history are untouched.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is unauthorized or the tenant has
    /// no administrative principal.
    pub async fn recover_tenant_admin(
        &self,
        tenant_id: DataTenantId,
    ) -> Result<ProvisionedTenantAdmin, WyrdError> {
        self.call(
            Method::POST,
            "/platform/tenants/admin/credentials",
            Some(&RecoverTenantAdminRequest { tenant_id }),
        )
        .await
    }

    /// Install or replace the deployment's platform OIDC connection.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is unauthorized, the issuer cannot
    /// be screened and discovered, or the write fails.
    pub async fn configure_oidc(
        &self,
        request: &ConfigurePlatformOidcRequest,
    ) -> Result<PlatformOidcConnectionView, WyrdError> {
        self.call(Method::PUT, "/platform/oidc/connection", Some(request))
            .await
    }

    /// Read the configured connection, which never carries the provider secret.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is unauthorized or no connection is
    /// configured.
    pub async fn oidc_connection(&self) -> Result<PlatformOidcConnectionView, WyrdError> {
        self.call::<(), _>(Method::GET, "/platform/oidc/connection", None)
            .await
    }

    /// Pre-register a human platform administrator.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is unauthorized, no connection is
    /// configured, or the name or claim is already registered.
    pub async fn register_admin(
        &self,
        request: &RegisterPlatformAdminRequest,
    ) -> Result<RegisterPlatformAdminResponse, WyrdError> {
        self.call(Method::POST, "/platform/admins", Some(request))
            .await
    }

    /// List every platform principal, including suspended ones.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is unauthorized or the read fails.
    pub async fn list_admins(&self) -> Result<PlatformPrincipalListResponse, WyrdError> {
        self.call::<(), _>(Method::GET, "/platform/admins", None)
            .await
    }

    /// Suspend or restore a platform principal.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller is unauthorized, the principal is
    /// unknown, or the change would leave the deployment with no active
    /// principal.
    pub async fn set_admin_status(
        &self,
        principal_id: &PrincipalId,
        status: &str,
    ) -> Result<(), WyrdError> {
        let request = SetPlatformPrincipalStatusRequest {
            status: status.to_owned(),
        };
        self.call_no_content(
            Method::PUT,
            &format!("/platform/admins/{principal_id}/status"),
            Some(&request),
        )
        .await
    }

    /// Send one authenticated platform request expecting a JSON body.
    ///
    /// # Errors
    /// Returns the server's stable Wyrd error, or a transport error.
    async fn call<S, D>(&self, method: Method, path: &str, body: Option<&S>) -> Result<D, WyrdError>
    where
        S: Serialize,
        D: DeserializeOwned,
    {
        self.client.request_json(method, path, body).await
    }

    /// Send one authenticated platform request expecting no body.
    ///
    /// The shared transport decodes an empty `2xx` body as JSON `null`, so the
    /// no-content routes need no second code path — only a unit target.
    ///
    /// # Errors
    /// Returns the server's stable Wyrd error, or a transport error.
    async fn call_no_content<S>(
        &self,
        method: Method,
        path: &str,
        body: Option<&S>,
    ) -> Result<(), WyrdError>
    where
        S: Serialize,
    {
        self.client.request_json::<S, ()>(method, path, body).await
    }
}
