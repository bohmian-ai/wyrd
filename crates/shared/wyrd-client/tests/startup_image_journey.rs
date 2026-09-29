//! Public Rust SDK half of the `test:server:startup` image journey.
//!
//! `scripts/server/test-startup.sh` starts the official application image and
//! runs these two ignored tests against it through nginx: `write` before a
//! container restart and `verify` after it. Each client is configured only by
//! `WYRD_SERVER_URL` and `WYRD_API_KEY`, so gRPC must reach nginx's public
//! port `50051` through the address derived from the server URL. The tests
//! share the Card uid and table name through `WYRD_STARTUP_STATE_DIR`.
//!
//! `scripts/server/test-kind-autoscale.sh` runs the three `kind_*` phases the
//! same way against port-forwarded pods: `kind_seed` and `kind_append` on the
//! fixed `all` anchor, and `kind_join_read` on the Oracle replica the
//! HorizontalPodAutoscaler added.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::Array;
use arrow_schema::{DataType, Field, Schema};
use wyrd_client::bifrost::{Bifrost, Correlation, TableConfig};
use wyrd_client::cards::Cards;
use wyrd_client::config::ClientConfig;
use wyrd_client::{GlobalConfig, WyrdClient};
use wyrd_spec::ids::CardUid;
use wyrd_spec::vala::api::{BifrostQueryRequest, RegisterOutcome};

/// Bytes of the one artifact the journey registers and reads back.
const ARTIFACT: &[u8] = b"startup image journey artifact";

/// Directory the script provides for state shared across the restart.
///
/// # Panics
/// Panics when `WYRD_STARTUP_STATE_DIR` is unset; the lane always sets it.
fn state_dir() -> PathBuf {
    PathBuf::from(std::env::var("WYRD_STARTUP_STATE_DIR").expect("lane sets the state dir"))
}

/// A client resolved from the environment alone: server URL plus API key.
///
/// # Panics
/// Panics when the client cannot be constructed from the environment.
fn env_client() -> WyrdClient {
    WyrdClient::with_config(ClientConfig::from_env()).expect("client from environment")
}

/// The journey table: one integer key and one text value.
///
/// # Panics
/// Panics when the static schema is not a valid table declaration.
fn table(fqn: &str) -> TableConfig {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    TableConfig::from_arrow(fqn, schema).expect("journey table")
}

/// Poll a strict fused `SELECT id` on `fqn` until it returns `expected`, or fail after 90s.
///
/// Fused visibility is the read contract for acknowledged rows: they are
/// durable in the WAL but publish only when a generation seals, so a
/// published-only read may legitimately miss them. The rows reach the live
/// tail asynchronously, so a bounded poll is the observable contract rather
/// than a synchronization sleep.
///
/// # Panics
/// Panics when the rows never match within the deadline, naming the last
/// query error so a refused read is not mistaken for an empty one.
async fn await_ids(bifrost: &Bifrost, fqn: &str, expected: &[i64]) {
    let request = BifrostQueryRequest {
        sql: format!("SELECT id FROM {fqn} ORDER BY id"),
        deadline_ms: Some(30_000),
    };
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut last = Vec::new();
    let mut last_error = None;
    while Instant::now() < deadline {
        match read_ids(bifrost, &request).await {
            Err(error) => last_error = Some(error.to_string()),
            Ok(ids) => last = ids,
        }
        if last == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    panic!("rows {last:?} never matched {expected:?} in {fqn}; last query error: {last_error:?}");
}

/// Run one query and collect its first `Int64` column.
///
/// # Errors
/// Returns the client error that refused or interrupted the query stream.
///
/// # Panics
/// Panics when the first column is not `Int64`.
async fn read_ids(
    bifrost: &Bifrost,
    request: &BifrostQueryRequest,
) -> Result<Vec<i64>, wyrd_client::bifrost::BifrostClientError> {
    let mut stream = bifrost.query(request).await?;
    let mut ids = Vec::new();
    while let Some(batch) = stream.next_batch().await? {
        let column = batch
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .expect("id is Int64");
        ids.extend(column.values().iter().copied());
    }
    Ok(ids)
}

/// Before restart: derive gRPC from the server URL, write through HTTP and
/// gRPC, and prove an explicit gRPC override also works.
#[tokio::test]
#[ignore = "run by scripts/server/test-startup.sh against the official image"]
async fn startup_image_write() {
    let client = env_client();
    let server_url = client.server_url().to_owned();
    assert!(server_url.starts_with("http://127.0.0.1:"));
    let derived = "http://127.0.0.1:50051";
    assert_eq!(client.grpc_url(), derived, "gRPC derives from server_url");

    let dir = state_dir();
    let card_dir = dir.join("card");
    std::fs::create_dir_all(&card_dir).expect("card dir");
    std::fs::write(card_dir.join("prompt.txt"), ARTIFACT).expect("artifact source");
    // The manifest carries the standard-base64 SHA-256 the client recomputes.
    let digest = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        <sha2::Sha256 as sha2::Digest>::digest(ARTIFACT),
    );
    std::fs::write(
        card_dir.join("card.yaml"),
        format!(
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: startup-prompt\n  \
             version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-4o\n  \
             messages: [hello]\nartifacts:\n  - relative_path: prompt.txt\n    \
             sha256: {digest}\n    size_bytes: {}\n    content_type: text/plain\n",
            ARTIFACT.len()
        ),
    )
    .expect("card document");
    let receipt = Cards::with_client(client.clone())
        .register_from_path(&card_dir.join("card.yaml"))
        .await
        .expect("register a Card with a local artifact through nginx");
    let card_uid = receipt.root.uid.expect("registered root carries its uid");

    let fqn = format!("vala.datasets.startup_{}", uuid::Uuid::now_v7().simple());
    let bifrost = Bifrost::connect_with_table(&client, table(&fqn))
        .await
        .expect("Bifrost over derived gRPC");
    assert_eq!(
        bifrost.register().await.expect("register"),
        RegisterOutcome::Created
    );
    bifrost
        .insert(
            br#"{"id": 1, "value": "derived"}"#.to_vec(),
            Correlation::default(),
        )
        .expect("insert over derived gRPC");
    bifrost
        .flush()
        .await
        .expect("acknowledged over derived gRPC");
    bifrost.shutdown().await.expect("drain");

    let overridden = WyrdClient::with_config(ClientConfig::from_global_with_overrides(
        &GlobalConfig::default(),
        Some(&server_url),
        Some(derived),
    ))
    .expect("client with explicit gRPC override");
    let explicit = Bifrost::connect_with_table(&overridden, table(&fqn))
        .await
        .expect("Bifrost over explicit gRPC");
    explicit
        .insert(
            br#"{"id": 2, "value": "explicit"}"#.to_vec(),
            Correlation::default(),
        )
        .expect("insert over explicit gRPC");
    explicit
        .flush()
        .await
        .expect("acknowledged over explicit gRPC");
    explicit.shutdown().await.expect("drain");
    await_ids(&explicit, &fqn, &[1, 2]).await;

    std::fs::write(dir.join("state"), format!("{card_uid}\n{fqn}\n")).expect("state");
}

/// After restart: the credential, Card, artifact bytes, and acknowledged
/// Bifrost rows written before the restart are all still readable.
#[tokio::test]
#[ignore = "run by scripts/server/test-startup.sh against the official image"]
async fn startup_image_verify() {
    let dir = state_dir();
    let state = std::fs::read_to_string(dir.join("state")).expect("write phase state");
    let mut lines = state.lines();
    let card_uid = CardUid::new(lines.next().expect("card uid")).expect("valid card uid");
    let fqn = lines.next().expect("table").to_owned();

    let client = env_client();
    let download = dir.join("download");
    Cards::with_client(client.clone())
        .download_artifacts_to(&card_uid, &download)
        .await
        .expect("download the artifact after restart");
    assert_eq!(
        std::fs::read(download.join("prompt.txt")).expect("downloaded artifact"),
        ARTIFACT
    );

    let bifrost = Bifrost::connect_with_table(&client, table(&fqn))
        .await
        .expect("Bifrost after restart");
    await_ids(&bifrost, &fqn, &[1, 2]).await;
    bifrost.shutdown().await.expect("drain");
}

/// Inserts `ids` into `fqn` through `client` and waits for the acknowledgement.
///
/// # Panics
/// Panics when the table cannot be opened or a row is not acknowledged.
async fn insert_ids(client: &WyrdClient, fqn: &str, ids: std::ops::RangeInclusive<i64>) {
    let bifrost = Bifrost::connect_with_table(client, table(fqn))
        .await
        .expect("Bifrost on the anchor");
    bifrost
        .register()
        .await
        .expect("register or reuse the table");
    for id in ids {
        bifrost
            .insert(
                format!(r#"{{"id": {id}, "value": "kind"}}"#).into_bytes(),
                Correlation::default(),
            )
            .expect("insert on the anchor");
    }
    bifrost
        .flush()
        .await
        .expect("acknowledged by the anchor's Scribe");
    bifrost.shutdown().await.expect("drain");
}

/// Before the scale-up, through the fixed `all` anchor: its Scribe
/// acknowledges rows `1..=4`, and a fused read through it returns them.
#[tokio::test]
#[ignore = "run by scripts/server/test-kind-autoscale.sh against a kind cluster"]
async fn kind_seed() {
    let client = env_client();
    let fqn = format!("vala.datasets.kind_{}", uuid::Uuid::now_v7().simple());
    insert_ids(&client, &fqn, 1..=4).await;
    let bifrost = Bifrost::connect_with_table(&client, table(&fqn))
        .await
        .expect("Bifrost for the seed read");
    await_ids(&bifrost, &fqn, &[1, 2, 3, 4]).await;
    bifrost.shutdown().await.expect("drain");
    std::fs::write(state_dir().join("kind_table"), &fqn).expect("kind state");
}

/// After the scale-up, through the anchor again: its Scribe acknowledges
/// rows `5..=8`, which stay in its live tail until a generation seals.
#[tokio::test]
#[ignore = "run by scripts/server/test-kind-autoscale.sh against a kind cluster"]
async fn kind_append() {
    let fqn = std::fs::read_to_string(state_dir().join("kind_table")).expect("seed phase state");
    insert_ids(&env_client(), &fqn, 5..=8).await;
}

/// Oracle replica added by the autoscaler: one strict fused read through it
/// returns all eight rows. The replica owns no Scribe, so the read executes on
/// its own Oracle and must fetch the anchor's live Scribe tail over mTLS; a
/// missing join, handshake, or remote call fails or short-reads.
#[tokio::test]
#[ignore = "run by scripts/server/test-kind-autoscale.sh against a kind cluster"]
async fn kind_join_read() {
    let fqn = std::fs::read_to_string(state_dir().join("kind_table")).expect("seed phase state");
    let bifrost = Bifrost::query_only(&env_client());
    await_ids(&bifrost, &fqn, &(1..=8).collect::<Vec<_>>()).await;
}
