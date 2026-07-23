use std::sync::Arc;

use secrecy::SecretString;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_registry::{CardSelector, Cards};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardUid, SpaceName};
use wyrd_spec::registry::ListCardsRequest;

#[derive(Clone, Debug)]
struct Request {
    method: String,
    target: String,
    body: Vec<u8>,
}

#[derive(Clone, Debug)]
struct Response {
    status: u16,
    body: Vec<u8>,
}

struct TestServer {
    uri: String,
    requests: Arc<Mutex<Vec<Request>>>,
    task: tokio::task::JoinHandle<()>,
}

impl TestServer {
    async fn start(responses: Vec<Response>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("test listener binds");
        let uri = format!(
            "http://{}",
            listener.local_addr().expect("listener address")
        );
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let task = tokio::spawn(async move {
            for response in responses {
                let (mut stream, _) = listener.accept().await.expect("test request connects");
                let request = read_request(&mut stream).await;
                captured.lock().await.push(request);
                write_response(&mut stream, response)
                    .await
                    .expect("test response writes");
            }
        });
        Self {
            uri,
            requests,
            task,
        }
    }

    async fn requests(&self) -> Vec<Request> {
        self.requests.lock().await.clone()
    }

    fn uri(&self) -> &str {
        &self.uri
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> Request {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).await.expect("test request reads");
        assert!(read > 0, "request ended before headers");
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.strip_prefix("content-length:")
                .or_else(|| line.strip_prefix("Content-Length:"))
        })
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    while bytes.len() < header_end + content_length {
        let read = stream.read(&mut chunk).await.expect("test body reads");
        assert!(read > 0, "request ended before body");
        bytes.extend_from_slice(&chunk[..read]);
    }
    let request_line = headers.lines().next().expect("request line exists");
    let mut parts = request_line.split_whitespace();
    Request {
        method: parts.next().expect("request method exists").to_owned(),
        target: parts.next().expect("request target exists").to_owned(),
        body: bytes[header_end..header_end + content_length].to_vec(),
    }
}

async fn write_response(
    stream: &mut tokio::net::TcpStream,
    response: Response,
) -> std::io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        404 => "Not Found",
        _ => "Test",
    };
    let head = format!(
        "HTTP/1.1 {} {}\r\ncontent-length: {}\r\ncontent-type: application/json\r\nconnection: close\r\n\r\n",
        response.status,
        reason,
        response.body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&response.body).await
}

fn json_response(status: u16, body: serde_json::Value) -> Response {
    Response {
        status,
        body: serde_json::to_vec(&body).expect("test JSON serializes"),
    }
}

fn client(base_url: impl Into<String>) -> WyrdClient {
    let base_url = base_url.into();
    let mut config = ClientConfig::default();
    config.http.base_url = base_url.clone();
    let auth = AuthMiddleware::new(
        &config,
        ResolvedCredential::BearerToken(SecretString::from("test-bearer")),
    )
    .expect("auth builds");
    let transport = HttpTransport::new(
        &HttpConfig {
            base_url,
            ..HttpConfig::default()
        },
        Arc::clone(&auth),
    )
    .expect("transport builds");
    WyrdClient::from_parts(auth, transport, config.grpc)
}

fn uid() -> CardUid {
    CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00").expect("test uid is valid")
}

#[tokio::test]
async fn list_uses_typed_query_parameters_and_an_empty_get_body() {
    let server = TestServer::start(vec![json_response(
        200,
        serde_json::json!({
            "items": [],
            "next_cursor": null
        }),
    )])
    .await;

    let cards = Cards::with_client(client(server.uri()));
    let page = cards
        .list(ListCardsRequest {
            kind: Some(CardKind::Prompt),
            space: Some(SpaceName::new("prod").expect("test space is valid")),
            name: None,
            version_range: None,
            status: None,
            filter: None,
            include_prerelease: false,
            limit: Some(20),
            cursor: Some("opaque-next".to_owned()),
        })
        .await
        .expect("typed list succeeds");
    assert!(page.items.is_empty());
    let requests = server.requests().await;
    assert_eq!(requests[0].method, "GET");
    assert_eq!(
        requests[0].target,
        "/v1/cards?kind=Prompt&space=prod&include_prerelease=false&limit=20&cursor=opaque-next"
    );
    assert!(requests[0].body.is_empty());
}

#[tokio::test]
async fn uid_delete_uses_kind_qualified_path_and_preserves_idempotence() {
    let server = TestServer::start(vec![json_response(
        200,
        serde_json::json!({
            "card_uid": "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00",
            "deleted": false
        }),
    )])
    .await;

    let cards = Cards::with_client(client(server.uri()));
    cards
        .delete(CardSelector::uid(CardKind::Prompt, uid()))
        .await
        .expect("idempotent delete succeeds when already deleted");
    let requests = server.requests().await;
    assert_eq!(requests[0].method, "DELETE");
    assert_eq!(
        requests[0].target,
        "/v1/cards/by-uid/Prompt/01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00"
    );
    assert!(requests[0].body.is_empty());
}

#[tokio::test]
async fn server_problem_code_and_status_survive_registry_boundary() {
    let server = TestServer::start(vec![json_response(
        404,
        serde_json::json!({
            "code": "WYRD_REGISTRY_404_CARD_NOT_FOUND",
            "detail": "card not found",
            "details": {"tenant": "redacted"}
        }),
    )])
    .await;

    let cards = Cards::with_client(client(server.uri()));
    let error = cards
        .get(CardSelector::uid(CardKind::Prompt, uid()))
        .await
        .expect_err("missing card must be returned as a Wyrd error");
    assert_eq!(error.code(), "WYRD_REGISTRY_404_CARD_NOT_FOUND");
    assert_eq!(error.status(), 404);
    assert_eq!(error.as_problem_json()["details"]["tenant"], "redacted");
    let requests = server.requests().await;
    assert_eq!(requests[0].method, "GET");
    assert_eq!(
        requests[0].target,
        "/v1/cards/by-uid/Prompt/01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00"
    );
}

#[test]
fn client_debug_redacts_constructor_secret() {
    let config = ClientConfig {
        api_key: Some(SecretString::from("wyrd_sk_private")),
        http: HttpConfig {
            base_url: "http://localhost:50050".to_owned(),
            ..HttpConfig::default()
        },
        ..ClientConfig::default()
    };
    let client = WyrdClient::with_config(config).expect("configured client assembles");
    let rendered = format!("{client:?}");
    assert!(!rendered.contains("wyrd_sk_private"));
}
