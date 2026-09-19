//! The platform control-plane handle.

use std::sync::Arc;

use reqwest::{Method, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use serde::de::DeserializeOwned;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    ConfigurePlatformOidcRequest, CreateTenantRequest, CreateTenantResponse,
    PlatformOidcConnectionView, PlatformPrincipalListResponse, PlatformTokenRequest,
    PlatformTokenResponse, PrincipalId, ProvisionedTenantAdmin, RecoverTenantAdminRequest,
    RegisterPlatformAdminRequest, RegisterPlatformAdminResponse, SecretBearer,
    SetPlatformPrincipalStatusRequest,
};
use wyrd_spec::error::WyrdError;

use crate::error::{WyrdClientError, from_problem_json};

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
/// Owns the server address and one session. Cheap to clone; every clone
/// presents the same session, so they expire together.
#[derive(Clone)]
pub struct Platform {
    /// Server base URL, without a trailing slash.
    base_url: Arc<str>,
    /// Shared HTTP pool.
    http: reqwest::Client,
    /// The session every request after the exchange presents.
    session: PlatformSession,
}

impl std::fmt::Debug for Platform {
    /// Prints the handle without its session.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Platform")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl Platform {
    /// Exchange a platform credential for a session and bind a client to it.
    ///
    /// This is the only call that reads credential material. Everything after
    /// it presents the returned session, so no later request handles a secret —
    /// the same shape the server's own surface enforces.
    ///
    /// # Errors
    /// Returns [`WyrdError::Unauthenticated`] for every credential rejection,
    /// indistinguishably, and a transport error when the server cannot be
    /// reached.
    pub async fn connect(base_url: &str, credential: &SecretString) -> Result<Self, WyrdError> {
        let base_url: Arc<str> = Arc::from(base_url.trim_end_matches('/'));
        let http = reqwest::Client::new();
        let request = PlatformTokenRequest {
            credential: SecretBearer::new(credential.expose_secret().to_owned()),
        };
        let response: PlatformTokenResponse = send(
            &http,
            &base_url,
            Method::POST,
            "/auth/platform/token",
            None,
            Some(&request),
        )
        .await?;

        Ok(Self {
            base_url,
            http,
            session: PlatformSession(SecretString::from(
                response.access_token.expose().to_owned(),
            )),
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
        send(
            &self.http,
            &self.base_url,
            method,
            path,
            Some(&self.session),
            body,
        )
        .await
    }

    /// Send one authenticated platform request expecting no body.
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
        let response = dispatch(
            &self.http,
            &self.base_url,
            method,
            path,
            Some(&self.session),
            body,
        )
        .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(error_from(response).await)
        }
    }
}

/// Send a request and decode its JSON body.
///
/// # Errors
/// Returns the server's stable Wyrd error when the status is not a success, and
/// a transport error when the request cannot be sent or decoded.
async fn send<S, D>(
    http: &reqwest::Client,
    base_url: &str,
    method: Method,
    path: &str,
    session: Option<&PlatformSession>,
    body: Option<&S>,
) -> Result<D, WyrdError>
where
    S: Serialize,
    D: DeserializeOwned,
{
    let response = dispatch(http, base_url, method, path, session, body).await?;
    if !response.status().is_success() {
        return Err(error_from(response).await);
    }
    response.json::<D>().await.map_err(|error| {
        WyrdError::from(WyrdClientError::TransportDown {
            message: format!("platform response could not be decoded: {error}"),
            transport: "http".to_owned(),
        })
    })
}

/// Build and send one request, without interpreting its status.
///
/// The session travels on `Authorization: Bearer`, the header the platform
/// plane reads. A tenant access token is never attached here, and the tenant
/// header is never attached at all, so this client cannot present a tenant
/// identity to a platform route.
///
/// # Errors
/// Returns a transport error when the request cannot be built or sent.
async fn dispatch<S>(
    http: &reqwest::Client,
    base_url: &str,
    method: Method,
    path: &str,
    session: Option<&PlatformSession>,
    body: Option<&S>,
) -> Result<reqwest::Response, WyrdError>
where
    S: Serialize,
{
    let mut request = http.request(method, format!("{base_url}{path}"));
    if let Some(session) = session {
        request = request.header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", session.0.expose_secret()),
        );
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    request.send().await.map_err(|error| {
        WyrdError::from(WyrdClientError::TransportDown {
            message: error.to_string(),
            transport: "http".to_owned(),
        })
    })
}

/// Map a non-success response onto the stable catalog.
///
/// An undecodable error body still becomes a Wyrd error rather than a panic or
/// a silent success: a failed request must never be reported as anything else.
async fn error_from(response: reqwest::Response) -> WyrdError {
    let status = response.status();
    match response.json::<serde_json::Value>().await {
        Ok(body) => from_problem_json(&body),
        Err(_) if status == StatusCode::UNAUTHORIZED => WyrdError::Unauthenticated {
            message: "invalid platform session".to_owned(),
            details: serde_json::json!({ "plane": "platform" }),
        },
        Err(error) => WyrdError::from(WyrdClientError::TransportDown {
            message: format!("platform error body could not be decoded: {error}"),
            transport: "http".to_owned(),
        }),
    }
}
