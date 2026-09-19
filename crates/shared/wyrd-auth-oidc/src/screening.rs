//! Outbound address screening for identity-provider requests.
//!
//! Every fetch Wyrd makes to a provider is driven by a URL someone configured:
//! an issuer, its token endpoint, its JWKS endpoint. That makes each of them a
//! server-side request forgery primitive unless the address behind the name is
//! checked — and checked at the moment of the request, because a name screened
//! at configuration time can resolve somewhere else an hour later.
//!
//! This module owns that check and the client that enforces it. One policy, one
//! resolver, one pinned client: discovery, token exchange, and JWKS refresh all
//! go through it so none of them can be the path that forgot.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use url::Url;

/// How long any provider fetch may take.
///
/// Bounded because these requests sit in a login path: a provider that hangs
/// must fail the login, not hold the connection.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Which internal address ranges a deployment may legitimately reach.
///
/// Multi-tenant deployments accept issuer URLs from semi-trusted tenant
/// administrators, so the whole internal space is off limits. Self-hosted,
/// enterprise single-tenant, and development deployments legitimately run the
/// identity provider on loopback or a private network, and blocking those would
/// make the product unusable there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressPolicy {
    /// Block loopback, private, CGNAT, and unique-local addresses as well.
    BlockInternal,
    /// Allow internal addresses; metadata and link-local stay blocked.
    AllowInternal,
}

/// Why a provider fetch could not be made.
#[derive(Debug, thiserror::Error)]
pub enum ScreenError {
    /// The URL has no host, or resolves to an address Wyrd must never reach.
    #[error("issuer resolves to a blocked address range")]
    Blocked,
    /// The host could not be resolved at all.
    #[error("issuer host could not be resolved")]
    Unresolved,
    /// The HTTP client could not be constructed.
    #[error("provider client could not be constructed")]
    Client,
}

/// Builds screened, pinned HTTP clients for identity-provider requests.
///
/// Holds only the policy, so it is cheap to copy into every owner that makes a
/// provider request rather than being threaded as a shared handle.
#[derive(Debug, Clone, Copy)]
pub struct ScreenedHttp {
    /// The internal-range policy this deployment runs under.
    policy: AddressPolicy,
}

impl ScreenedHttp {
    /// Bind screening to a deployment's address policy.
    #[must_use]
    pub const fn new(policy: AddressPolicy) -> Self {
        Self { policy }
    }

    /// The policy a development or single-tenant deployment runs under.
    ///
    /// Also what tests use, since their mock providers listen on loopback.
    #[must_use]
    pub const fn allowing_internal() -> Self {
        Self::new(AddressPolicy::AllowInternal)
    }

    /// Build a client that can only reach `url`'s screened addresses.
    ///
    /// A literal-IP host is screened directly. A name is resolved, every
    /// resolved address is screened, and the returned client is pinned to those
    /// addresses — so the answer that passed the screen is the answer the
    /// connection uses, and a rebinding reply between the two cannot redirect it
    /// inward. Redirects are refused outright, because following one would leave
    /// the screen behind.
    ///
    /// Rejecting on *any* blocked record rather than filtering them out is
    /// deliberate: split-horizon DNS that mixes one public and one internal
    /// answer is the attack, not a mixed-quality result to be salvaged.
    ///
    /// # Errors
    /// Returns [`ScreenError::Blocked`] when the URL has no host or any
    /// resolved address is blocked, [`ScreenError::Unresolved`] when resolution
    /// fails, and [`ScreenError::Client`] when TLS setup or client construction
    /// fails.
    pub async fn client_for(&self, url: &Url) -> Result<reqwest::Client, ScreenError> {
        wyrd_tls::install_crypto_provider().map_err(|_| ScreenError::Client)?;
        let builder = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none());

        let builder = match url.host() {
            Some(url::Host::Ipv4(addr)) => {
                if self.is_blocked(IpAddr::V4(addr)) {
                    return Err(ScreenError::Blocked);
                }
                builder
            }
            Some(url::Host::Ipv6(addr)) => {
                if self.is_blocked(IpAddr::V6(addr)) {
                    return Err(ScreenError::Blocked);
                }
                builder
            }
            Some(url::Host::Domain(domain)) => {
                let port = url.port_or_known_default().unwrap_or(443);
                let addrs = self.resolve_and_screen(domain, port).await?;
                builder.resolve_to_addrs(domain, &addrs)
            }
            None => return Err(ScreenError::Blocked),
        };

        builder.build().map_err(|error| {
            tracing::error!(%error, "failed to build screened provider client");
            ScreenError::Client
        })
    }

    /// Resolve `host:port` and reject unless every answer is permitted.
    ///
    /// # Errors
    /// Returns [`ScreenError::Unresolved`] when the name does not resolve and
    /// [`ScreenError::Blocked`] when any answer is blocked.
    async fn resolve_and_screen(
        &self,
        host: &str,
        port: u16,
    ) -> Result<Vec<SocketAddr>, ScreenError> {
        let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
            .await
            .map_err(|error| {
                tracing::warn!(%host, %error, "provider host resolution failed");
                ScreenError::Unresolved
            })?
            .collect();

        if addrs.is_empty() || addrs.iter().any(|addr| self.is_blocked(addr.ip())) {
            return Err(ScreenError::Blocked);
        }
        Ok(addrs)
    }

    /// Whether this deployment must refuse to connect to `ip`.
    fn is_blocked(&self, ip: IpAddr) -> bool {
        is_always_blocked(ip) || (self.policy == AddressPolicy::BlockInternal && is_internal(ip))
    }
}

/// Collapse an IPv4-mapped IPv6 address (`::ffff:a.b.c.d`) to its IPv4 form.
///
/// One classifier then catches a range however it was written. Bare `::1`/`::`
/// stay IPv6 and are handled by the IPv6 arms.
fn normalize_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    }
}

/// `100.64.0.0/10` — RFC 6598 carrier-grade NAT shared address space.
fn is_cgnat(v4: Ipv4Addr) -> bool {
    let [a, b, ..] = v4.octets();
    a == 100 && (b & 0xc0) == 0x40
}

/// `fc00::/7` — RFC 4193 unique local addresses.
fn is_unique_local_v6(v6: Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xfe00) == 0xfc00
}

/// Addresses that are never a legitimate provider, in every deployment.
///
/// Link-local (`169.254.0.0/16`) covers the cloud instance metadata endpoint
/// (`169.254.169.254`); even a self-hosted deployment must never let a provider
/// URL reach it.
fn is_always_blocked(ip: IpAddr) -> bool {
    match normalize_ip(ip) {
        IpAddr::V4(v4) => v4.is_link_local() || v4.is_broadcast() || v4.is_documentation(),
        // fe80::/10 link-local.
        IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) == 0xfe80,
    }
}

/// Internal ranges a multi-tenant deployment must not let a provider URL reach.
fn is_internal(ip: IpAddr) -> bool {
    match normalize_ip(ip) {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_unspecified() || is_cgnat(v4)
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified() || is_unique_local_v6(v6),
    }
}

#[cfg(test)]
mod tests {
    use super::{AddressPolicy, ScreenError, ScreenedHttp};
    use std::net::IpAddr;
    use url::Url;

    /// The internal ranges separate the two policies, and only those ranges do.
    #[test]
    fn the_policy_decides_only_the_internal_ranges() {
        let blocking = ScreenedHttp::new(AddressPolicy::BlockInternal);
        let allowing = ScreenedHttp::allowing_internal();

        for raw in [
            "127.0.0.1",   // loopback
            "10.0.0.5",    // private
            "192.168.1.1", // private
            "172.16.0.1",  // private
            "100.64.0.1",  // CGNAT
            "::1",         // v6 loopback
            "fc00::1",     // v6 ULA
        ] {
            let ip: IpAddr = raw.parse().expect("valid ip");
            assert!(blocking.is_blocked(ip), "{raw} is internal");
            assert!(
                !allowing.is_blocked(ip),
                "{raw} is a legitimate local provider"
            );
        }

        for raw in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"] {
            let ip: IpAddr = raw.parse().expect("valid ip");
            assert!(!blocking.is_blocked(ip), "{raw} is public");
            assert!(!allowing.is_blocked(ip), "{raw} is public");
        }
    }

    /// An IPv4-mapped IPv6 spelling of the metadata address is caught too.
    #[test]
    fn a_mapped_spelling_does_not_evade_the_screen() {
        let mapped: IpAddr = "::ffff:169.254.169.254".parse().expect("valid ip");
        assert!(ScreenedHttp::allowing_internal().is_blocked(mapped));
    }

    /// The metadata endpoint is unreachable however permissive the deployment.
    #[tokio::test]
    async fn cloud_metadata_is_blocked_in_every_profile() {
        for policy in [AddressPolicy::AllowInternal, AddressPolicy::BlockInternal] {
            let error = ScreenedHttp::new(policy)
                .client_for(&Url::parse("http://169.254.169.254/latest/meta-data").expect("url"))
                .await
                .expect_err("metadata is refused");
            assert!(matches!(error, ScreenError::Blocked), "{policy:?}");
        }
    }

    /// Loopback is the developer's provider and the multi-tenant attack.
    #[tokio::test]
    async fn loopback_depends_on_the_deployment_policy() {
        let url = Url::parse("http://127.0.0.1:8080/realms/wyrd").expect("url");

        assert!(
            ScreenedHttp::allowing_internal()
                .client_for(&url)
                .await
                .is_ok()
        );
        assert!(matches!(
            ScreenedHttp::new(AddressPolicy::BlockInternal)
                .client_for(&url)
                .await
                .expect_err("refused"),
            ScreenError::Blocked
        ));
    }

    /// A URL with no host cannot be screened, so it is refused.
    #[tokio::test]
    async fn a_hostless_url_is_refused() {
        let error = ScreenedHttp::allowing_internal()
            .client_for(&Url::parse("file:///etc/passwd").expect("url"))
            .await
            .expect_err("refused");
        assert!(matches!(error, ScreenError::Blocked));
    }
}
