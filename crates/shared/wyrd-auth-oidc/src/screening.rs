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

use reqwest::{Client, Response};
use url::Url;

/// How long any provider fetch may take.
///
/// Bounded because these requests sit in a login path: a provider that hangs
/// must fail the login, not hold the connection.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Largest decoded provider response body Wyrd buffers: 1 MiB.
///
/// Discovery documents, key sets, and token responses are a few KiB. The cap is
/// counted after transfer and content decoding, so neither a chunked stream nor
/// a small compressed body that inflates can grow a serving replica's memory
/// past it.
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

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
    /// The URL uses a scheme the policy refuses, has no host, or resolves to
    /// an address Wyrd must never reach.
    #[error("provider URL uses a refused scheme or resolves to a blocked address range")]
    Blocked,
    /// The host could not be resolved at all.
    #[error("issuer host could not be resolved")]
    Unresolved,
    /// The HTTP client could not be constructed.
    #[error("provider client could not be constructed")]
    Client,
}

/// Why a provider response body could not be read.
#[derive(Debug, thiserror::Error)]
pub enum BodyError {
    /// The declared or decoded body exceeds [`MAX_RESPONSE_BYTES`].
    #[error("provider response exceeds the {MAX_RESPONSE_BYTES}-byte limit")]
    TooLarge,
    /// The transfer failed, timed out, or could not be decoded.
    #[error("provider response could not be read: {0}")]
    Read(#[from] reqwest::Error),
}

/// Read a provider response body, refusing it once it exceeds
/// [`MAX_RESPONSE_BYTES`].
///
/// A declared `Content-Length` above the cap is refused before any body byte
/// is read. Otherwise the decoded body is read chunk by chunk through reqwest,
/// and reading stops at the first chunk that would cross the cap, so a
/// chunked or decompression-amplified body is never buffered whole. The
/// request's own timeout keeps bounding the read; dropping the future cancels
/// it with nothing retained.
///
/// # Errors
/// Returns [`BodyError::TooLarge`] when the declared or decoded body exceeds
/// the cap, and [`BodyError::Read`] when the transfer or content decoding
/// fails or times out.
pub async fn read_bounded_body(mut response: Response) -> Result<Vec<u8>, BodyError> {
    let declared_too_large = response
        .content_length()
        .is_some_and(|len| !usize::try_from(len).is_ok_and(|len| len <= MAX_RESPONSE_BYTES));
    if declared_too_large {
        return Err(BodyError::TooLarge);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(BodyError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
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
    /// The scheme is screened first, before any resolution or request:
    /// [`AddressPolicy::BlockInternal`] accepts only `https`, so no discovered
    /// endpoint can carry a client secret or authorization code in cleartext;
    /// [`AddressPolicy::AllowInternal`] also accepts `http` for local and test
    /// providers. Every other scheme is refused under both policies.
    ///
    /// A literal-IP host is screened directly. A name is resolved, every
    /// resolved address is screened, and the returned client is pinned to those
    /// addresses — so the answer that passed the screen is the answer the
    /// connection uses, and a rebinding reply between the two cannot redirect it
    /// inward. Redirects are refused outright, because following one would leave
    /// the screen behind, and system proxies are ignored for the same reason.
    ///
    /// Rejecting on *any* blocked record rather than filtering them out is
    /// deliberate: split-horizon DNS that mixes one public and one internal
    /// answer is the attack, not a mixed-quality result to be salvaged.
    ///
    /// # Errors
    /// Returns [`ScreenError::Blocked`] when the scheme is refused, the URL has
    /// no host, or any resolved address is blocked, [`ScreenError::Unresolved`] when resolution
    /// fails, and [`ScreenError::Client`] when TLS setup or client construction
    /// fails.
    pub async fn client_for(&self, url: &Url) -> Result<Client, ScreenError> {
        if !self.permits_scheme(url.scheme()) {
            return Err(ScreenError::Blocked);
        }
        wyrd_tls::install_crypto_provider().map_err(|_| ScreenError::Client)?;
        // No proxy, ambient or otherwise: a proxy would receive the original
        // hostname and choose its own destination, bypassing the pinned,
        // screened addresses below.
        let builder = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy();

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

    /// Whether this deployment may send a provider request over `scheme`.
    fn permits_scheme(&self, scheme: &str) -> bool {
        scheme == "https" || (self.policy == AddressPolicy::AllowInternal && scheme == "http")
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

/// Address-policy screening for outbound OIDC calls.
#[cfg(test)]
mod tests {
    use super::{
        AddressPolicy, BodyError, MAX_RESPONSE_BYTES, ScreenError, ScreenedHttp, read_bounded_body,
    };
    use std::net::IpAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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

    /// Environment variable naming the pinned target the proxy probe child
    /// requests; absent in every ordinary run.
    const PROXY_PROBE_TARGET: &str = "WYRD_SCREENED_PROXY_PROBE_TARGET";

    /// An ambient HTTP proxy neither observes nor reroutes a screened request.
    ///
    /// The workspace denies `unsafe`, so the proxy environment cannot be set
    /// in-process; the test re-runs this binary's
    /// [`screened_request_child`] with `HTTP_PROXY` pointing at a recording
    /// server and asserts the request reached the pinned target instead.
    ///
    /// # Panics
    /// Panics when the child fails, the target misses the request, or the
    /// proxy sees any request.
    #[tokio::test]
    async fn an_ambient_proxy_cannot_observe_a_screened_request() {
        let proxy = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&proxy)
            .await;
        let target = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pinned"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&target)
            .await;
        let target_url = format!("http://localhost:{}/pinned", target.address().port());
        let proxy_url = proxy.uri();

        let status = tokio::task::spawn_blocking(move || {
            std::process::Command::new(std::env::current_exe().expect("test binary path"))
                .args(["--exact", "screening::tests::screened_request_child"])
                .env(PROXY_PROBE_TARGET, target_url)
                .env("HTTP_PROXY", &proxy_url)
                .env("http_proxy", &proxy_url)
                .env("ALL_PROXY", &proxy_url)
                .env_remove("NO_PROXY")
                .env_remove("no_proxy")
                .status()
                .expect("child test runs")
        })
        .await
        .expect("child join");

        assert!(status.success(), "the screened request succeeds: {status}");
        let proxied = proxy.received_requests().await.expect("proxy records");
        assert!(
            proxied.is_empty(),
            "the proxy saw {} requests",
            proxied.len()
        );
        let reached = target.received_requests().await.expect("target records");
        assert_eq!(reached.len(), 1, "the pinned target receives the request");
    }

    /// Child half of [`an_ambient_proxy_cannot_observe_a_screened_request`]:
    /// one screened GET under the parent's proxy environment. Without that
    /// environment it has nothing to probe and returns.
    ///
    /// # Panics
    /// Panics when the screened client cannot be built or the request fails.
    #[tokio::test]
    async fn screened_request_child() {
        let Ok(target) = std::env::var(PROXY_PROBE_TARGET) else {
            return;
        };
        let url = Url::parse(&target).expect("target parses");
        let response = ScreenedHttp::allowing_internal()
            .client_for(&url)
            .await
            .expect("screened client builds")
            .get(url)
            .send()
            .await
            .expect("screened request completes");
        assert!(response.status().is_success(), "{}", response.status());
    }

    /// Production screening refuses cleartext before resolving or sending;
    /// the permissive policy keeps its `http` local-provider path; no policy
    /// accepts another scheme.
    #[tokio::test]
    async fn only_the_permissive_policy_accepts_cleartext_http() {
        let target = MockServer::start().await;
        let blocking = ScreenedHttp::new(AddressPolicy::BlockInternal);
        let allowing = ScreenedHttp::allowing_internal();

        let cleartext = Url::parse("http://idp.example.com/token").expect("url");
        assert!(matches!(
            blocking.client_for(&cleartext).await.expect_err("refused"),
            ScreenError::Blocked
        ));

        let local = Url::parse(&target.uri()).expect("mock uri");
        allowing
            .client_for(&local)
            .await
            .expect("the local provider stays reachable")
            .get(local)
            .send()
            .await
            .expect("the local request is sent");

        let other = Url::parse("ftp://127.0.0.1/token").expect("url");
        for policy in [blocking, allowing] {
            assert!(matches!(
                policy.client_for(&other).await.expect_err("refused"),
                ScreenError::Blocked
            ));
        }
    }

    /// Fetch `url` through a permissive screened client and read it bounded.
    ///
    /// # Panics
    /// Panics when the client cannot be built or the request is not sent.
    async fn bounded_get(url: &str) -> Result<Vec<u8>, BodyError> {
        let url = Url::parse(url).expect("url");
        let response = ScreenedHttp::allowing_internal()
            .client_for(&url)
            .await
            .expect("screened client builds")
            .get(url)
            .send()
            .await
            .expect("request is sent");
        read_bounded_body(response).await
    }

    /// Serve `body` from a mock provider at `/body` with extra `headers`.
    async fn serve(body: Vec<u8>, headers: &[(&str, &str)]) -> MockServer {
        let server = MockServer::start().await;
        let mut response = ResponseTemplate::new(200).set_body_bytes(body);
        for (name, value) in headers {
            response = response.insert_header(*name, *value);
        }
        Mock::given(method("GET"))
            .and(path("/body"))
            .respond_with(response)
            .mount(&server)
            .await;
        server
    }

    /// A body of exactly the cap is returned whole.
    #[tokio::test]
    async fn a_body_at_the_limit_is_read_whole() {
        let server = serve(vec![b'a'; MAX_RESPONSE_BYTES], &[]).await;
        let body = bounded_get(&format!("{}/body", server.uri()))
            .await
            .expect("an at-limit body is read");
        assert_eq!(body.len(), MAX_RESPONSE_BYTES);
    }

    /// A declared length one byte over the cap is refused.
    #[tokio::test]
    async fn an_oversized_declared_body_is_refused() {
        let server = serve(vec![b'a'; MAX_RESPONSE_BYTES + 1], &[]).await;
        let error = bounded_get(&format!("{}/body", server.uri()))
            .await
            .expect_err("oversized body is refused");
        assert!(matches!(error, BodyError::TooLarge), "{error:?}");
    }

    /// A chunked body with no declared length stops at the cap.
    ///
    /// # Panics
    /// Panics when the raw server cannot bind or accept.
    #[tokio::test]
    async fn an_oversized_chunked_body_is_refused() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("local address");
        let chunk = 64 * 1024;
        let mut response = b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n".to_vec();
        for _ in 0..=MAX_RESPONSE_BYTES / chunk {
            response.extend_from_slice(format!("{chunk:x}\r\n").as_bytes());
            response.extend(std::iter::repeat_n(b'a', chunk));
            response.extend_from_slice(b"\r\n");
        }
        response.extend_from_slice(b"0\r\n\r\n");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = [0_u8; 4096];
            let _ = socket.read(&mut request).await;
            // The client may hang up mid-body once it refuses; that is the point.
            let _ = socket.write_all(&response).await;
        });

        let error = bounded_get(&format!("http://{address}/body"))
            .await
            .expect_err("oversized chunked body is refused");
        assert!(matches!(error, BodyError::TooLarge), "{error:?}");
        server.abort();
    }

    /// A gzip body of a few KiB that inflates past the cap is refused on its
    /// decoded size.
    #[tokio::test]
    async fn an_oversized_decompressed_body_is_refused() {
        let repeats = MAX_RESPONSE_BYTES / 258 + 1;
        let compressed = gzip_repeating_a(repeats);
        assert!(
            compressed.len() < MAX_RESPONSE_BYTES / 64,
            "the body is small on the wire"
        );
        let server = serve(compressed, &[("content-encoding", "gzip")]).await;
        let error = bounded_get(&format!("{}/body", server.uri()))
            .await
            .expect_err("inflated body is refused");
        assert!(matches!(error, BodyError::TooLarge), "{error:?}");
    }

    /// Gzip stream inflating to `1 + 258 * repeats` bytes of `a`.
    ///
    /// One fixed-Huffman deflate block: a literal `a`, then `repeats`
    /// length-258/distance-1 back-references (13 bits each). The trailer's
    /// CRC and size are zero because a bounded reader stops before them; the
    /// workspace ships no compression encoder to this crate.
    ///
    /// # Panics
    /// Never in practice; the byte conversion masks to eight bits.
    fn gzip_repeating_a(repeats: usize) -> Vec<u8> {
        let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff];
        let (mut bits, mut used) = (0_u32, 0_u32);
        let mut put = |out: &mut Vec<u8>, value: u32, len: u32| {
            bits |= value << used;
            used += len;
            while used >= 8 {
                out.push(u8::try_from(bits & 0xff).expect("masked to a byte"));
                bits >>= 8;
                used -= 8;
            }
        };
        // Huffman codes are packed most-significant bit first.
        let huffman = |code: u32, len: u32| code.reverse_bits() >> (32 - len);
        put(&mut out, 0b011, 3); // BFINAL = 1, BTYPE = 01 (fixed Huffman).
        put(&mut out, huffman(0x30 + u32::from(b'a'), 8), 8);
        for _ in 0..repeats {
            put(&mut out, huffman(0b1100_0101, 8), 8); // length code 285 = 258.
            put(&mut out, huffman(0, 5), 5); // distance code 0 = 1.
        }
        put(&mut out, huffman(0, 7), 7); // end of block.
        put(&mut out, 0, 7); // flush the final partial byte.
        out.extend([0; 8]);
        out
    }
}
