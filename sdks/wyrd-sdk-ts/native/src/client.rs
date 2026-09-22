//! Thin napi projection of the shared [`wyrd_client::WyrdClient`].
//!
//! Construction resolves exactly as the Rust client does, and
//! [`NativeWyrdClient::on_behalf_of`] calls the Rust RFC 8693 exchange. Token
//! exchange, caching, renewal, and errors stay in the Rust owner.

use napi_derive::napi;
use secrecy::SecretString;
use wyrd_client::WyrdClient;
use wyrd_spec::auth::TokenAudience;
use wyrd_spec::error::WyrdError;

use crate::NativeWyrdError;

/// Node-facing handle to one authenticated [`WyrdClient`].
#[napi]
pub struct NativeWyrdClient {
    /// Shared transport plus authentication state.
    client: WyrdClient,
}

/// Closed result of building or delegating one client: a handle or a catalog error.
#[napi(object, object_from_js = false)]
pub struct NativeWyrdClientResult {
    /// Client when construction or the exchange succeeded.
    pub client: Option<NativeWyrdClient>,
    /// Catalog failure otherwise.
    pub error: Option<NativeWyrdError>,
}

impl NativeWyrdClientResult {
    /// Project one client outcome onto the closed result.
    fn from_outcome(outcome: Result<WyrdClient, WyrdError>) -> Self {
        match outcome {
            Ok(client) => Self {
                client: Some(NativeWyrdClient { client }),
                error: None,
            },
            Err(error) => Self {
                client: None,
                error: Some(NativeWyrdError::from_wyrd(&error)),
            },
        }
    }
}

/// Builds one client without performing IO.
///
/// Omitted arguments resolve through `client_from_options`: the environment,
/// then `~/.config/wyrd/credentials.toml`. Failures are returned as catalog
/// metadata.
#[napi]
pub fn connect_wyrd_client(
    server_url: Option<String>,
    credential: Option<String>,
    grpc_url: Option<String>,
) -> NativeWyrdClientResult {
    let client = wyrd_client::bifrost::client_from_options(
        server_url.as_deref(),
        credential.as_deref(),
        grpc_url.as_deref(),
    );
    drop(server_url);
    drop(credential);
    drop(grpc_url);
    NativeWyrdClientResult::from_outcome(client.map_err(|error| WyrdError::from(&error)))
}

#[napi]
impl NativeWyrdClient {
    /// Returns a client that acts for the holder of `subject_token`.
    ///
    /// This client's credential is the actor. The first exchange runs here so a
    /// refusal is returned at the call site; the returned client re-exchanges
    /// in Rust before expiry. An `audience` other than `wyrd` or `bifrost` is a
    /// `WYRD_SPEC_400_VALIDATION` result.
    #[napi]
    pub async fn on_behalf_of(
        &self,
        subject_token: String,
        audience: String,
    ) -> NativeWyrdClientResult {
        let audience: TokenAudience =
            match serde_json::from_value(serde_json::Value::String(audience)) {
                Ok(audience) => audience,
                Err(error) => {
                    return NativeWyrdClientResult::from_outcome(Err(WyrdError::Validation {
                        message: format!("audience is invalid: {error}"),
                        details: serde_json::json!({ "field": "audience" }),
                    }));
                }
            };
        NativeWyrdClientResult::from_outcome(
            self.client
                .on_behalf_of(SecretString::from(subject_token), audience)
                .await,
        )
    }
}
