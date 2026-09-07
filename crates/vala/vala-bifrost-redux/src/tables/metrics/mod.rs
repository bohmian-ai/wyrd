//! Canonical metric-signal table definition and its table-owned projection.

pub mod points;
pub mod projection;

pub use points::PointsTable;
pub use projection::{canonical_metric_schema, project_resource_metrics, validate_metric_points};

#[cfg(test)]
mod tests {
    use arrow::array::{
        Array, BinaryArray, BooleanArray, Float64Array, Int32Array, Int64Array, ListArray,
        StringArray, StructArray,
    };
    use arrow::record_batch::RecordBatch;
    use std::sync::Arc;
    use wyrd_tonic::otlp::common::v1::any_value::Value;
    use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue};
    use wyrd_tonic::otlp::metrics::v1::{
        Exemplar, ExponentialHistogram, ExponentialHistogramDataPoint, Gauge, Histogram,
        HistogramDataPoint, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum, Summary,
        SummaryDataPoint, exemplar, exponential_histogram_data_point::Buckets, metric,
        number_data_point, summary_data_point::ValueAtQuantile,
    };

    use super::points::METRIC_FIELDS;
    use super::{project_resource_metrics, validate_metric_points};
    use crate::tables::signal::{encode_attributes, without_correlation_columns};

    /// The double whose exact bits must survive projection unchanged.
    const SIGNALLING_NAN: u64 = 0x7ff8_0000_0000_0001;

    /// Build one attribute entry with the supplied protocol value.
    fn attribute(key: &str, value: Value) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue { value: Some(value) }),
        }
    }

    /// Wrap one metric of the supplied kind into a complete request.
    fn request(metrics: Vec<Metric>) -> Vec<ResourceMetrics> {
        vec![ResourceMetrics {
            resource: None,
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }]
    }

    /// Build one metric of the supplied name around its data collection.
    fn metric(name: &str, data: metric::Data) -> Metric {
        Metric {
            name: name.to_owned(),
            description: "described".to_owned(),
            unit: "1".to_owned(),
            metadata: vec![attribute("owner", Value::StringValue("wyrd".to_owned()))],
            data: Some(data),
        }
    }

    /// Build one numeric point carrying the supplied value alternative.
    fn number_point(value: number_data_point::Value) -> NumberDataPoint {
        NumberDataPoint {
            attributes: vec![attribute("k", Value::IntValue(1))],
            start_time_unix_nano: 10,
            time_unix_nano: 20,
            exemplars: vec![Exemplar {
                filtered_attributes: vec![attribute("e", Value::BoolValue(true))],
                time_unix_nano: 15,
                span_id: vec![0x22; 8],
                trace_id: vec![0x11; 16],
                value: Some(exemplar::Value::AsInt(-7)),
            }],
            flags: 1,
            value: Some(value),
        }
    }

    /// Borrow one named column, panicking when it is absent.
    fn column<'a>(batch: &'a RecordBatch, name: &str) -> &'a dyn Array {
        batch
            .column_by_name(name)
            .unwrap_or_else(|| panic!("canonical point batch has column {name}"))
            .as_ref()
    }

    /// Downcast one named column, panicking when its Arrow type differs.
    fn typed<'a, T: 'static>(batch: &'a RecordBatch, name: &str) -> &'a T {
        column(batch, name)
            .as_any()
            .downcast_ref::<T>()
            .unwrap_or_else(|| panic!("column {name} has the declared Arrow type"))
    }

    /// Read one nested `UInt64` list row as a plain vector.
    fn u64_list(batch: &RecordBatch, name: &str, row: usize) -> Vec<i64> {
        let values = typed::<ListArray>(batch, name).value(row);
        let values = values
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("nested unsigned list");
        values.iter().map(|value| value.expect("no null")).collect()
    }

    /// Read one nested `Float64` list row as a plain vector.
    fn f64_list(batch: &RecordBatch, name: &str, row: usize) -> Vec<f64> {
        let values = typed::<ListArray>(batch, name).value(row);
        let values = values
            .as_any()
            .downcast_ref::<Float64Array>()
            .expect("nested double list");
        values.iter().map(|value| value.expect("no null")).collect()
    }

    /// One point of every pinned kind, each carrying a maximal value shape.
    fn all_kind_request() -> Vec<ResourceMetrics> {
        request(vec![
            metric(
                "gauge.int",
                metric::Data::Gauge(Gauge {
                    data_points: vec![number_point(number_data_point::Value::AsInt(i64::MIN))],
                }),
            ),
            metric(
                "sum.double",
                metric::Data::Sum(Sum {
                    data_points: vec![number_point(number_data_point::Value::AsDouble(
                        f64::from_bits(SIGNALLING_NAN),
                    ))],
                    aggregation_temporality: 2,
                    is_monotonic: true,
                }),
            ),
            metric(
                "histogram",
                metric::Data::Histogram(Histogram {
                    data_points: vec![HistogramDataPoint {
                        attributes: Vec::new(),
                        start_time_unix_nano: 30,
                        time_unix_nano: 40,
                        count: 6,
                        sum: Some(1.5),
                        bucket_counts: vec![1, 2, 3],
                        explicit_bounds: vec![0.5, 1.5],
                        exemplars: Vec::new(),
                        flags: 0,
                        min: Some(f64::NEG_INFINITY),
                        max: Some(f64::INFINITY),
                    }],
                    aggregation_temporality: 1,
                }),
            ),
            metric(
                "exponential",
                metric::Data::ExponentialHistogram(ExponentialHistogram {
                    data_points: vec![ExponentialHistogramDataPoint {
                        attributes: Vec::new(),
                        start_time_unix_nano: 50,
                        time_unix_nano: 60,
                        count: 9,
                        sum: None,
                        scale: -3,
                        zero_count: 4,
                        positive: Some(Buckets {
                            offset: -2,
                            bucket_counts: vec![5, 6],
                        }),
                        negative: None,
                        flags: 0,
                        exemplars: Vec::new(),
                        min: None,
                        max: None,
                        zero_threshold: 0.25,
                    }],
                    aggregation_temporality: 2,
                }),
            ),
            metric(
                "summary",
                metric::Data::Summary(Summary {
                    data_points: vec![SummaryDataPoint {
                        attributes: Vec::new(),
                        start_time_unix_nano: 70,
                        time_unix_nano: 80,
                        count: 11,
                        sum: 2.5,
                        quantile_values: vec![
                            ValueAtQuantile {
                                quantile: 0.0,
                                value: -1.0,
                            },
                            ValueAtQuantile {
                                quantile: 1.0,
                                value: 9.0,
                            },
                        ],
                        flags: 0,
                    }],
                }),
            ),
        ])
    }

    /// The five pinned kinds project one row each without narrowing a shape.
    ///
    /// # Panics
    ///
    /// Panics when a projected value, nested collection, kind discriminant, or
    /// per-kind null shape differs from the supplied point.
    #[test]
    fn all_pinned_metric_point_kinds_project_without_narrowing() {
        let requested = all_kind_request();
        let (batch, outcome) =
            project_resource_metrics(&requested).expect("every pinned kind projects");
        assert_eq!(outcome.accepted_points, 5);
        assert_eq!(outcome.rejected_points, 0);
        assert!(outcome.rejection_message.is_none());
        assert_eq!(batch.num_rows(), 5);
        assert_eq!(
            batch.num_columns(),
            METRIC_FIELDS.len() + 2,
            "the ledger columns plus the two appended correlation columns"
        );

        let kinds = typed::<StringArray>(&batch, "metric_type");
        assert_eq!(kinds.value(0), "gauge");
        assert_eq!(kinds.value(1), "sum");
        assert_eq!(kinds.value(2), "histogram");
        assert_eq!(kinds.value(3), "exponential_histogram");
        assert_eq!(kinds.value(4), "summary");

        let ints = typed::<Int64Array>(&batch, "int_value");
        assert_eq!(ints.value(0), i64::MIN);
        assert!(ints.is_null(1), "a double point owns no integer value");
        let doubles = typed::<Float64Array>(&batch, "double_value");
        assert!(doubles.is_null(0), "an integer point owns no double value");
        assert_eq!(
            doubles.value(1).to_bits(),
            SIGNALLING_NAN,
            "the exact IEEE bits survive"
        );
        assert!(typed::<BooleanArray>(&batch, "is_monotonic").value(1));
        assert_eq!(
            typed::<Int32Array>(&batch, "aggregation_temporality").value(1),
            2
        );

        assert_eq!(typed::<Int64Array>(&batch, "histogram_count").value(2), 6);
        assert_eq!(u64_list(&batch, "bucket_counts", 2), vec![1, 2, 3]);
        assert_eq!(f64_list(&batch, "explicit_bounds", 2), vec![0.5, 1.5]);
        assert_eq!(
            typed::<Float64Array>(&batch, "histogram_min")
                .value(2)
                .to_bits(),
            f64::NEG_INFINITY.to_bits()
        );
        assert_eq!(
            typed::<Float64Array>(&batch, "histogram_max")
                .value(2)
                .to_bits(),
            f64::INFINITY.to_bits()
        );

        assert_eq!(
            typed::<Int32Array>(&batch, "exponential_scale").value(3),
            -3
        );
        assert_eq!(
            typed::<Int64Array>(&batch, "exponential_zero_count").value(3),
            4
        );
        let positive = typed::<StructArray>(&batch, "positive_buckets");
        assert!(positive.is_valid(3));
        assert!(
            !positive.is_valid(0),
            "another kind owns no positive bucket"
        );
        let offsets = positive
            .column_by_name("offset")
            .and_then(|column| column.as_any().downcast_ref::<Int32Array>())
            .expect("bucket offset column");
        assert_eq!(offsets.value(3), -2);
        let negative = typed::<StructArray>(&batch, "negative_buckets");
        assert!(!negative.is_valid(3), "an absent bucket collection is null");

        assert_eq!(typed::<Int64Array>(&batch, "summary_count").value(4), 11);
        let quantiles = typed::<ListArray>(&batch, "quantile_values").value(4);
        let quantiles = quantiles
            .as_any()
            .downcast_ref::<StructArray>()
            .expect("quantiles are structs");
        assert_eq!(quantiles.len(), 2);

        let exemplars = typed::<ListArray>(&batch, "exemplars");
        assert_eq!(exemplars.value_length(0), 1);
        assert_eq!(exemplars.value_length(2), 0);

        let ledger = without_correlation_columns(&batch)
            .expect("the appended correlation columns split off cleanly");
        validate_metric_points(&ledger).expect("the projected batch validates its own kind shapes");
    }

    /// Inconsistent point shapes are rejected one record at a time.
    ///
    /// # Panics
    ///
    /// Panics when an invalid point is accepted, when a valid point in the same
    /// request is lost, or when a supplied batch that populates a foreign
    /// kind's column validates.
    #[test]
    fn inconsistent_metric_shapes_are_record_rejections() {
        let requested = request(vec![
            metric(
                "gauge.valid",
                metric::Data::Gauge(Gauge {
                    data_points: vec![
                        number_point(number_data_point::Value::AsInt(1)),
                        NumberDataPoint {
                            value: None,
                            ..number_point(number_data_point::Value::AsInt(2))
                        },
                    ],
                }),
            ),
            Metric {
                name: "no.data".to_owned(),
                description: String::new(),
                unit: String::new(),
                metadata: Vec::new(),
                data: None,
            },
            metric(
                "histogram.mismatched",
                metric::Data::Histogram(Histogram {
                    data_points: vec![HistogramDataPoint {
                        attributes: Vec::new(),
                        start_time_unix_nano: 0,
                        time_unix_nano: 1,
                        count: 6,
                        sum: None,
                        bucket_counts: vec![1, 2, 3],
                        explicit_bounds: vec![0.5],
                        exemplars: Vec::new(),
                        flags: 0,
                        min: None,
                        max: None,
                    }],
                    aggregation_temporality: 1,
                }),
            ),
            metric(
                "summary.out.of.range",
                metric::Data::Summary(Summary {
                    data_points: vec![SummaryDataPoint {
                        attributes: Vec::new(),
                        start_time_unix_nano: 0,
                        time_unix_nano: 1,
                        count: 1,
                        sum: 1.0,
                        quantile_values: vec![ValueAtQuantile {
                            quantile: 1.5,
                            value: 0.0,
                        }],
                        flags: 0,
                    }],
                }),
            ),
        ]);

        let (batch, outcome) =
            project_resource_metrics(&requested).expect("the valid point still projects");
        assert_eq!(outcome.accepted_points, 1);
        assert_eq!(outcome.rejected_points, 4);
        assert_eq!(
            outcome.rejection_message.as_deref(),
            Some("numeric data point carries no value")
        );
        assert_eq!(batch.num_rows(), 1);
        assert_eq!(
            typed::<StringArray>(&batch, "metric_name").value(0),
            "gauge.valid"
        );

        let foreign: Vec<_> = batch
            .schema()
            .fields()
            .iter()
            .zip(batch.columns())
            .map(|(field, values)| {
                if field.name() == "summary_count" {
                    Arc::new(Int64Array::from(vec![Some(3i64)])) as Arc<dyn Array>
                } else {
                    Arc::clone(values)
                }
            })
            .collect();
        let foreign = RecordBatch::try_new(batch.schema(), foreign)
            .expect("the foreign-shape batch still assembles");
        assert!(
            validate_metric_points(&foreign).is_err(),
            "a gauge row may not populate a summary column"
        );
    }

    /// Optional Card correlation is read from the final point attribute only,
    /// rejects exactly its own point, and never disturbs the lossless payload.
    ///
    /// # Panics
    ///
    /// Panics when a missing value is not null, a final duplicate does not win,
    /// a wrongly typed or malformed value rejects more than its own point, an
    /// original attribute entry changes, or the outcome counts and first reason
    /// are not exact.
    #[test]
    fn optional_card_correlation_is_atomic_and_lossless() {
        const CARD: &str = "prod/Service/checkout@1.0.0";
        const RUN: &str = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b11";

        let base = || vec![attribute("k", Value::IntValue(1))];
        let missing = base();
        let mut valid = base();
        valid.push(attribute(
            "wyrd.card_ref",
            Value::StringValue(CARD.to_owned()),
        ));
        valid.push(attribute("wyrd.run_id", Value::StringValue(RUN.to_owned())));
        let mut duplicate = base();
        duplicate.push(attribute(
            "wyrd.card_ref",
            Value::StringValue("prod/Service/superseded@9.9.9".to_owned()),
        ));
        duplicate.push(attribute(
            "wyrd.card_ref",
            Value::StringValue(CARD.to_owned()),
        ));
        let mut wrong_typed = base();
        wrong_typed.push(attribute("wyrd.run_id", Value::IntValue(7)));
        let mut malformed = base();
        malformed.push(attribute(
            "wyrd.card_ref",
            Value::StringValue("not-a-card-ref".to_owned()),
        ));

        let sets = vec![
            missing.clone(),
            valid.clone(),
            duplicate.clone(),
            wrong_typed,
            malformed,
        ];
        let requested = request(vec![metric(
            "gauge.correlated",
            metric::Data::Gauge(Gauge {
                data_points: sets
                    .into_iter()
                    .map(|attributes| NumberDataPoint {
                        attributes,
                        ..number_point(number_data_point::Value::AsInt(1))
                    })
                    .collect(),
            }),
        )]);
        let (batch, outcome) = project_resource_metrics(&requested).expect("projection completes");

        assert_eq!(outcome.accepted_points, 3);
        assert_eq!(outcome.rejected_points, 2);
        assert_eq!(
            outcome.rejection_message.as_deref(),
            Some("wyrd.run_id is not a valid run correlation"),
            "the first rejection in traversal order is reported"
        );
        assert_eq!(
            batch.num_rows(),
            3,
            "only the two defective points are lost"
        );

        let card_refs = typed::<StringArray>(&batch, "card_ref");
        assert!(
            card_refs.is_null(0),
            "a missing wyrd.card_ref projects null"
        );
        assert_eq!(card_refs.value(1), CARD);
        assert_eq!(
            card_refs.value(2),
            CARD,
            "the final duplicate is authoritative"
        );
        let run_ids = typed::<StringArray>(&batch, "run_id");
        assert!(run_ids.is_null(0), "a missing wyrd.run_id projects null");
        assert_eq!(run_ids.value(1), RUN);
        assert!(run_ids.is_null(2));

        let attributes = typed::<BinaryArray>(&batch, "attributes");
        for (row, source) in [missing, valid, duplicate].iter().enumerate() {
            assert_eq!(
                attributes.value(row),
                encode_attributes(source).as_slice(),
                "row {row} retains every ordered source attribute byte for byte"
            );
        }
    }
}
