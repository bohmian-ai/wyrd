//! Network policy for provider endpoints.
//!
//! Provider calls go through Skald provider clients; this module owns only the
//! egress policy those clients run under. [`EndpointPolicy::admits`] screens a
//! base URL's scheme, port, and literal address before an attempt, and
//! [`EndpointPolicy::transport`] builds the Skald transport whose DNS resolver
//! screens every resolved address at connection time, so a rebinding answer
//! cannot reach a blocked address. Certificate verification stays intact, no
//! proxy is consulted, and redirects are never followed.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::Duration;

use reqwest::ClientBuilder;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::redirect::Policy;
use skald_providers::{HttpTransport, ProviderResult, TransportConfig};
use url::{Host, Url};

/// Bound on one DNS resolution.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

/// Bound on establishing one TCP and TLS connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Backstop on one whole provider exchange; the engine's call deadline is the
/// operative bound.
const REQUEST_TIMEOUT: Duration = Duration::from_mins(10);

/// Network policy for tenant-controlled endpoints.
///
/// Cloud-metadata, link-local, broadcast, and documentation addresses are
/// rejected in every profile. A production profile also rejects loopback,
/// private, carrier-grade NAT, unspecified, and unique-local addresses, and
/// admits only `https` on port 443; other profiles admit `http` and `https`
/// on any port so self-hosted and local providers remain reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointPolicy {
    /// Whether the production restrictions apply.
    production: bool,
}

impl EndpointPolicy {
    /// Builds the policy for a production or non-production deployment.
    #[must_use]
    pub const fn new(production: bool) -> Self {
        Self { production }
    }

    /// Whether a connection to `ip` is permitted.
    ///
    /// An IPv4-mapped IPv6 address is judged as its IPv4 form so either
    /// spelling of a blocked range is caught.
    #[must_use]
    pub fn permits(self, ip: IpAddr) -> bool {
        let ip = match ip {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
            v4 @ IpAddr::V4(_) => v4,
        };
        !(always_blocked(ip) || (self.production && internal(ip)))
    }

    /// Whether `url` may be called: its scheme and port are approved and a
    /// literal IP host is permitted.
    ///
    /// Hostnames are screened when they resolve, by the resolver of
    /// [`EndpointPolicy::transport`]; literal addresses never reach a
    /// resolver, so they are screened here.
    #[must_use]
    pub fn admits(self, url: &Url) -> bool {
        let Some(port) = url.port_or_known_default() else {
            return false;
        };
        let approved = match url.scheme() {
            "https" => !self.production || port == 443,
            "http" => !self.production,
            _ => false,
        };
        approved && self.permits_host(url)
    }

    /// Whether `url` has a host and, when that host is a literal IP address,
    /// the address is permitted; hostnames are left to the screening
    /// resolver.
    #[must_use]
    pub(crate) fn permits_host(self, url: &Url) -> bool {
        match url.host() {
            Some(Host::Domain(_)) => true,
            Some(Host::Ipv4(ip)) => self.permits(IpAddr::V4(ip)),
            Some(Host::Ipv6(ip)) => self.permits(IpAddr::V6(ip)),
            None => false,
        }
    }

    /// Builds the Skald provider transport enforcing this policy.
    ///
    /// Every hostname resolves once under a bounded timeout and is refused
    /// when any answer is blocked; redirects are returned as answers rather
    /// than followed; no proxy is consulted. The transport is shared by every
    /// attempt, so connections are pooled per resolved endpoint.
    ///
    /// # Errors
    ///
    /// Returns the Skald transport error when the TLS provider cannot be
    /// installed or the client cannot be built.
    pub fn transport(self) -> ProviderResult<HttpTransport> {
        let config = TransportConfig {
            timeout: REQUEST_TIMEOUT,
            connect_timeout: CONNECT_TIMEOUT,
            ..TransportConfig::default()
        };
        HttpTransport::customized(config, |builder| self.screened(builder))
    }

    /// Applies this policy's egress screening to `builder`.
    ///
    /// Every hostname resolves through a resolver that refuses the whole
    /// answer when any address is blocked, and the connection uses exactly
    /// those screened answers; redirects are not followed and no proxy is
    /// consulted. Literal addresses bypass resolvers, so callers screen them
    /// with [`EndpointPolicy::permits_host`] before sending.
    pub(crate) fn screened(self, builder: ClientBuilder) -> ClientBuilder {
        builder
            .dns_resolver(Arc::new(ScreeningResolver { policy: self }))
            .redirect(Policy::none())
            .no_proxy()
    }
}

/// `100.64.0.0/10`, RFC 6598 carrier-grade NAT space.
fn cgnat(v4: Ipv4Addr) -> bool {
    let [a, b, ..] = v4.octets();
    a == 100 && (b & 0xc0) == 0x40
}

/// `fc00::/7`, RFC 4193 unique-local space.
fn unique_local(v6: Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xfe00) == 0xfc00
}

/// Addresses no deployment may reach; link-local covers the cloud instance
/// metadata endpoint `169.254.169.254`, and the metadata endpoints outside it,
/// `100.100.100.200` and `fd00:ec2::254`, are named exactly so neighboring
/// carrier-grade NAT and unique-local addresses keep their profile rules.
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

/// Internal addresses a production deployment must not reach.
fn internal(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_unspecified() || cgnat(v4),
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified() || unique_local(v6),
    }
}

/// Why a hostname was not resolved for a provider connection. Names no host
/// or address, so it is safe to log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
enum ResolveError {
    /// The lookup failed, timed out, or returned nothing.
    #[error("provider endpoint host could not be resolved")]
    Unresolvable,
    /// An answer is blocked by the endpoint policy.
    #[error("provider endpoint resolves to a blocked address")]
    Blocked,
}

/// DNS resolver that screens every answer against an [`EndpointPolicy`].
///
/// The connection uses exactly the screened answers, so there is no gap
/// between screening and connecting for a rebinding server to exploit.
#[derive(Debug, Clone, Copy)]
struct ScreeningResolver {
    /// Policy every resolved address must satisfy.
    policy: EndpointPolicy,
}

impl Resolve for ScreeningResolver {
    /// Resolves `name` once under [`RESOLVE_TIMEOUT`], rejecting the whole
    /// answer when it is empty or any address is blocked; rejecting on any
    /// blocked record defeats split-horizon answers mixing public and internal
    /// addresses.
    fn resolve(&self, name: Name) -> Resolving {
        let policy = self.policy;
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
            if !addrs.iter().all(|addr| policy.permits(addr.ip())) {
                return Err(ResolveError::Blocked.into());
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Parses a URL fixture.
    fn url(value: &str) -> Url {
        Url::parse(value).expect("url")
    }

    /// Metadata and link-local ranges are blocked in every profile, internal
    /// ranges only in production, and public addresses never.
    #[test]
    fn address_policy_blocks_metadata_always_and_internal_in_production() {
        for policy in [EndpointPolicy::new(false), EndpointPolicy::new(true)] {
            for raw in [
                "169.254.169.254",
                "::ffff:169.254.169.254",
                "fe80::1",
                "100.100.100.200",
                "::ffff:100.100.100.200",
                "fd00:ec2::254",
            ] {
                assert!(!policy.permits(raw.parse().expect("ip")), "{raw}");
            }
            for raw in ["8.8.8.8", "2606:4700:4700::1111"] {
                assert!(policy.permits(raw.parse().expect("ip")), "{raw}");
            }
        }
        for raw in [
            "127.0.0.1",
            "10.0.0.5",
            "172.16.0.1",
            "192.168.1.1",
            "100.64.0.1",
            "100.100.100.201",
            "0.0.0.0",
            "::1",
            "fc00::1",
            "fd00:ec2::253",
            "::ffff:10.0.0.5",
        ] {
            let ip = raw.parse().expect("ip");
            assert!(!EndpointPolicy::new(true).permits(ip), "{raw}");
            assert!(EndpointPolicy::new(false).permits(ip), "{raw}");
        }
    }

    /// Production admits only https on 443; other profiles admit http too;
    /// blocked literal addresses are never admitted.
    #[test]
    fn admission_follows_profile_scheme_port_and_literal_address() {
        let production = EndpointPolicy::new(true);
        assert!(production.admits(&url("https://api.example")));
        for raw in [
            "http://api.example",
            "https://api.example:8443",
            "ftp://api.example",
            "https://10.0.0.5",
        ] {
            assert!(!production.admits(&url(raw)), "{raw}");
        }
        let dev = EndpointPolicy::new(false);
        assert!(dev.admits(&url("http://127.0.0.1:8080")));
        assert!(!dev.admits(&url("http://169.254.169.254/latest")));
    }

    /// A hostname resolving to a blocked address never connects, and a
    /// redirect answer is returned instead of followed.
    #[tokio::test]
    async fn resolved_blocked_hosts_fail_and_redirects_are_not_followed() {
        let server = MockServer::start().await;
        let target = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", target.uri().as_str()),
            )
            .mount(&server)
            .await;
        let port = server.address().port();

        let production = EndpointPolicy::new(true).transport().expect("transport");
        let blocked = production
            .client()
            .post(format!("http://localhost:{port}/"))
            .send()
            .await
            .expect_err("loopback resolution is blocked in production");
        assert!(blocked.is_connect(), "{blocked}");
        assert!(
            server
                .received_requests()
                .await
                .expect("recorded")
                .is_empty()
        );

        let dev = EndpointPolicy::new(false).transport().expect("transport");
        let redirect = dev
            .client()
            .post(format!("http://localhost:{port}/"))
            .send()
            .await
            .expect("screened hostname connects");
        assert_eq!(redirect.status().as_u16(), 302);
        assert!(
            target
                .received_requests()
                .await
                .expect("recorded")
                .is_empty()
        );
    }

    /// A TLS endpoint presenting a certificate no trusted root signs is
    /// refused during the handshake, so a tenant-controlled endpoint cannot
    /// bypass certificate validation and no request byte reaches it.
    #[tokio::test]
    async fn untrusted_certificates_are_refused_before_any_request_byte() {
        let dev = EndpointPolicy::new(false).transport().expect("transport");
        let certified = rcgen::generate_simple_self_signed(vec![
            "localhost".to_owned(),
            "127.0.0.1".to_owned(),
        ])
        .expect("self-signed certificate");
        let key =
            rustls::pki_types::PrivateKeyDer::Pkcs8(certified.signing_key.serialize_der().into());
        let config = Arc::new(
            rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![certified.cert.der().clone()], key)
                .expect("server config"),
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let port = listener.local_addr().expect("address").port();
        let server = std::thread::spawn(move || {
            let (mut tcp, _) = listener.accept().expect("client connects");
            let mut tls = rustls::ServerConnection::new(config).expect("server connection");
            let mut request = Vec::new();
            std::io::Read::read_to_end(&mut rustls::Stream::new(&mut tls, &mut tcp), &mut request)
                .map(|_| request)
        });

        let error = dev
            .client()
            .post(format!("https://127.0.0.1:{port}/"))
            .body("request-canary")
            .send()
            .await
            .expect_err("an untrusted certificate is refused");
        assert!(error.is_connect(), "{error}");
        let received = tokio::task::spawn_blocking(move || server.join().expect("server thread"))
            .await
            .expect("server join");
        assert!(
            received.is_err() || received.is_ok_and(|bytes| bytes.is_empty()),
            "no request byte is decrypted by the untrusted endpoint"
        );
    }
}
