//! A loopback Postgres proxy that silently stalls notification listeners.
//!
//! A cache listener whose network path stops delivering bytes, without either
//! socket closing, looks healthy to the client forever. Closing a socket or
//! terminating a backend does not reproduce that: both report a loss. This
//! proxy relays every connection to the real server and remembers which
//! channels each one has subscribed to with `LISTEN "<channel>"`. Stalling a
//! channel stops relaying data in both directions on every connection
//! subscribed to it, now or later, while both sockets stay open; ordinary
//! request connections keep flowing. A stalled connection still notices when
//! its client closes, so a test can count the stalled connections a client
//! still retains, and still forwards a server-side close, so a terminated
//! backend is reported to the client as a loss.
//!
//! The proxy reads plaintext protocol bytes, so [`ListenerStallProxy::route`]
//! also disables TLS on the routed DSNs.

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use secrecy::{ExposeSecret as _, SecretString};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use wyrd_sql::dsn::ResolvedDsns;

/// The statement prefix a subscription sends, as SQLx quotes it.
const LISTEN: &[u8] = b"LISTEN \"";

/// Bytes of the previous client chunk kept so a `LISTEN` split across reads
/// is still recognized.
const SCAN_TAIL: usize = 256;

/// Errors starting or routing through the proxy.
#[derive(Debug, thiserror::Error)]
pub enum ListenerStallError {
    /// The proxy could not bind or resolve its upstream address.
    #[error("listener stall proxy I/O: {0}")]
    Io(#[from] std::io::Error),
    /// A DSN does not parse or names no host and port.
    #[error("listener stall proxy DSN: {0}")]
    Dsn(String),
}

/// One relayed connection's subscription and stall state.
#[derive(Default)]
struct Link {
    /// Channels this connection has sent `LISTEN` for.
    channels: Mutex<BTreeSet<String>>,
    /// Set once the connection is stalled; never cleared, because a stalled
    /// connection stands for an abandoned network path.
    stalled: AtomicBool,
    /// Cleared when the client closes its side of the connection.
    client_open: AtomicBool,
}

impl Link {
    /// The channels this connection subscribed to.
    fn channels(&self) -> MutexGuard<'_, BTreeSet<String>> {
        self.channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// State shared by the accept loop, every relay, and the test.
#[derive(Default)]
struct Shared {
    /// Channels currently stalled.
    stalled: Mutex<BTreeSet<String>>,
    /// Every connection accepted so far.
    links: Mutex<Vec<Arc<Link>>>,
}

impl Shared {
    /// The currently stalled channels.
    fn stalled(&self) -> MutexGuard<'_, BTreeSet<String>> {
        self.stalled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Every connection accepted so far.
    fn links(&self) -> MutexGuard<'_, Vec<Arc<Link>>> {
        self.links
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Records the channels named by `LISTEN` statements in `scan` on
    /// `link`, stalling the link when any of them is stalled.
    fn observe(&self, link: &Link, scan: &[u8]) {
        let mut found = Vec::new();
        let mut rest = scan;
        while let Some(at) = rest.windows(LISTEN.len()).position(|w| w == LISTEN) {
            let unlisten = at > 0 && rest[at - 1] == b'N';
            rest = &rest[at + LISTEN.len()..];
            if let Some(end) = rest.iter().position(|&byte| byte == b'"') {
                if !unlisten {
                    found.push(String::from_utf8_lossy(&rest[..end]).into_owned());
                }
                rest = &rest[end..];
            }
        }
        if found.is_empty() {
            return;
        }
        let stalled = self.stalled();
        if found.iter().any(|channel| stalled.contains(channel)) {
            link.stalled.store(true, Ordering::SeqCst);
        }
        link.channels().extend(found);
    }
}

/// A running proxy; dropping it stops accepting connections and aborts
/// every relay, closing their sockets.
pub struct ListenerStallProxy {
    /// The loopback address routed DSNs dial.
    addr: SocketAddr,
    /// Channel and connection state.
    shared: Arc<Shared>,
    /// The accept loop; it owns every relay task.
    accept: JoinHandle<()>,
}

impl ListenerStallProxy {
    /// Listens on an ephemeral loopback port and relays each connection to
    /// the host and port of `dsns.app`.
    ///
    /// # Errors
    /// Returns [`ListenerStallError`] when the DSN names no address or the
    /// proxy cannot bind.
    pub async fn start(dsns: &ResolvedDsns) -> Result<Self, ListenerStallError> {
        let app = parse(&dsns.app)?;
        let host = app
            .host_str()
            .ok_or_else(|| ListenerStallError::Dsn("the DSN names no host".to_owned()))?;
        let port = app.port().unwrap_or(5432);
        let target = tokio::net::lookup_host((host, port))
            .await?
            .next()
            .ok_or_else(|| ListenerStallError::Dsn(format!("{host} does not resolve")))?;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared::default());
        let accept = tokio::spawn(Self::accept(listener, target, Arc::clone(&shared)));
        Ok(Self {
            addr,
            shared,
            accept,
        })
    }

    /// Rewrites every DSN in `dsns` to dial this proxy with TLS disabled.
    ///
    /// # Errors
    /// Returns [`ListenerStallError::Dsn`] when a DSN does not parse.
    pub fn route(&self, dsns: &ResolvedDsns) -> Result<ResolvedDsns, ListenerStallError> {
        let route = |dsn: &SecretString| -> Result<SecretString, ListenerStallError> {
            let mut url = parse(dsn)?;
            url.set_ip_host(self.addr.ip())
                .map_err(|()| ListenerStallError::Dsn("the DSN cannot take a host".to_owned()))?;
            url.set_port(Some(self.addr.port()))
                .map_err(|()| ListenerStallError::Dsn("the DSN cannot take a port".to_owned()))?;
            let pairs = url
                .query_pairs()
                .filter(|(key, _)| key != "sslmode")
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect::<Vec<_>>();
            url.query_pairs_mut()
                .clear()
                .extend_pairs(pairs)
                .append_pair("sslmode", "disable");
            Ok(SecretString::from(url.to_string()))
        };
        Ok(ResolvedDsns {
            app: route(&dsns.app)?,
            platform_admin: dsns.platform_admin.as_ref().map(route).transpose()?,
        })
    }

    /// Stalls `channel`: every connection subscribed to it now or later
    /// stops relaying data while its sockets stay open. Returns how many
    /// already-subscribed connections were stalled.
    pub fn stall(&self, channel: &str) -> usize {
        self.shared.stalled().insert(channel.to_owned());
        self.shared
            .links()
            .iter()
            .filter(|link| link.channels().contains(channel))
            .filter(|link| !link.stalled.swap(true, Ordering::SeqCst))
            .count()
    }

    /// Lets new subscriptions to `channel` through again. Connections
    /// already stalled stay stalled, like an abandoned network path.
    pub fn restore(&self, channel: &str) {
        self.shared.stalled().remove(channel);
    }

    /// Connections subscribed to `channel` that relay normally and whose
    /// client is still connected.
    #[must_use]
    pub fn listening(&self, channel: &str) -> usize {
        self.count(channel, false)
    }

    /// Stalled connections subscribed to `channel` whose client still holds
    /// its socket open.
    #[must_use]
    pub fn stalled_open(&self, channel: &str) -> usize {
        self.count(channel, true)
    }

    /// Every connection that ever subscribed to `channel`, open or closed.
    #[must_use]
    pub fn subscriptions(&self, channel: &str) -> usize {
        self.shared
            .links()
            .iter()
            .filter(|link| link.channels().contains(channel))
            .count()
    }

    /// Counts open connections subscribed to `channel` with the given stall
    /// state.
    fn count(&self, channel: &str, stalled: bool) -> usize {
        self.shared
            .links()
            .iter()
            .filter(|link| link.client_open.load(Ordering::SeqCst))
            .filter(|link| link.stalled.load(Ordering::SeqCst) == stalled)
            .filter(|link| link.channels().contains(channel))
            .count()
    }

    /// Accepts connections and relays each until the proxy drops.
    async fn accept(listener: TcpListener, target: SocketAddr, shared: Arc<Shared>) {
        let mut relays = tokio::task::JoinSet::new();
        while let Ok((inbound, _)) = listener.accept().await {
            let link = Arc::new(Link::default());
            link.client_open.store(true, Ordering::SeqCst);
            shared.links().push(Arc::clone(&link));
            relays.spawn(Self::relay(inbound, target, Arc::clone(&shared), link));
            while relays.try_join_next().is_some() {}
        }
    }

    /// Relays one connection until its client closes.
    async fn relay(inbound: TcpStream, target: SocketAddr, shared: Arc<Shared>, link: Arc<Link>) {
        let Ok(outbound) = TcpStream::connect(target).await else {
            link.client_open.store(false, Ordering::SeqCst);
            return;
        };
        let (client_read, client_write) = inbound.into_split();
        let (server_read, server_write) = outbound.into_split();
        let downstream = tokio::spawn(Self::downstream(server_read, client_write, Arc::clone(&link)));
        Self::upstream(client_read, server_write, &shared, &link).await;
        link.client_open.store(false, Ordering::SeqCst);
        downstream.abort();
    }

    /// Copies client bytes to the server, recording subscriptions and
    /// discarding bytes while stalled, until the client closes.
    async fn upstream(
        mut client: OwnedReadHalf,
        mut server: OwnedWriteHalf,
        shared: &Shared,
        link: &Link,
    ) {
        let mut scan = Vec::new();
        let mut chunk = vec![0; 16 * 1024];
        while let Ok(read) = client.read(&mut chunk).await {
            if read == 0 {
                break;
            }
            scan.extend_from_slice(&chunk[..read]);
            shared.observe(link, &scan);
            scan.drain(..scan.len().saturating_sub(SCAN_TAIL));
            if !link.stalled.load(Ordering::SeqCst)
                && server.write_all(&chunk[..read]).await.is_err()
            {
                break;
            }
        }
    }

    /// Copies server bytes to the client, discarding them while stalled,
    /// and closes the client's side when the server closes.
    async fn downstream(mut server: OwnedReadHalf, mut client: OwnedWriteHalf, link: Arc<Link>) {
        let mut chunk = vec![0; 16 * 1024];
        while let Ok(read) = server.read(&mut chunk).await {
            if read == 0 {
                break;
            }
            if !link.stalled.load(Ordering::SeqCst)
                && client.write_all(&chunk[..read]).await.is_err()
            {
                return;
            }
        }
        let _ = client.shutdown().await;
    }
}

impl Drop for ListenerStallProxy {
    /// Stops accepting and aborts every relay, closing their sockets.
    fn drop(&mut self) {
        self.accept.abort();
    }
}

/// Parses one DSN.
///
/// # Errors
/// Returns [`ListenerStallError::Dsn`] when it is not a URL.
fn parse(dsn: &SecretString) -> Result<url::Url, ListenerStallError> {
    url::Url::parse(dsn.expose_secret())
        .map_err(|error| ListenerStallError::Dsn(format!("unparseable DSN: {error}")))
}
