//! Configure mTLS secret references for a Wyrd transport.
//!
//! Authentication is owned by `AuthMiddleware` (see `client_request`), not the
//! transport config; this example covers the TLS identity material only.
//!
//! Run with:
//!     cargo run -p wyrd-rust-examples --bin transport_secrets

use wyrd_client::transport::{GrpcConfig, TransportConfig};
use wyrd_spec::security::{SecretRef, TlsConfig};

fn main() -> anyhow::Result<()> {
    let grpc = GrpcConfig {
        endpoint: "https://wyrd-ingest.internal:50051".to_string(),
        tls: Some(TlsConfig {
            ca_cert: Some(SecretRef::File {
                path: "/var/run/secrets/wyrd/ca.pem".to_string(),
            }),
            client_cert: Some(SecretRef::File {
                path: "/var/run/secrets/wyrd/client.crt".to_string(),
            }),
            client_key: Some(SecretRef::File {
                path: "/var/run/secrets/wyrd/client.key".to_string(),
            }),
            server_name_override: Some("ingest.internal".to_string()),
            insecure_skip_verify: false,
        }),
        ..GrpcConfig::default()
    };
    grpc.validate()?;

    let transport = TransportConfig::Grpc(grpc);
    transport.validate()?;

    let json = serde_json::to_string_pretty(&transport)?;
    let round_trip: TransportConfig = serde_json::from_str(&json)?;
    anyhow::ensure!(
        round_trip == transport,
        "transport config did not round-trip"
    );
    println!("{json}");
    Ok(())
}
