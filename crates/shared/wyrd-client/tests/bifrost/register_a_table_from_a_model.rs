//! Register a table from nested derived models, write rows, and query inside them.
//!
//! Nested models register as struct columns, queried by path:
//! `prediction['feature_importance']['importance']`. A `serde_json::Value`
//! field registers as a Variant column for open data, queried with `->` and
//! `->>`.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wyrd_client::bifrost::{Bifrost, Correlation, TableConfig};
use wyrd_spec::vala::api::RegisterOutcome;
use wyrd_testing::server::WyrdTestServer;

use crate::pg_tests::admin_client;

/// The table the API requests are registered and written to.
const API_REQUESTS: &str = "vala.datasets.api_requests";

/// Which feature mattered most to one prediction.
#[derive(Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
struct FeatureImportance {
    /// The feature's name.
    name: String,
    /// How much the feature mattered.
    importance: f64,
}

/// One model prediction, nested in an API request.
#[derive(Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
struct Prediction {
    /// The predicted class.
    result: i64,
    /// The probability of each class.
    probabilities: Vec<f64>,
    /// The most important feature.
    feature_importance: FeatureImportance,
}

/// One served API request: each field becomes one Iceberg column.
#[derive(Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
struct ApiRequest {
    /// The request's identity.
    request_id: String,
    /// The observed outcome.
    outcome: i64,
    /// The nested prediction: a struct column.
    prediction: Prediction,
    /// Open request attributes: a Variant column.
    attributes: Value,
}

/// A selected nested importance.
#[derive(Debug, PartialEq, Deserialize)]
struct Importance {
    /// The request's identity.
    request_id: String,
    /// The selected `prediction['feature_importance']['importance']`.
    importance: f64,
}

/// A selected Variant key.
#[derive(Debug, PartialEq, Deserialize)]
struct Region {
    /// The request's identity.
    request_id: String,
    /// The selected `attributes -> 'client' ->> 'region'`.
    region: String,
}

/// The two requests every test in this story writes and reads.
fn requests() -> Vec<ApiRequest> {
    vec![
        ApiRequest {
            request_id: "req-1".to_owned(),
            outcome: 1,
            prediction: Prediction {
                result: 1,
                probabilities: vec![0.1, 0.9],
                feature_importance: FeatureImportance {
                    name: "tenure".to_owned(),
                    importance: 0.72,
                },
            },
            attributes: json!({"client": {"region": "us-east"}, "retries": 0}),
        },
        ApiRequest {
            request_id: "req-2".to_owned(),
            outcome: 0,
            prediction: Prediction {
                result: 0,
                probabilities: vec![0.8, 0.2],
                feature_importance: FeatureImportance {
                    name: "spend".to_owned(),
                    importance: 0.31,
                },
            },
            attributes: json!({"client": {"region": "eu-west"}, "retries": 2, "coupon": null}),
        },
    ]
}

/// Register `API_REQUESTS` from [`ApiRequest`] and write [`requests`].
///
/// # Panics
///
/// Panics when registration, the insert, the flush, or publication fails.
async fn api_requests(srv: &WyrdTestServer) -> Bifrost {
    let client = admin_client(srv, "register-a-table-from-a-model").await;
    let api_requests = Bifrost::connect_with_table(
        &client,
        TableConfig::from_model::<ApiRequest>(API_REQUESTS).expect("the model declares a table"),
    )
    .await
    .expect("the writer connects");
    api_requests.register().await.expect("the table registers");
    for request in requests() {
        let row = serde_json::to_vec(&request).expect("the request serializes");
        api_requests
            .insert(row, Correlation::default())
            .expect("the row is admitted");
    }
    api_requests.flush().await.expect("the rows flush");
    srv.flush_bifrost().await.expect("the rows publish");
    api_requests
}

/// Requests written from the model read back as the same models.
///
/// # Panics
///
/// Panics when the rows read back differ.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn api_requests_read_back_as_the_same_models() {
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    let api_requests = api_requests(&srv).await;

    let rows: Vec<ApiRequest> = api_requests
        .sql_as(&format!(
            "SELECT request_id, outcome, prediction, attributes \
             FROM {API_REQUESTS} ORDER BY request_id"
        ))
        .await
        .expect("the rows read back");

    assert_eq!(rows, requests());
    srv.shutdown().await.expect("server shutdown");
}

/// A nested model field is selected by its path.
///
/// # Panics
///
/// Panics when the selected values differ.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn a_nested_model_field_is_selected_by_its_path() {
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    let api_requests = api_requests(&srv).await;

    let rows: Vec<Importance> = api_requests
        .sql_as(&format!(
            "SELECT request_id, prediction['feature_importance']['importance'] AS importance \
             FROM {API_REQUESTS} ORDER BY request_id"
        ))
        .await
        .expect("the path selects");

    assert_eq!(
        rows,
        [
            Importance {
                request_id: "req-1".to_owned(),
                importance: 0.72,
            },
            Importance {
                request_id: "req-2".to_owned(),
                importance: 0.31,
            },
        ]
    );
    srv.shutdown().await.expect("server shutdown");
}

/// A nested model field filters rows.
///
/// # Panics
///
/// Panics when the filtered rows differ.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn a_nested_model_field_filters_rows() {
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    let api_requests = api_requests(&srv).await;

    let rows: Vec<Importance> = api_requests
        .sql_as(&format!(
            "SELECT request_id, prediction['feature_importance']['importance'] AS importance \
             FROM {API_REQUESTS} \
             WHERE prediction['feature_importance']['importance'] > 0.5"
        ))
        .await
        .expect("the path filters");

    assert_eq!(
        rows,
        [Importance {
            request_id: "req-1".to_owned(),
            importance: 0.72,
        }]
    );
    srv.shutdown().await.expect("server shutdown");
}

/// An open JSON field is queried by key.
///
/// # Panics
///
/// Panics when the selected keys differ.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn an_open_dict_field_is_queried_by_key() {
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    let api_requests = api_requests(&srv).await;

    let rows: Vec<Region> = api_requests
        .sql_as(&format!(
            "SELECT request_id, attributes -> 'client' ->> 'region' AS region \
             FROM {API_REQUESTS} ORDER BY request_id"
        ))
        .await
        .expect("the key selects");

    assert_eq!(
        rows,
        [
            Region {
                request_id: "req-1".to_owned(),
                region: "us-east".to_owned(),
            },
            Region {
                request_id: "req-2".to_owned(),
                region: "eu-west".to_owned(),
            },
        ]
    );
    srv.shutdown().await.expect("server shutdown");
}

/// Registering the same model again finds the existing table.
///
/// # Panics
///
/// Panics when the second registration is not `AlreadyExists`.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn registering_the_same_model_again_finds_the_existing_table() {
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    let api_requests = api_requests(&srv).await;

    assert_eq!(
        api_requests.register().await.expect("the table registers"),
        RegisterOutcome::AlreadyExists
    );
    srv.shutdown().await.expect("server shutdown");
}
