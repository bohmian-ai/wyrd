//! The OTLP metric journey: every supported point kind, on every route.

use wyrd_tonic::otlp::metrics::v1::ResourceMetrics;
use wyrd_tonic::otlp::metrics_service::metrics_service_client::MetricsServiceClient;
use wyrd_tonic::otlp::metrics_service::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use wyrd_tonic::tonic::Request;
use wyrd_tonic::tonic::transport::Channel;

use super::support::OtlpJourney;
use super::trace_export_http::{HttpEncoding, post_otlp};

/// Sends one OTLP metric export through the bound gRPC collector route.
///
/// # Panics
///
/// Panics when the transport cannot be dialed or the export is refused.
pub(super) async fn export_metrics_over_grpc(
    journey: &OtlpJourney,
    resource_metrics: Vec<ResourceMetrics>,
) {
    let channel = Channel::from_shared(journey.grpc_url())
        .expect("the bound gRPC URL is a valid endpoint")
        .connect()
        .await
        .expect("the OTLP exporter dials the bound collector");
    let mut request = Request::new(ExportMetricsServiceRequest { resource_metrics });
    request.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {}", journey.token())
            .parse()
            .expect("the minted bearer is valid ASCII metadata"),
    );
    let partial = MetricsServiceClient::new(channel)
        .export(request)
        .await
        .expect("the collector accepts the metric export")
        .into_inner()
        .partial_success;
    assert!(
        partial.is_none_or(|partial| partial.rejected_data_points == 0),
        "a wholly valid export reports no rejected data point"
    );
}

/// Posts one metric export in the requested HTTP encoding.
///
/// # Panics
///
/// Panics when the collector refuses the export.
pub(super) async fn export_metrics_over_http(
    journey: &OtlpJourney,
    encoding: HttpEncoding,
    resource_metrics: Vec<ResourceMetrics>,
) {
    let request = ExportMetricsServiceRequest { resource_metrics };
    let payload = match encoding {
        HttpEncoding::Json => otlp_json_body(&request),
        HttpEncoding::Protobuf => encoding.encode(&request),
    };
    let body = post_otlp(journey, "/v1/metrics", encoding, payload).await;
    let response: ExportMetricsServiceResponse = encoding.decode(&body);
    if let Some(partial) = response.partial_success {
        assert_eq!(
            partial.rejected_data_points,
            0,
            "a wholly valid {} export reports no rejected data point: {}",
            encoding.content_type(),
            partial.error_message
        );
    }
}

/// Serializes one metrics request as the OTLP JSON mapping actually defines it.
///
/// `opentelemetry-proto` flattens the numeric oneof of a data point but not the
/// one on an exemplar, so its serde output nests the exemplar's value under a
/// `value` object. The OTLP JSON mapping — and therefore every real exporter
/// and this server's decoder — puts `asInt`/`asDouble` directly on the
/// exemplar. Sending the nested form would prove only that the fixture and the
/// generated types agree with each other.
fn otlp_json_body(request: &ExportMetricsServiceRequest) -> Vec<u8> {
    let mut body = serde_json::to_value(request).expect("the pinned request serializes to JSON");
    flatten_exemplar_values(&mut body);
    serde_json::to_vec(&body).expect("the corrected request serializes to JSON")
}

/// Hoists every exemplar's nested numeric oneof onto the exemplar itself.
///
/// Restricted to members of an `exemplars` array on purpose: an attribute's
/// `value` object is a real nested message and must keep its nesting.
fn flatten_exemplar_values(node: &mut serde_json::Value) {
    if let Some(exemplars) = node
        .get_mut("exemplars")
        .and_then(serde_json::Value::as_array_mut)
    {
        for exemplar in exemplars {
            let Some(nested) = exemplar
                .as_object_mut()
                .and_then(|object| object.remove("value"))
            else {
                continue;
            };
            if let (Some(object), Some(fields)) = (exemplar.as_object_mut(), nested.as_object()) {
                for (key, value) in fields {
                    object.insert(key.clone(), value.clone());
                }
            }
        }
    }
    match node {
        serde_json::Value::Array(items) => items.iter_mut().for_each(flatten_exemplar_values),
        serde_json::Value::Object(map) => map.values_mut().for_each(flatten_exemplar_values),
        _ => {}
    }
}

/// Tests that need Postgres, a bound server, and the publication boundary.
mod pg_tests {
    use arrow::array::{
        Array, BooleanArray, FixedSizeBinaryArray, Float64Array, Int32Array, Int64Array,
        LargeBinaryArray, ListArray, StringArray, StructArray,
    };
    use arrow::record_batch::RecordBatch;

    use super::super::support::{
        self, EXEMPLAR_INT_VALUE, EXPONENTIAL_COUNT, EXPONENTIAL_HISTOGRAM_METRIC, EXPONENTIAL_MAX,
        EXPONENTIAL_MIN, EXPONENTIAL_NEGATIVE_COUNTS, EXPONENTIAL_NEGATIVE_OFFSET,
        EXPONENTIAL_POSITIVE_COUNTS, EXPONENTIAL_POSITIVE_OFFSET, EXPONENTIAL_SCALE,
        EXPONENTIAL_SUM, EXPONENTIAL_ZERO_COUNT, EXPONENTIAL_ZERO_THRESHOLD, GAUGE_DOUBLE_METRIC,
        GAUGE_DOUBLE_VALUE, GAUGE_INT_METRIC, GAUGE_INT_VALUE, GRPC_SPAN, HISTOGRAM_BUCKET_COUNTS,
        HISTOGRAM_COUNT, HISTOGRAM_EXPLICIT_BOUNDS, HISTOGRAM_MAX, HISTOGRAM_METRIC, HISTOGRAM_MIN,
        HISTOGRAM_SUM, METRIC_DESCRIPTION, METRIC_EXEMPLAR_OFFSET_NANOS, METRIC_FLAGS,
        METRIC_SCOPE_NAME, METRIC_START_OFFSET_NANOS, METRIC_TEMPORALITY, METRIC_UNIT,
        METRICS_TABLE, OtlpJourney, RESOURCE_DROPPED_ATTRIBUTES, RESOURCE_SCHEMA_URL,
        SCOPE_DROPPED_ATTRIBUTES, SCOPE_SCHEMA_URL, SCOPE_VERSION, SUM_DOUBLE_METRIC,
        SUM_DOUBLE_VALUE, SUM_INT_METRIC, SUM_INT_VALUE, SUM_IS_MONOTONIC, SUMMARY_COUNT,
        SUMMARY_METRIC, SUMMARY_QUANTILES, SUMMARY_SUM, column, row_by_string,
    };
    use super::super::trace_export_http::HttpEncoding;
    use super::{export_metrics_over_grpc, export_metrics_over_http};

    /// Reads one child column out of a single-row struct value.
    ///
    /// # Panics
    ///
    /// Panics when the child is missing or is not of the requested Arrow type.
    fn child<'batch, A: Array + 'static>(parent: &'batch StructArray, name: &str) -> &'batch A {
        let array = parent
            .column_by_name(name)
            .unwrap_or_else(|| panic!("the struct carries a `{name}` child"));
        array.as_any().downcast_ref::<A>().unwrap_or_else(|| {
            panic!(
                "`{name}` has the canonical Arrow type, got {}",
                array.data_type()
            )
        })
    }

    /// Reads the single stored list value of one column as a flat array.
    ///
    /// # Panics
    ///
    /// Panics when the column is not a list or its element type differs.
    fn list_values(row: &RecordBatch, name: &str) -> std::sync::Arc<dyn Array> {
        let list = column::<ListArray>(row, name);
        assert!(list.is_valid(0), "`{name}` is stored for this point kind");
        list.value(0)
    }

    /// Asserts the descriptor and envelope columns every fixture point shares.
    ///
    /// # Panics
    ///
    /// Panics when any shared column differs from the exported fixture.
    fn assert_shared_columns(row: &RecordBatch, time: i64, kind: &str, transport: &str) {
        assert_eq!(
            column::<StringArray>(row, "metric_type").value(0),
            kind,
            "{transport} stores the {kind} discriminant"
        );
        assert_eq!(
            column::<StringArray>(row, "description").value(0),
            METRIC_DESCRIPTION
        );
        assert_eq!(column::<StringArray>(row, "unit").value(0), METRIC_UNIT);
        assert_eq!(
            column::<LargeBinaryArray>(row, "metadata").value(0),
            support::canonical_attribute_bytes(&support::metric_metadata()),
            "{transport} keeps metric metadata distinct from point attributes"
        );
        assert_eq!(column::<Int64Array>(row, "time_unix_nano").value(0), time);
        assert_eq!(
            column::<Int64Array>(row, "start_time_unix_nano").value(0),
            time - METRIC_START_OFFSET_NANOS
        );
        assert_eq!(column::<Int64Array>(row, "flags").value(0), METRIC_FLAGS);
        assert_eq!(
            column::<LargeBinaryArray>(row, "attributes").value(0),
            support::canonical_attribute_bytes(&support::point_attributes())
        );
        assert!(column::<BooleanArray>(row, "resource_present").value(0));
        assert_eq!(
            column::<LargeBinaryArray>(row, "resource_attributes").value(0),
            support::canonical_attribute_bytes(&support::resource_attributes())
        );
        assert_eq!(
            column::<Int64Array>(row, "resource_dropped_attributes_count").value(0),
            RESOURCE_DROPPED_ATTRIBUTES
        );
        assert_eq!(
            column::<StringArray>(row, "resource_schema_url").value(0),
            RESOURCE_SCHEMA_URL
        );
        assert!(column::<BooleanArray>(row, "scope_present").value(0));
        assert_eq!(
            column::<StringArray>(row, "scope_name").value(0),
            METRIC_SCOPE_NAME
        );
        assert_eq!(
            column::<StringArray>(row, "scope_version").value(0),
            SCOPE_VERSION
        );
        assert_eq!(
            column::<LargeBinaryArray>(row, "scope_attributes").value(0),
            support::canonical_attribute_bytes(&support::scope_attributes())
        );
        assert_eq!(
            column::<Int64Array>(row, "scope_dropped_attributes_count").value(0),
            SCOPE_DROPPED_ATTRIBUTES
        );
        assert_eq!(
            column::<StringArray>(row, "scope_schema_url").value(0),
            SCOPE_SCHEMA_URL
        );
    }

    /// Every supported point kind survives every OTLP route unchanged.
    ///
    /// One export carries a gauge and a sum in both their integer and their
    /// double alternative, an explicit-bucket histogram, an exponential
    /// histogram with populated positive and negative buckets, and a summary
    /// with two quantiles — plus one exemplar correlating back to the span the
    /// trace case stores. Each is sent over gRPC, OTLP/HTTP protobuf and
    /// OTLP/HTTP protobuf-JSON, and read back through canonical SQL. The
    /// integer and double alternatives are asserted as distinct columns: a
    /// projector that funnelled both through one numeric column would still
    /// round-trip the value while destroying the caller's declared type.
    ///
    /// # Panics
    ///
    /// Panics when an export is refused, a stored column differs from what was
    /// sent, or a column a point kind does not own is populated.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn otlp_metric_routes_round_trip_every_supported_point_kind() {
        let journey = OtlpJourney::start().await;
        let anchor = support::anchor_nanos();

        export_metrics_over_grpc(&journey, support::maximal_resource_metrics(anchor)).await;
        for (offset, encoding) in [(1, HttpEncoding::Protobuf), (2, HttpEncoding::Json)] {
            export_metrics_over_http(
                &journey,
                encoding,
                support::maximal_resource_metrics(anchor + offset),
            )
            .await;
        }
        journey.publish().await;

        for (offset, transport) in [(0, "gRPC"), (1, "HTTP protobuf"), (2, "HTTP JSON")] {
            let time = anchor + offset;
            let rows = journey
                .query(&format!(
                    "SELECT * FROM {METRICS_TABLE} WHERE time_unix_nano = {time}"
                ))
                .await;
            let stored: Vec<String> = rows
                .iter()
                .flat_map(|batch| {
                    let names = column::<StringArray>(batch, "metric_name");
                    (0..batch.num_rows())
                        .map(|index| names.value(index).to_owned())
                        .collect::<Vec<_>>()
                })
                .collect();
            assert_eq!(
                stored.len(),
                7,
                "{transport} stores one row for each exported data point, got {stored:?}"
            );

            let gauge_int = row_by_string(&rows, "metric_name", GAUGE_INT_METRIC);
            assert_shared_columns(&gauge_int, time, "gauge", transport);
            assert_eq!(
                column::<Int64Array>(&gauge_int, "int_value").value(0),
                GAUGE_INT_VALUE
            );
            assert!(
                column::<Float64Array>(&gauge_int, "double_value").is_null(0),
                "{transport} keeps an integer gauge out of the double column"
            );
            assert!(
                column::<Int32Array>(&gauge_int, "aggregation_temporality").is_null(0),
                "a gauge declares no temporality"
            );
            assert!(
                column::<BooleanArray>(&gauge_int, "is_monotonic").is_null(0),
                "a gauge declares no monotonicity"
            );
            let exemplars = list_values(&gauge_int, "exemplars");
            let exemplars = exemplars
                .as_any()
                .downcast_ref::<StructArray>()
                .expect("exemplars are stored as structs");
            assert_eq!(exemplars.len(), 1, "{transport} stores the one exemplar");
            assert_eq!(
                child::<Int64Array>(exemplars, "time_unix_nano").value(0),
                time + METRIC_EXEMPLAR_OFFSET_NANOS
            );
            assert_eq!(
                child::<Int64Array>(exemplars, "int_value").value(0),
                EXEMPLAR_INT_VALUE
            );
            assert_eq!(
                child::<LargeBinaryArray>(exemplars, "filtered_attributes").value(0),
                support::canonical_attribute_bytes(&support::exemplar_attributes())
            );
            assert_eq!(
                child::<FixedSizeBinaryArray>(exemplars, "trace_id").value(0),
                GRPC_SPAN.trace_id,
                "{transport} keeps the exemplar's span correlation"
            );
            assert_eq!(
                child::<FixedSizeBinaryArray>(exemplars, "span_id").value(0),
                GRPC_SPAN.span_id
            );

            let gauge_double = row_by_string(&rows, "metric_name", GAUGE_DOUBLE_METRIC);
            assert_shared_columns(&gauge_double, time, "gauge", transport);
            assert!(
                column::<Int64Array>(&gauge_double, "int_value").is_null(0),
                "{transport} keeps a double gauge out of the integer column"
            );
            assert!(
                (column::<Float64Array>(&gauge_double, "double_value").value(0)
                    - GAUGE_DOUBLE_VALUE)
                    .abs()
                    < f64::EPSILON
            );

            let sum_int = row_by_string(&rows, "metric_name", SUM_INT_METRIC);
            assert_shared_columns(&sum_int, time, "sum", transport);
            assert_eq!(
                column::<Int64Array>(&sum_int, "int_value").value(0),
                SUM_INT_VALUE
            );
            assert_eq!(
                column::<Int32Array>(&sum_int, "aggregation_temporality").value(0),
                METRIC_TEMPORALITY
            );
            assert_eq!(
                column::<BooleanArray>(&sum_int, "is_monotonic").value(0),
                SUM_IS_MONOTONIC
            );

            let sum_double = row_by_string(&rows, "metric_name", SUM_DOUBLE_METRIC);
            assert_shared_columns(&sum_double, time, "sum", transport);
            assert!(
                (column::<Float64Array>(&sum_double, "double_value").value(0) - SUM_DOUBLE_VALUE)
                    .abs()
                    < f64::EPSILON
            );

            let histogram = row_by_string(&rows, "metric_name", HISTOGRAM_METRIC);
            assert_shared_columns(&histogram, time, "histogram", transport);
            assert_eq!(
                column::<Int64Array>(&histogram, "histogram_count").value(0),
                HISTOGRAM_COUNT
            );
            for (name, expected) in [
                ("histogram_sum", HISTOGRAM_SUM),
                ("histogram_min", HISTOGRAM_MIN),
                ("histogram_max", HISTOGRAM_MAX),
            ] {
                assert!(
                    (column::<Float64Array>(&histogram, name).value(0) - expected).abs()
                        < f64::EPSILON,
                    "{transport} stores `{name}` exactly"
                );
            }
            let buckets = list_values(&histogram, "bucket_counts");
            let buckets = buckets
                .as_any()
                .downcast_ref::<Int64Array>()
                .expect("bucket counts are stored as int64");
            assert_eq!(**buckets.values(), HISTOGRAM_BUCKET_COUNTS);
            let bounds = list_values(&histogram, "explicit_bounds");
            let bounds = bounds
                .as_any()
                .downcast_ref::<Float64Array>()
                .expect("explicit bounds are stored as float64");
            assert_eq!(**bounds.values(), HISTOGRAM_EXPLICIT_BOUNDS);

            let exponential = row_by_string(&rows, "metric_name", EXPONENTIAL_HISTOGRAM_METRIC);
            assert_shared_columns(&exponential, time, "exponential_histogram", transport);
            assert_eq!(
                column::<Int64Array>(&exponential, "histogram_count").value(0),
                EXPONENTIAL_COUNT
            );
            for (name, expected) in [
                ("histogram_sum", EXPONENTIAL_SUM),
                ("histogram_min", EXPONENTIAL_MIN),
                ("histogram_max", EXPONENTIAL_MAX),
                ("exponential_zero_threshold", EXPONENTIAL_ZERO_THRESHOLD),
            ] {
                assert!(
                    (column::<Float64Array>(&exponential, name).value(0) - expected).abs()
                        < f64::EPSILON,
                    "{transport} stores `{name}` exactly"
                );
            }
            assert_eq!(
                column::<Int32Array>(&exponential, "exponential_scale").value(0),
                EXPONENTIAL_SCALE
            );
            assert_eq!(
                column::<Int64Array>(&exponential, "exponential_zero_count").value(0),
                EXPONENTIAL_ZERO_COUNT
            );
            for (name, offset, counts) in [
                (
                    "positive_buckets",
                    EXPONENTIAL_POSITIVE_OFFSET,
                    EXPONENTIAL_POSITIVE_COUNTS.to_vec(),
                ),
                (
                    "negative_buckets",
                    EXPONENTIAL_NEGATIVE_OFFSET,
                    EXPONENTIAL_NEGATIVE_COUNTS.to_vec(),
                ),
            ] {
                let side = column::<StructArray>(&exponential, name);
                assert_eq!(
                    child::<Int32Array>(side, "offset").value(0),
                    offset,
                    "{transport} keeps the signed `{name}` offset"
                );
                let stored = child::<ListArray>(side, "bucket_counts").value(0);
                let stored = stored
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .expect("bucket counts are stored as int64");
                assert_eq!(**stored.values(), *counts.as_slice());
            }

            let summary = row_by_string(&rows, "metric_name", SUMMARY_METRIC);
            assert_shared_columns(&summary, time, "summary", transport);
            assert_eq!(
                column::<Int64Array>(&summary, "summary_count").value(0),
                SUMMARY_COUNT
            );
            assert!(
                (column::<Float64Array>(&summary, "summary_sum").value(0) - SUMMARY_SUM).abs()
                    < f64::EPSILON
            );
            let quantiles = list_values(&summary, "quantile_values");
            let quantiles = quantiles
                .as_any()
                .downcast_ref::<StructArray>()
                .expect("quantiles are stored as structs");
            assert_eq!(quantiles.len(), SUMMARY_QUANTILES.len());
            for (index, (quantile, value)) in SUMMARY_QUANTILES.iter().enumerate() {
                assert!(
                    (child::<Float64Array>(quantiles, "quantile").value(index) - quantile).abs()
                        < f64::EPSILON,
                    "{transport} keeps quantile {index} in request order"
                );
                assert!(
                    (child::<Float64Array>(quantiles, "value").value(index) - value).abs()
                        < f64::EPSILON
                );
            }
        }

        journey.shutdown().await;
    }

    /// An ordinary Rust application's OpenTelemetry meter reaches Bifrost.
    ///
    /// The upstream `SdkMeterProvider` and OTLP/gRPC `MetricExporter` are
    /// configured the way an application configures them and record through
    /// the four instruments the pinned SDK exposes directly — a counter, an
    /// up/down counter, a gauge and a histogram. The SDK owns aggregation, so
    /// the readback requires the aggregate it produced rather than the
    /// measurements that went in. The exhaustive per-kind fidelity contract,
    /// including the `Summary` and exponential-histogram shapes no upstream
    /// instrument authors, stays with the raw protocol case above.
    ///
    /// # Panics
    ///
    /// Panics when the exporter cannot be built, the provider does not flush,
    /// or any produced aggregate differs from what canonical SQL returns.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn stock_rust_otel_meter_exports_representative_metrics_to_bifrost() {
        use opentelemetry::KeyValue;
        use opentelemetry::metrics::MeterProvider as _;
        use opentelemetry_otlp::{MetricExporter, WithExportConfig, WithTonicConfig};
        use opentelemetry_sdk::metrics::SdkMeterProvider;

        let journey = OtlpJourney::start().await;
        let exporter = MetricExporter::builder()
            .with_tonic()
            .with_endpoint(journey.grpc_url())
            .with_metadata(journey.stock_metadata())
            .build()
            .expect("the upstream OTLP metric exporter builds against the bound collector");
        let metrics = SdkMeterProvider::builder()
            .with_resource(support::stock_resource())
            .with_periodic_exporter(exporter)
            .build();
        let meter = metrics.meter_with_scope(
            opentelemetry::InstrumentationScope::builder(support::STOCK_METRIC_SCOPE)
                .with_version(support::SCOPE_VERSION)
                .build(),
        );
        let attributes = [KeyValue::new("wyrd.test.marker", "rust-metric")];
        meter
            .u64_counter("orders.created")
            .build()
            .add(7, &attributes);
        meter
            .i64_up_down_counter("orders.active")
            .build()
            .add(-2, &attributes);
        meter
            .f64_gauge("queue.depth")
            .build()
            .record(3.5, &attributes);
        meter
            .f64_histogram("request.duration")
            .with_unit("ms")
            .build()
            .record(12.5, &attributes);
        // Shutdown is the single export boundary here rather than a flush
        // followed by a shutdown: the pinned SDK aggregates cumulatively, so
        // both would export the same running totals and the table would carry
        // two indistinguishable points per instrument.
        metrics
            .shutdown()
            .expect("the upstream meter provider collects, exports and shuts down");

        journey.publish().await;
        let rows = journey
            .query(&format!(
                "SELECT * FROM {METRICS_TABLE} WHERE scope_name = '{}'",
                support::STOCK_METRIC_SCOPE
            ))
            .await;

        for name in [
            "orders.created",
            "orders.active",
            "queue.depth",
            "request.duration",
        ] {
            let row = row_by_string(&rows, "metric_name", name);
            assert_eq!(
                support::decode_attributes(column::<LargeBinaryArray>(&row, "attributes").value(0))
                    .get("wyrd.test.marker")
                    .map(String::as_str),
                Some("rust-metric"),
                "`{name}` keeps the point attribute the application recorded with"
            );
            assert_eq!(
                support::decode_attributes(
                    column::<LargeBinaryArray>(&row, "resource_attributes").value(0)
                )
                .get("service.name")
                .map(String::as_str),
                Some(support::STOCK_SERVICE_NAME)
            );
        }

        let created = row_by_string(&rows, "metric_name", "orders.created");
        assert_eq!(
            column::<StringArray>(&created, "metric_type").value(0),
            "sum"
        );
        assert_eq!(column::<Int64Array>(&created, "int_value").value(0), 7);
        assert!(
            column::<BooleanArray>(&created, "is_monotonic").value(0),
            "a counter aggregates to a monotonic sum"
        );

        let active = row_by_string(&rows, "metric_name", "orders.active");
        assert_eq!(
            column::<StringArray>(&active, "metric_type").value(0),
            "sum"
        );
        assert_eq!(column::<Int64Array>(&active, "int_value").value(0), -2);
        assert!(
            !column::<BooleanArray>(&active, "is_monotonic").value(0),
            "an up/down counter aggregates to a non-monotonic sum"
        );

        let depth = row_by_string(&rows, "metric_name", "queue.depth");
        assert_eq!(
            column::<StringArray>(&depth, "metric_type").value(0),
            "gauge"
        );
        assert!(
            (column::<Float64Array>(&depth, "double_value").value(0) - 3.5).abs() < f64::EPSILON
        );

        let duration = row_by_string(&rows, "metric_name", "request.duration");
        assert_eq!(
            column::<StringArray>(&duration, "metric_type").value(0),
            "histogram"
        );
        assert_eq!(column::<StringArray>(&duration, "unit").value(0), "ms");
        assert_eq!(
            column::<Int64Array>(&duration, "histogram_count").value(0),
            1
        );
        assert!(
            (column::<Float64Array>(&duration, "histogram_sum").value(0) - 12.5).abs()
                < f64::EPSILON
        );
        let bounds = column::<ListArray>(&duration, "explicit_bounds").value(0);
        let counts = column::<ListArray>(&duration, "bucket_counts").value(0);
        assert_eq!(
            counts.len(),
            bounds.len() + 1,
            "the SDK's default bucket layout keeps one more count than bound"
        );
        assert_eq!(
            counts
                .as_any()
                .downcast_ref::<Int64Array>()
                .expect("a bucket count is an int64")
                .iter()
                .flatten()
                .sum::<i64>(),
            1,
            "the one recorded measurement lands in exactly one bucket"
        );

        journey.shutdown().await;
    }
}
