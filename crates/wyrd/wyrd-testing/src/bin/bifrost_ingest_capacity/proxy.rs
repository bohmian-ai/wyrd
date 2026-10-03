//! A loopback TCP proxy that delays every server→client byte by a fixed
//! interval, standing in for a server whose acknowledgements arrive late.
//!
//! Client→server bytes pass straight through, so batches reach the server
//! on time and only their acknowledgements (and every other response frame)
//! are held back, in order.

use std::net::SocketAddr;
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::Result;

/// A running delay proxy; dropping it stops accepting connections.
pub struct DelayProxy {
    /// The loopback address clients dial.
    addr: SocketAddr,
    /// The accept loop.
    accept: JoinHandle<()>,
}

impl DelayProxy {
    /// Listens on an ephemeral loopback port and relays each connection to
    /// `target`, delaying the server's bytes by `delay`.
    ///
    /// # Errors
    ///
    /// Returns the bind failure.
    pub async fn start(target: SocketAddr, delay: Duration) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let accept = tokio::spawn(async move {
            while let Ok((inbound, _)) = listener.accept().await {
                tokio::spawn(Self::relay(inbound, target, delay));
            }
        });
        Ok(Self { addr, accept })
    }

    /// The `http://` endpoint a gRPC client dials.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Relays one connection until both directions reach end of stream.
    async fn relay(inbound: TcpStream, target: SocketAddr, delay: Duration) {
        let Ok(outbound) = TcpStream::connect(target).await else {
            return;
        };
        let (mut client_read, client_write) = inbound.into_split();
        let (server_read, mut server_write) = outbound.into_split();
        let upstream = async move {
            let _ = tokio::io::copy(&mut client_read, &mut server_write).await;
            let _ = server_write.shutdown().await;
        };
        tokio::join!(upstream, Self::delayed(server_read, client_write, delay));
    }

    /// Copies `from` to `to`, writing each chunk `delay` after it was read.
    ///
    /// Reading never waits on the delayed writer, so the delay adds latency
    /// without throttling the stream.
    async fn delayed(mut from: OwnedReadHalf, mut to: OwnedWriteHalf, delay: Duration) {
        let (sender, mut receiver) = mpsc::unbounded_channel::<(Instant, Bytes)>();
        let reader = async move {
            loop {
                let mut chunk = BytesMut::new();
                match from.read_buf(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if sender
                            .send((Instant::now() + delay, chunk.freeze()))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        };
        let writer = async move {
            while let Some((due, chunk)) = receiver.recv().await {
                tokio::time::sleep_until(due).await;
                if to.write_all(&chunk).await.is_err() {
                    break;
                }
            }
            let _ = to.shutdown().await;
        };
        tokio::join!(reader, writer);
    }
}

impl Drop for DelayProxy {
    /// Stops accepting; relayed connections close when their peers do.
    fn drop(&mut self) {
        self.accept.abort();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::TcpListener;
    use tokio::time::Instant;

    use super::DelayProxy;

    /// Proves request bytes pass through and the echoed response arrives no
    /// sooner than the delay.
    ///
    /// # Panics
    ///
    /// Panics when the echo is wrong or early.
    #[tokio::test]
    async fn delays_server_bytes_only() {
        let echo = TcpListener::bind("127.0.0.1:0").await.expect("bind echo");
        let target = echo.local_addr().expect("echo addr");
        tokio::spawn(async move {
            let (mut socket, _) = echo.accept().await.expect("accept");
            let mut buffer = [0_u8; 4];
            socket.read_exact(&mut buffer).await.expect("read");
            socket.write_all(&buffer).await.expect("write");
        });
        let proxy = DelayProxy::start(target, Duration::from_millis(50))
            .await
            .expect("proxy");
        let mut client = tokio::net::TcpStream::connect(proxy.addr)
            .await
            .expect("connect");
        let sent = Instant::now();
        client.write_all(b"ping").await.expect("send");
        let mut answer = [0_u8; 4];
        client.read_exact(&mut answer).await.expect("answer");
        assert_eq!(&answer, b"ping");
        assert!(sent.elapsed() >= Duration::from_millis(50));
    }
}
