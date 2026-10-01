//! The one HashiCorp Vault KV v2 reader Wyrd uses.
//!
//! The gateway reads provider credentials and the server reads Operator
//! key-encryption keys through [`VaultKv2`]. Each caller keeps its own
//! configuration, token source, and interpretation of the value it reads;
//! this crate owns only the network read: TLS trust, bounded connect and total
//! time, a bounded response body, the token and namespace headers, and egress
//! screening. Every read issues exactly one latest-version request and caches
//! nothing, so rotation and revocation apply to the next read. Neither the
//! token nor the value ever enters a URL, log, or error.
//!
//! Egress screening refuses a literal metadata, link-local, broadcast, or
//! documentation address before any request, and resolves a hostname through a
//! resolver that refuses the whole answer when any address is in those ranges;
//! the connection is pinned to exactly the screened answers. Internal Vault
//! addresses stay reachable because Vault is operator-configured, not
//! tenant-supplied.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::Duration;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::header::HeaderValue;
use reqwest::redirect::Policy;
use reqwest::{Certificate, Client, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use url::{Host, Url};

/// Total time one Vault read may take.
const VAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Time allowed to establish the Vault connection.
const VAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Bound on one DNS resolution of the Vault host.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

/// Largest Vault response body read, in bytes; larger bodies are unavailable.
const MAX_VAULT_RESPONSE_BYTES: usize = 256 * 1024;

/// Why a Vault read produced no value.
///
/// The classes are closed and carry no address, path, token, or Vault text,
/// so callers can map them onto their own domain errors and log them safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VaultError {
    /// HTTP 404, a null or absent secret, a missing or non-string field, or a
    /// deleted or destroyed latest version.
    #[error("the Vault secret or field is missing")]
    Missing,
    /// HTTP 400, 403, or 405, a token that is not a valid header value, or a
    /// literal address in a blocked range, refused before any request.
    #[error("Vault refused the read")]
    Refused,
    /// A timeout, connection failure (including a hostname resolving to a
    /// blocked address), any other status, or an oversized or undecodable
    /// body.
    #[error("Vault is unavailable")]
    Unavailable,
}

/// A bounded, screened, and pinned client for one Vault KV v2 mount.
///
/// The client holds no token and no default headers; the caller supplies its
/// current token on every read, so the derived `Debug` prints nothing secret.
#[derive(Debug, Clone)]
pub struct VaultKv2 {
    /// Vault server base address.
    address: Url,
    /// KV v2 mount path, possibly several segments, without surrounding `/`.
    mount: String,
    /// Optional Vault Enterprise namespace.
    namespace: Option<String>,
    /// Egress screening of the address literal and every resolved answer.
    egress: Egress,
    /// Redirect-free, proxy-free, time-bounded client using the configured
    /// trust and the screening resolver.
    client: Client,
}

impl VaultKv2 {
    /// Builds a reader for `mount` at `address`.
    ///
    /// `ca_cert` is a PEM bundle that replaces the platform trust roots when
    /// present. The client follows no redirects, consults no proxy, bounds
    /// connect and total time, and resolves hostnames through the screening
    /// resolver. Wyrd's Rustls provider is installed first; a conflicting
    /// provider installed earlier still serves the client.
    ///
    /// # Errors
    ///
    /// Returns [`reqwest::Error`] when the CA bundle does not parse or the
    /// client cannot be built.
    pub fn new(
        address: Url,
        mount: &str,
        namespace: Option<String>,
        ca_cert: Option<&[u8]>,
    ) -> Result<Self, reqwest::Error> {
        Self::with_egress(
            address,
            mount,
            namespace,
            ca_cert,
            Egress {
                block_internal: false,
            },
        )
    }

    /// Builds a reader whose `egress` screens the literal address and every
    /// resolved answer; [`VaultKv2::new`] fixes the operator-configured
    /// policy that keeps internal addresses reachable.
    ///
    /// # Errors
    ///
    /// Returns [`reqwest::Error`] when the CA bundle does not parse or the
    /// client cannot be built.
    fn with_egress(
        address: Url,
        mount: &str,
        namespace: Option<String>,
        ca_cert: Option<&[u8]>,
        egress: Egress,
    ) -> Result<Self, reqwest::Error> {
        wyrd_tls::install_crypto_provider().ok();
        let mut builder = Client::builder()
            .timeout(VAULT_TIMEOUT)
            .connect_timeout(VAULT_CONNECT_TIMEOUT)
            .dns_resolver(Arc::new(ScreeningResolver { egress }))
            .redirect(Policy::none())
            .no_proxy();
        if let Some(pem) = ca_cert {
            builder = builder.tls_certs_only(Certificate::from_pem_bundle(pem)?);
        }
        Ok(Self {
            address,
            mount: mount.trim_matches('/').to_owned(),
            namespace,
            egress,
            client: builder.build()?,
        })
    }

    /// Reads the latest version of the secret at `path` and returns the
    /// string at `data.data.<field>`.
    ///
    /// Sends `GET {address}/v1/{mount}/data/{path}` with no `version`
    /// parameter, every path segment percent-encoded, `token` (trimmed) in a
    /// sensitive `X-Vault-Token` header, and the namespace in
    /// `X-Vault-Namespace` when set. The body is read in chunks and abandoned
    /// once it exceeds the response bound.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Refused`] for a blocked literal address (before
    /// the token is used), an invalid token header, or HTTP 400, 403, or 405;
    /// [`VaultError::Missing`] for HTTP 404, an absent or non-string field, or
    /// a deleted or destroyed latest version; and [`VaultError::Unavailable`]
    /// for transport failures, timeouts, other statuses, and oversized or
    /// undecodable bodies.
    pub async fn read_field(
        &self,
        token: &SecretString,
        path: &str,
        field: &str,
    ) -> Result<SecretString, VaultError> {
        if !self.egress.permits_host(&self.address) {
            return Err(VaultError::Refused);
        }
        let mut token =
            HeaderValue::from_str(token.expose_secret().trim()).map_err(|_| VaultError::Refused)?;
        token.set_sensitive(true);
        let mut url = self.address.clone();
        url.path_segments_mut()
            .map_err(|()| VaultError::Refused)?
            .pop_if_empty()
            .push("v1")
            .extend(self.mount.split('/'))
            .push("data")
            .extend(path.split('/'));
        let mut request = self.client.get(url).header("X-Vault-Token", token);
        if let Some(namespace) = &self.namespace {
            request = request.header("X-Vault-Namespace", namespace);
        }
        let mut response = request.send().await.map_err(|_| VaultError::Unavailable)?;
        match response.status() {
            StatusCode::OK => {}
            StatusCode::NOT_FOUND => return Err(VaultError::Missing),
            StatusCode::BAD_REQUEST | StatusCode::FORBIDDEN | StatusCode::METHOD_NOT_ALLOWED => {
                return Err(VaultError::Refused);
            }
            _ => return Err(VaultError::Unavailable),
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| VaultError::Unavailable)?
        {
            if body.len() + chunk.len() > MAX_VAULT_RESPONSE_BYTES {
                return Err(VaultError::Unavailable);
            }
            body.extend_from_slice(&chunk);
        }
        let envelope: Value = serde_json::from_slice(&body).map_err(|_| VaultError::Unavailable)?;
        let metadata = &envelope["data"]["metadata"];
        let deleted = metadata["deletion_time"]
            .as_str()
            .is_some_and(|time| !time.is_empty());
        if deleted || metadata["destroyed"] == Value::Bool(true) {
            return Err(VaultError::Missing);
        }
        envelope["data"]["data"][field]
            .as_str()
            .map(SecretString::from)
            .ok_or(VaultError::Missing)
    }
}

/// Address screening for Vault egress.
///
/// Metadata, link-local, broadcast, and documentation addresses are refused
/// always; `block_internal` additionally refuses loopback, private,
/// carrier-grade NAT, unspecified, and unique-local addresses, which only
/// tests use to prove resolved answers are screened and pinned.
#[derive(Debug, Clone, Copy)]
struct Egress {
    /// Whether internal ranges are refused as well.
    block_internal: bool,
}

impl Egress {
    /// Whether a connection to `ip` is permitted, judging an IPv4-mapped IPv6
    /// address as its IPv4 form so either spelling of a range is caught.
    fn permits(self, ip: IpAddr) -> bool {
        let ip = match ip {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
            v4 @ IpAddr::V4(_) => v4,
        };
        !(always_blocked(ip) || (self.block_internal && internal(ip)))
    }

    /// Whether `url` has a host and, when that host is a literal IP address,
    /// the address is permitted; hostnames are screened by the resolver.
    fn permits_host(self, url: &Url) -> bool {
        match url.host() {
            Some(Host::Domain(_)) => true,
            Some(Host::Ipv4(ip)) => self.permits(IpAddr::V4(ip)),
            Some(Host::Ipv6(ip)) => self.permits(IpAddr::V6(ip)),
            None => false,
        }
    }
}

/// Addresses Vault egress never reaches: link-local (including the cloud
/// metadata endpoint `169.254.169.254`), broadcast, documentation, and the
/// metadata endpoints outside link-local, `100.100.100.200` and
/// `fd00:ec2::254`.
fn always_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4 == Ipv4Addr::new(100, 100, 100, 200)
        }
        IpAddr::V6(v6) => {
            (v6.segments()[0] & 0xffc0) == 0xfe80
                || v6 == Ipv6Addr::new(0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254)
        }
    }
}

/// Internal addresses refused when [`Egress::block_internal`] is set.
fn internal(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_unspecified()
                || (a == 100 && (b & 0xc0) == 0x40)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback() || v6.is_unspecified() || (v6.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

/// Why the Vault host was not resolved. Names no host or address, so it is
/// safe to log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
enum ResolveError {
    /// The lookup failed, timed out, or returned nothing.
    #[error("Vault host could not be resolved")]
    Unresolvable,
    /// An answer is in a blocked range.
    #[error("Vault host resolves to a blocked address")]
    Blocked,
}

/// DNS resolver that screens every answer, so the connection uses exactly the
/// screened addresses and a rebinding answer cannot slip between screening and
/// connecting.
#[derive(Debug, Clone, Copy)]
struct ScreeningResolver {
    /// Screening every resolved address must satisfy.
    egress: Egress,
}

impl Resolve for ScreeningResolver {
    /// Resolves `name` once under [`RESOLVE_TIMEOUT`], refusing the whole
    /// answer when it is empty or any address is blocked.
    fn resolve(&self, name: Name) -> Resolving {
        let egress = self.egress;
        Box::pin(async move {
            let addrs: Vec<_> =
                tokio::time::timeout(RESOLVE_TIMEOUT, tokio::net::lookup_host((name.as_str(), 0)))
                    .await
                    .map_err(|_| ResolveError::Unresolvable)?
                    .map_err(|_| ResolveError::Unresolvable)?
                    .collect();
            if addrs.is_empty() {
                return Err(ResolveError::Unresolvable.into());
            }
            if !addrs.iter().all(|addr| egress.permits(addr.ip())) {
                return Err(ResolveError::Blocked.into());
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    //! Latest-version reads, outcome classes, and screened, pinned egress.

    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    /// Canary sent as the Vault token; it must reach only the header.
    const TOKEN: &str = "hvs.token-canary";

    /// The canary token.
    fn token() -> SecretString {
        SecretString::from(TOKEN)
    }

    /// Reader over `address` with mount `kv/team` and namespace `ns1`.
    fn reader(address: &str) -> VaultKv2 {
        VaultKv2::new(
            Url::parse(address).expect("address"),
            "kv/team",
            Some("ns1".to_owned()),
            None,
        )
        .expect("reader builds")
    }

    /// Answers the next read of `route` with `status` and `body`.
    async fn answer(server: &MockServer, route: &str, status: u16, body: Value) {
        server.reset().await;
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(server)
            .await;
    }

    /// A read issues one latest-version KV v2 GET with encoded segments and
    /// the trimmed token and namespace headers, and returns the named field.
    #[tokio::test]
    async fn reads_latest_version_field_with_supplied_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/kv/team/data/openai/a%20b"))
            .and(header("x-vault-token", TOKEN))
            .and(header("x-vault-namespace", "ns1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"data": {"api_key": "sk-v2"}, "metadata": {"version": 2, "deletion_time": "", "destroyed": false}}
            })))
            .mount(&server)
            .await;
        let value = reader(&server.uri())
            .read_field(
                &SecretString::from(format!("{TOKEN}\n")),
                "openai/a b",
                "api_key",
            )
            .await
            .expect("latest version resolves");
        assert_eq!(value.expose_secret(), "sk-v2");
        let requests = server.received_requests().await.expect("recording");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.query(), None, "no version parameter");
    }

    /// Every status, body, and connection outcome maps onto the closed
    /// [`VaultError`] classes.
    #[tokio::test]
    async fn maps_outcomes() {
        let server = MockServer::start().await;
        let route = "/v1/kv/team/data/openai/a%20b";
        let vault = reader(&server.uri());
        let oversized = "x".repeat(MAX_VAULT_RESPONSE_BYTES);
        let cases = [
            (404, json!({"errors": []}), VaultError::Missing),
            (
                200,
                json!({"data": {"data": null, "metadata": {}}}),
                VaultError::Missing,
            ),
            (
                200,
                json!({"data": {"data": {"other": "x"}}}),
                VaultError::Missing,
            ),
            (
                200,
                json!({"data": {"data": {"api_key": 7}}}),
                VaultError::Missing,
            ),
            (
                200,
                json!({"data": {"data": {"api_key": "old"}, "metadata": {"deletion_time": "2026-01-01T00:00:00Z"}}}),
                VaultError::Missing,
            ),
            (
                200,
                json!({"data": {"data": {"api_key": "old"}, "metadata": {"destroyed": true}}}),
                VaultError::Missing,
            ),
            (400, json!({}), VaultError::Refused),
            (
                403,
                json!({"errors": ["permission denied"]}),
                VaultError::Refused,
            ),
            (405, json!({}), VaultError::Refused),
            (500, json!({}), VaultError::Unavailable),
            (307, json!({}), VaultError::Unavailable),
            (
                200,
                json!({"data": {"data": {"api_key": oversized}}}),
                VaultError::Unavailable,
            ),
        ];
        for (status, body, expected) in cases {
            answer(&server, route, status, body).await;
            assert_eq!(
                vault
                    .read_field(&token(), "openai/a b", "api_key")
                    .await
                    .map(|_| ()),
                Err(expected),
                "{status}"
            );
        }
        server.reset().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;
        assert_eq!(
            vault
                .read_field(&token(), "openai/a b", "api_key")
                .await
                .map(|_| ()),
            Err(VaultError::Unavailable)
        );
        assert_eq!(
            vault
                .read_field(&SecretString::from("bad\ntoken"), "openai/a b", "api_key")
                .await
                .map(|_| ()),
            Err(VaultError::Refused),
            "a token that is not a header value is refused"
        );
        let address = server.uri();
        drop(server);
        assert_eq!(
            reader(&address)
                .read_field(&token(), "openai/a b", "api_key")
                .await
                .map(|_| ()),
            Err(VaultError::Unavailable)
        );
    }

    /// Metadata and link-local literal addresses are refused before any
    /// request, a hostname is screened at resolution with the connection
    /// pinned to its screened answers (an internal-blocking reader refuses the
    /// loopback fixture without it receiving a request), and the default
    /// reader still reaches the internal fixture.
    #[tokio::test]
    async fn egress_is_screened_and_pinned() {
        for literal in [
            "http://169.254.169.254",
            "http://100.100.100.200:8200",
            "http://[fe80::1]:8200",
            "http://[::ffff:169.254.169.254]",
        ] {
            assert_eq!(
                reader(literal)
                    .read_field(&token(), "openai/a b", "api_key")
                    .await
                    .map(|_| ()),
                Err(VaultError::Refused),
                "{literal}"
            );
        }
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data": {"data": {"api_key": "sk-internal"}}})),
            )
            .mount(&server)
            .await;
        let address = format!("http://localhost:{}", server.address().port());
        let screened = VaultKv2::with_egress(
            Url::parse(&address).expect("address"),
            "kv/team",
            None,
            None,
            Egress {
                block_internal: true,
            },
        )
        .expect("reader builds");
        assert_eq!(
            screened
                .read_field(&token(), "openai/a b", "api_key")
                .await
                .map(|_| ()),
            Err(VaultError::Unavailable)
        );
        assert!(
            server
                .received_requests()
                .await
                .expect("recording")
                .is_empty()
        );
        let internal = reader(&address)
            .read_field(&token(), "openai/a b", "api_key")
            .await
            .expect("internal Vault resolves");
        assert_eq!(internal.expose_secret(), "sk-internal");
    }
}
