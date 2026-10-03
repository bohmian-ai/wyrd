//! The local OpenAI-shaped judge provider every LLM-judge Eval calls.
//!
//! It answers each chat completion after [`JUDGE_DELAY`] with a passing
//! grade, so the deterministic assertion beside the judge decides a verdict.
//! It serves over TLS from a leaf of a CA the benchmark writes to disk; the
//! release server trusts that CA through `SSL_CERT_FILE`, so certificate
//! validation and the provider's `https`-only base URL stay enforced. The
//! mock measures Wyrd overhead and concurrency, not real-provider capacity.
//!
//! The judge is also the owner of provider-wait evidence (AC-040): it records
//! how long it held every answered completion, so a step reports that wait
//! apart from the server's judge engine overhead.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use axum::Router;
use axum::routing::post;
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::server::TlsStream;
use tokio_util::sync::CancellationToken;
use wyrd_testing::bifrost::peer_ca::BifrostPeerCa;

use crate::Result;

/// How long every judge call waits before answering (AC-040).
pub const JUDGE_DELAY: Duration = Duration::from_millis(200);

/// Host name the judge's certificate names and its base URL dials.
const HOST: &str = "localhost";

/// The running judge provider.
pub struct Judge {
    /// `https://` OpenAI base URL ending in `/v1`.
    base_url: String,
    /// PEM file holding the CA the server must trust.
    ca_file: PathBuf,
    /// How long each answered completion waited before its answer,
    /// microseconds, in answer order.
    waits: Arc<Mutex<Vec<u64>>>,
    /// Stops the listener.
    stop: CancellationToken,
}

impl Judge {
    /// Mints a CA and leaf for [`HOST`], writes the CA under `directory`, and
    /// serves the delayed completion route on a loopback port.
    ///
    /// # Errors
    ///
    /// Returns a certificate, file, TLS configuration, or bind failure.
    ///
    /// # Cancellation
    ///
    /// Before it returns nothing is served; the CA file may remain under
    /// `directory`, which its owner removes.
    pub async fn start(directory: &Path) -> Result<Self> {
        let authority = BifrostPeerCa::generate(HOST)?;
        let leaf = authority.issue_leaf("capacity-judge")?;
        std::fs::create_dir_all(directory)?;
        let ca_file = directory.join("judge-ca.pem");
        std::fs::write(&ca_file, authority.ca_certificate_pem())?;
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(
            CertificateDer::pem_slice_iter(leaf.certificate_pem().as_bytes())
                .collect::<std::result::Result<Vec<_>, _>>()?,
            PrivateKeyDer::from_pem_slice(leaf.private_key_pem().as_bytes())?,
        )?;
        let tcp = TcpListener::bind("127.0.0.1:0").await?;
        let base_url = format!("https://{HOST}:{}/v1", tcp.local_addr()?.port());
        let waits = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&waits);
        let router = Router::new().route(
            "/v1/chat/completions",
            post(move || {
                let waits = Arc::clone(&recorded);
                async move {
                    let received = tokio::time::Instant::now();
                    tokio::time::sleep(JUDGE_DELAY).await;
                    let waited = u64::try_from(received.elapsed().as_micros()).unwrap_or(u64::MAX);
                    waits
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(waited);
                    axum::Json(serde_json::json!({
                        "id": "chatcmpl_capacity", "object": "chat.completion",
                        "created": 1_700_000_000, "model": "gpt-test",
                        "choices": [{ "index": 0, "finish_reason": "stop",
                            "message": { "role": "assistant", "content": "{\"passed\":true}" } }],
                        "usage": { "prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8 }
                    }))
                }
            }),
        );
        let listener = TlsListener {
            tcp,
            acceptor: TlsAcceptor::from(Arc::new(config)),
        };
        let stop = CancellationToken::new();
        let stopping = stop.clone();
        tokio::spawn(async move {
            let served = axum::serve(listener, router)
                .with_graceful_shutdown(stopping.cancelled_owned())
                .await;
            if let Err(error) = served {
                eprintln!("judge provider stopped: {error}");
            }
        });
        Ok(Self {
            base_url,
            ca_file,
            waits,
            stop,
        })
    }

    /// The base URL the server's `OPENAI_BASE_URL` names.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The CA file the server's `SSL_CERT_FILE` names.
    pub fn ca_file(&self) -> &Path {
        &self.ca_file
    }

    /// Completions answered so far; a step marks its start with it.
    pub fn calls(&self) -> usize {
        self.waits
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// The provider wait of every completion answered since `mark`, a
    /// [`Judge::calls`] reading, microseconds.
    pub fn waits_since(&self, mark: usize) -> Vec<u64> {
        self.waits
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(mark..)
            .map(<[u64]>::to_vec)
            .unwrap_or_default()
    }
}

impl Drop for Judge {
    /// Stops serving.
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

/// A TCP listener completing a TLS handshake on every accepted connection.
struct TlsListener {
    /// The bound loopback socket.
    tcp: TcpListener,
    /// Server TLS configuration.
    acceptor: TlsAcceptor,
}

impl axum::serve::Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = std::net::SocketAddr;

    /// Accepts the next connection whose handshake succeeds; a failed accept
    /// or handshake drops that connection and keeps listening.
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let Ok((stream, address)) = self.tcp.accept().await else {
                continue;
            };
            if let Ok(tls) = self.acceptor.accept(stream).await {
                return (tls, address);
            }
        }
    }

    /// The bound loopback address.
    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.tcp.local_addr()
    }
}
