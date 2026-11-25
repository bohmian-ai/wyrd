//! The trusted source of a request, for per-source admission windows.
//!
//! A request's source is its TCP peer, unless that peer is a configured
//! trusted ingress: then it is the client address the ingress supplies in the
//! configured header, as Cloudflare's `CF-Connecting-IP` restoration does. A
//! forwarding header from any other peer is ignored. IPv4-mapped IPv6
//! addresses are normalized to IPv4. Password, signup and mail admission all
//! key their source windows by this one derivation.

use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderName};
use ipnet::IpNet;

use crate::state::AppState;

/// The deployment's trusted ingress proxies and the header they name the
/// client address in. The default trusts none.
#[derive(Debug, Clone, Default)]
pub struct TrustedIngress {
    /// Peers whose header is believed.
    proxies: Vec<IpNet>,
    /// The client-address header; `None` exactly when `proxies` is empty.
    header: Option<HeaderName>,
}

impl TrustedIngress {
    /// Trust `header` from peers inside `proxies`.
    #[must_use]
    pub fn new(proxies: Vec<IpNet>, header: HeaderName) -> Self {
        Self {
            proxies,
            header: Some(header),
        }
    }

    /// The source of a request from `peer` carrying `headers`.
    ///
    /// Returns `None` when no source can be determined: no peer was recorded,
    /// or a trusted ingress sent no single valid client address. Callers that
    /// admit by source refuse such a request rather than skip its window.
    #[must_use]
    pub fn source(&self, peer: Option<IpAddr>, headers: &HeaderMap) -> Option<IpAddr> {
        let peer = peer?.to_canonical();
        let Some(header) = &self.header else {
            return Some(peer);
        };
        if !self.proxies.iter().any(|proxy| proxy.contains(&peer)) {
            return Some(peer);
        }
        let mut values = headers.get_all(header).iter();
        match (values.next(), values.next()) {
            (Some(value), None) => value
                .to_str()
                .ok()
                .and_then(|value| value.trim().parse::<IpAddr>().ok())
                .map(|client| client.to_canonical()),
            _ => None,
        }
    }
}

/// The trusted source of the current request; `None` when it cannot be
/// determined.
#[derive(Debug, Clone, Copy)]
pub struct RequestSource(
    /// The derived source address.
    pub Option<IpAddr>,
);

impl FromRequestParts<AppState> for RequestSource {
    /// Derivation never rejects; an unknown source is `None`.
    type Rejection = std::convert::Infallible;

    /// Derive the source from the recorded peer and the headers.
    ///
    /// # Errors
    /// Never fails.
    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip());
        let source = state.auth.trusted_ingress.source(peer, &parts.headers);
        std::future::ready(Ok(Self(source)))
    }
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use axum::http::{HeaderMap, HeaderName, HeaderValue};

    use super::TrustedIngress;

    fn ip(value: &str) -> IpAddr {
        value.parse().expect("valid address")
    }

    fn headers(values: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(
                "cf-connecting-ip",
                HeaderValue::from_str(value).expect("header value"),
            );
        }
        headers
    }

    /// Only a trusted peer's single valid header names the source; mapped
    /// IPv4 normalizes; anything else from a trusted peer is no source.
    #[test]
    fn only_a_trusted_peer_names_the_client() {
        let ingress = TrustedIngress::new(
            vec!["10.0.0.0/8".parse().expect("cidr")],
            HeaderName::from_static("cf-connecting-ip"),
        );
        let trusted = Some(ip("10.1.2.3"));
        assert_eq!(
            ingress.source(trusted, &headers(&["198.51.100.7"])),
            Some(ip("198.51.100.7"))
        );
        assert_eq!(
            ingress.source(trusted, &headers(&["::ffff:198.51.100.7"])),
            Some(ip("198.51.100.7"))
        );
        assert_eq!(
            ingress.source(Some(ip("203.0.113.9")), &headers(&["198.51.100.7"])),
            Some(ip("203.0.113.9")),
            "an untrusted peer's header is ignored"
        );
        assert_eq!(ingress.source(trusted, &headers(&[])), None);
        assert_eq!(ingress.source(trusted, &headers(&["not-an-ip"])), None);
        assert_eq!(
            ingress.source(trusted, &headers(&["1.1.1.1", "2.2.2.2"])),
            None
        );
        assert_eq!(ingress.source(None, &headers(&["198.51.100.7"])), None);
        assert_eq!(
            TrustedIngress::default().source(Some(ip("::ffff:10.1.2.3")), &headers(&["1.1.1.1"])),
            Some(ip("10.1.2.3"))
        );
    }
}
