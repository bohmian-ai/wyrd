//! Authenticated `DataFusion` physical-plan extension codec for Oracle followers.

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex};

use arrow::datatypes::SchemaRef;
use datafusion::common::{DataFusionError, Result};
use datafusion::execution::TaskContext;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, PlanProperties, SendableRecordBatchStream,
};
use datafusion_proto::physical_plan::PhysicalExtensionCodec;
use prost::Message;
use sha2::{Digest as _, Sha256};

/// Version of the private Oracle physical extension envelope.
pub const ORACLE_PHYSICAL_CODEC_VERSION: u32 = 1;
/// Only extension type accepted by the Oracle follower.
pub const ORACLE_REMOTE_SCAN_TAG: &str = "wyrd.oracle.remote_scan";

/// Fingerprints the exact versioned physical-plan bytes shared by all followers.
#[must_use]
pub fn physical_plan_fingerprint(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"wyrd.oracle.physical-plan.v1\0");
    digest.update(ORACLE_PHYSICAL_CODEC_VERSION.to_be_bytes());
    digest.update(bytes);
    format!("sha256:{:x}", digest.finalize())
}

/// Versioned extension envelope stored in a `DataFusion` extension node.
#[derive(Clone, PartialEq, Message)]
struct OracleExtensionEnvelope {
    /// Private codec version.
    #[prost(uint32, tag = "1")]
    codec_version: u32,
    /// Stable extension type identifier.
    #[prost(string, tag = "2")]
    type_tag: String,
    /// Type-specific prost payload.
    #[prost(bytes, tag = "3")]
    payload: Vec<u8>,
}

/// Identity payload for one remote scan placeholder.
#[derive(Clone, PartialEq, Message)]
pub(crate) struct RemoteScanPayload {
    /// Stable request-local scan identifier.
    #[prost(string, tag = "1")]
    pub(crate) scan_id: String,
    /// Fingerprint of the expected provider schema.
    #[prost(string, tag = "2")]
    pub(crate) schema_fingerprint: String,
}

/// Leaf placeholder substituted for one Wyrd-owned source before serialization.
#[derive(Debug)]
pub struct RemoteScanExec {
    /// Stable request-local identity.
    scan_id: String,
    /// Expected provider schema fingerprint.
    schema_fingerprint: String,
    /// Empty executable carrying the placeholder schema and plan properties.
    empty: datafusion::physical_plan::empty::EmptyExec,
}

impl RemoteScanExec {
    /// Creates one remote source placeholder.
    #[must_use]
    pub fn new(
        scan_id: impl Into<String>,
        schema_fingerprint: impl Into<String>,
        schema: SchemaRef,
    ) -> Self {
        Self {
            scan_id: scan_id.into(),
            schema_fingerprint: schema_fingerprint.into(),
            empty: datafusion::physical_plan::empty::EmptyExec::new(schema),
        }
    }

    /// Returns the stable scan identity.
    #[must_use]
    pub fn scan_id(&self) -> &str {
        &self.scan_id
    }
}

impl DisplayAs for RemoteScanExec {
    /// Formats only non-secret placeholder identity.
    ///
    /// # Errors
    /// Returns the formatter error if the destination cannot accept the identity.
    fn fmt_as(
        &self,
        _display: DisplayFormatType,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(formatter, "RemoteScanExec: {}", self.scan_id)
    }
}

impl ExecutionPlan for RemoteScanExec {
    /// Returns the stable diagnostic operator name.
    fn name(&self) -> &'static str {
        "RemoteScanExec"
    }
    /// Enables extension-codec downcasting.
    fn as_any(&self) -> &dyn Any {
        self
    }
    /// Returns the placeholder's cached physical properties.
    fn properties(&self) -> &Arc<PlanProperties> {
        self.empty.properties()
    }
    /// A remote scan is always a leaf.
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        Vec::new()
    }
    /// Rejects children and preserves the immutable leaf.
    ///
    /// # Errors
    /// Returns a plan error when a caller attempts to attach any child.
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        if children.is_empty() {
            Ok(self)
        } else {
            Err(DataFusionError::Plan(
                "remote scan must remain a leaf".to_owned(),
            ))
        }
    }
    /// Executes as empty only before role-local follower reconstruction.
    ///
    /// # Errors
    /// Returns the wrapped empty plan's execution error for an invalid partition
    /// or task context.
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream> {
        self.empty.execute(partition, context)
    }
}

/// Physical extension codec that encodes placeholders and consumes prebuilt providers.
#[derive(Debug, Default)]
pub struct OraclePhysicalExtensionCodec {
    /// Complete role-local provider registry, consumed once per scan.
    providers: Mutex<HashMap<String, Arc<dyn ExecutionPlan>>>,
}

impl OraclePhysicalExtensionCodec {
    /// Creates an encoding-only codec.
    #[must_use]
    pub fn encoder() -> Self {
        Self::default()
    }

    /// Creates a decoding codec with the complete validated provider registry.
    #[must_use]
    pub fn decoder(providers: HashMap<String, Arc<dyn ExecutionPlan>>) -> Self {
        Self {
            providers: Mutex::new(providers),
        }
    }

    /// Requires the native decoder to consume every authenticated provider exactly once.
    ///
    /// # Errors
    /// Returns a plan error when the decoded tree did not reference every preflighted scan.
    pub fn require_complete_consumption(&self) -> Result<()> {
        let providers = self
            .providers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if providers.is_empty() {
            Ok(())
        } else {
            Err(DataFusionError::Plan(format!(
                "decoded physical plan left {} authenticated provider(s) unused",
                providers.len()
            )))
        }
    }

    /// Validates one envelope without constructing or resolving providers.
    ///
    /// # Errors
    /// Returns a plan error for malformed, unsupported, empty, or duplicate identities.
    pub fn preflight(envelopes: &[Vec<u8>]) -> Result<Vec<(String, String)>> {
        let mut seen = HashSet::new();
        envelopes
            .iter()
            .map(|bytes| {
                let payload = Self::decode_payload(bytes)?;
                if payload.scan_id.is_empty()
                    || payload.schema_fingerprint.is_empty()
                    || !seen.insert(payload.scan_id.clone())
                {
                    return Err(DataFusionError::Plan(
                        "invalid or duplicate remote scan identity".to_owned(),
                    ));
                }
                Ok((payload.scan_id, payload.schema_fingerprint))
            })
            .collect()
    }

    /// Decodes and validates the fixed extension envelope.
    ///
    /// # Errors
    /// Returns a plan error when the envelope or payload is malformed or unsupported.
    pub(crate) fn decode_payload(bytes: &[u8]) -> Result<RemoteScanPayload> {
        let envelope = OracleExtensionEnvelope::decode(bytes).map_err(|error| {
            DataFusionError::Plan(format!("invalid Oracle extension envelope: {error}"))
        })?;
        if envelope.codec_version != ORACLE_PHYSICAL_CODEC_VERSION
            || envelope.type_tag != ORACLE_REMOTE_SCAN_TAG
        {
            return Err(DataFusionError::Plan(
                "unsupported Oracle extension version or tag".to_owned(),
            ));
        }
        RemoteScanPayload::decode(envelope.payload.as_slice())
            .map_err(|error| DataFusionError::Plan(format!("invalid remote scan payload: {error}")))
    }
}

impl PhysicalExtensionCodec for OraclePhysicalExtensionCodec {
    /// Reconstructs only a pre-resolved role-local provider for the authenticated scan.
    ///
    /// # Errors
    /// Returns a plan error for a non-leaf envelope, malformed payload, or absent
    /// authenticated provider.
    fn try_decode(
        &self,
        buf: &[u8],
        inputs: &[Arc<dyn ExecutionPlan>],
        _ctx: &TaskContext,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        if !inputs.is_empty() {
            return Err(DataFusionError::Plan(
                "remote scan extension must be a leaf".to_owned(),
            ));
        }
        let payload = Self::decode_payload(buf)?;
        self.providers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&payload.scan_id)
            .ok_or_else(|| {
                DataFusionError::Plan(format!(
                    "missing authenticated provider for scan {}",
                    payload.scan_id
                ))
            })
    }

    /// Encodes only the Wyrd remote-scan placeholder in the fixed v1 envelope.
    ///
    /// # Errors
    /// Returns a plan error for any other extension type or protobuf failure.
    fn try_encode(&self, node: Arc<dyn ExecutionPlan>, buf: &mut Vec<u8>) -> Result<()> {
        let scan = node
            .as_any()
            .downcast_ref::<RemoteScanExec>()
            .ok_or_else(|| {
                DataFusionError::Plan("unsupported Oracle physical extension".to_owned())
            })?;
        let payload = RemoteScanPayload {
            scan_id: scan.scan_id.clone(),
            schema_fingerprint: scan.schema_fingerprint.clone(),
        }
        .encode_to_vec();
        OracleExtensionEnvelope {
            codec_version: ORACLE_PHYSICAL_CODEC_VERSION,
            type_tag: ORACLE_REMOTE_SCAN_TAG.to_owned(),
            payload,
        }
        .encode(buf)
        .map_err(|error| {
            DataFusionError::Plan(format!("Oracle extension encoding failed: {error}"))
        })
    }
}

#[cfg(test)]
mod tests {
    //! Behavioral proof for physical-plan encoding and provider ownership.
    use super::*;
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use datafusion::datasource::MemTable;
    use datafusion::execution::context::SessionContext;
    use datafusion::physical_plan::{collect, displayable};
    use datafusion_proto::bytes::{
        physical_plan_from_bytes_with_extension_codec, physical_plan_to_bytes_with_extension_codec,
    };
    use datafusion_proto::protobuf::{
        AggregateExecNode, PhysicalExtensionNode, PhysicalPlanNode,
        physical_plan_node::PhysicalPlanType,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Provider wrapper whose destruction proves failed decode releases acquired state.
    #[derive(Debug)]
    struct DropObservedExec {
        /// Executable empty provider delegated by this wrapper.
        empty: datafusion::physical_plan::empty::EmptyExec,
        /// Number of wrapper drops.
        drops: Arc<AtomicUsize>,
        /// Number of physical execution attempts.
        executions: Arc<AtomicUsize>,
    }

    impl DropObservedExec {
        /// Creates a provider with one observable ownership token.
        #[must_use]
        fn new(schema: SchemaRef, drops: Arc<AtomicUsize>, executions: Arc<AtomicUsize>) -> Self {
            Self {
                empty: datafusion::physical_plan::empty::EmptyExec::new(schema),
                drops,
                executions,
            }
        }
    }

    impl Drop for DropObservedExec {
        /// Records release of the acquired provider.
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl DisplayAs for DropObservedExec {
        /// Formats the non-secret test provider identity.
        ///
        /// # Errors
        /// Returns the formatter error if the destination cannot accept the identity.
        fn fmt_as(
            &self,
            _display: DisplayFormatType,
            formatter: &mut fmt::Formatter<'_>,
        ) -> fmt::Result {
            formatter.write_str("DropObservedExec")
        }
    }

    impl ExecutionPlan for DropObservedExec {
        /// Returns the stable test operator name.
        fn name(&self) -> &'static str {
            "DropObservedExec"
        }
        /// Enables downcasting.
        fn as_any(&self) -> &dyn Any {
            self
        }
        /// Delegates cached properties.
        fn properties(&self) -> &Arc<PlanProperties> {
            self.empty.properties()
        }
        /// This provider is a leaf.
        fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
            Vec::new()
        }
        /// Preserves the leaf only when no children are supplied.
        ///
        /// # Errors
        /// Returns a plan error when a caller supplies a child.
        fn with_new_children(
            self: Arc<Self>,
            children: Vec<Arc<dyn ExecutionPlan>>,
        ) -> Result<Arc<dyn ExecutionPlan>> {
            if children.is_empty() {
                Ok(self)
            } else {
                Err(DataFusionError::Plan(
                    "drop-observed provider must remain a leaf".to_owned(),
                ))
            }
        }
        /// Delegates physical execution to the empty provider.
        ///
        /// # Errors
        /// Returns the delegated DataFusion execution error.
        fn execute(
            &self,
            partition: usize,
            context: Arc<TaskContext>,
        ) -> Result<SendableRecordBatchStream> {
            self.executions.fetch_add(1, Ordering::SeqCst);
            self.empty.execute(partition, context)
        }
    }

    /// Encodes one canonical extension envelope for focused codec tests.
    fn envelope(scan_id: &str) -> Vec<u8> {
        OracleExtensionEnvelope {
            codec_version: ORACLE_PHYSICAL_CODEC_VERSION,
            type_tag: ORACLE_REMOTE_SCAN_TAG.to_owned(),
            payload: RemoteScanPayload {
                scan_id: scan_id.to_owned(),
                schema_fingerprint: "sha256:schema".to_owned(),
            }
            .encode_to_vec(),
        }
        .encode_to_vec()
    }

    /// Replaces the single physical source leaf with an Oracle extension placeholder.
    ///
    /// # Errors
    /// Returns a plan error if the native tree cannot accept reconstructed children.
    fn replace_source(
        plan: Arc<dyn ExecutionPlan>,
        source: &mut Option<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        let children = plan.children();
        if children.is_empty() {
            if source.is_some() {
                return Err(DataFusionError::Plan(
                    "test aggregate tree unexpectedly has multiple source leaves".to_owned(),
                ));
            }
            *source = Some(Arc::clone(&plan));
            return Ok(Arc::new(RemoteScanExec::new(
                "scan",
                super::super::sealed_fragment_schema_fingerprint(plan.schema().as_ref()),
                plan.schema(),
            )));
        }
        let replaced = children
            .into_iter()
            .map(|child| replace_source(Arc::clone(child), source))
            .collect::<Result<Vec<_>>>()?;
        plan.with_new_children(replaced)
    }

    /// A real native aggregate/sort/limit tree round-trips and executes identically.
    ///
    /// # Panics
    /// Panics if fixture construction, physical planning, codec round-trip, or
    /// execution violates its test invariant.
    #[tokio::test]
    async fn physical_plan_round_trip_preserves_aggregate_sort_and_limit() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("group_name", DataType::Utf8, false),
            Field::new("value", DataType::Int64, false),
        ]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(StringArray::from(vec!["a", "a", "b"])),
                Arc::new(Int64Array::from(vec![2, 5, 3])),
            ],
        )
        .expect("valid aggregate input");
        let context = SessionContext::new();
        context
            .register_table(
                "source",
                Arc::new(MemTable::try_new(schema, vec![vec![batch]]).expect("valid table")),
            )
            .expect("table registration succeeds");
        let native = context
            .sql("SELECT group_name, SUM(value) AS total FROM source GROUP BY group_name ORDER BY total DESC LIMIT 1")
            .await
            .expect("valid aggregate SQL")
            .create_physical_plan()
            .await
            .expect("physical planning succeeds");
        let mut source = None;
        let outbound = replace_source(native, &mut source).expect("source replacement succeeds");
        let display = displayable(outbound.as_ref()).indent(true).to_string();
        assert!(display.contains("AggregateExec"));
        assert!(display.contains("SortExec"));
        assert!(
            display.contains("GlobalLimitExec") || display.contains("fetch=1"),
            "limit is absent from {display}"
        );
        assert!(display.contains("RemoteScanExec"));
        let bytes = physical_plan_to_bytes_with_extension_codec(
            outbound,
            &OraclePhysicalExtensionCodec::encoder(),
        )
        .expect("native plan encodes");
        let providers = HashMap::from([(
            "scan".to_owned(),
            source.expect("one source leaf was captured"),
        )]);
        let codec = OraclePhysicalExtensionCodec::decoder(providers);
        let task = context.task_ctx();
        let decoded = physical_plan_from_bytes_with_extension_codec(&bytes, &task, &codec)
            .expect("native plan decodes");
        codec
            .require_complete_consumption()
            .expect("provider is consumed once");
        let rows = collect(decoded, task).await.expect("decoded plan executes");
        assert_eq!(rows.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
        let totals = rows[0]
            .column(1)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("aggregate total is int64");
        assert_eq!(totals.value(0), 7);
    }

    /// Every malformed tree or request-component contradiction fails before effects.
    ///
    /// # Panics
    /// Panics if malformed envelopes or any authenticated follower contradiction
    /// reaches resolver, decode, execution, or output effects.
    #[tokio::test]
    async fn preflight_rejects_malformed_tree_before_resolver_io() {
        assert!(OraclePhysicalExtensionCodec::preflight(&[vec![0]]).is_err());
        assert!(
            OraclePhysicalExtensionCodec::preflight(&[envelope("scan"), envelope("scan")]).is_err()
        );
        super::super::follower::tests::assert_complete_preflight_matrix().await;
    }

    /// A post-resolution native decode failure drops providers before execution/output.
    ///
    /// # Panics
    /// Panics if provider ownership or effect counters violate the fixture invariant.
    #[test]
    fn post_resolution_decode_failure_prevents_execution_and_output() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            true,
        )]));
        let drops = Arc::new(AtomicUsize::new(0));
        let executions = Arc::new(AtomicUsize::new(0));
        let provider: Arc<dyn ExecutionPlan> = Arc::new(DropObservedExec::new(
            schema,
            Arc::clone(&drops),
            Arc::clone(&executions),
        ));
        let codec =
            OraclePhysicalExtensionCodec::decoder(HashMap::from([("scan".to_owned(), provider)]));
        let malformed = PhysicalPlanNode {
            physical_plan_type: Some(PhysicalPlanType::Aggregate(Box::new(AggregateExecNode {
                mode: 999,
                input: Some(Box::new(PhysicalPlanNode {
                    physical_plan_type: Some(PhysicalPlanType::Extension(PhysicalExtensionNode {
                        node: envelope("scan"),
                        inputs: Vec::new(),
                    })),
                })),
                ..Default::default()
            }))),
        }
        .encode_to_vec();
        let decoded = physical_plan_from_bytes_with_extension_codec(
            &malformed,
            &TaskContext::default(),
            &codec,
        );
        assert!(decoded.is_err());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert_eq!(executions.load(Ordering::SeqCst), 0);
    }
}
