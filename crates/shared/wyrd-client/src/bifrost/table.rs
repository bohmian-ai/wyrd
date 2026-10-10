//! The table a Bifrost writer binds to, and the optional correlation one row
//! carries.
//!
//! [`TableConfig`] is the single client-side description of one Bifrost table:
//! its `<namespace>.<name>` identity, the user columns the caller declares, and
//! the physical layout it asks the server to create. Every language SDK builds
//! one of these and hands it to [`crate::bifrost::Bifrost`]; none of them re-derive the
//! schema mapping, which belongs to `wyrd-queue`, or the table identity, which
//! belongs to the server.

use std::sync::Arc;

use crate::WyrdClient;
use arrow_schema::SchemaRef;
use serde::{Deserialize, Serialize};
use wyrd_spec::ids::CardUid;
use wyrd_spec::vala::api::{
    BifrostTableDescription, CompactionTypeWire, FieldSpec, PhysicalLayoutWire,
    RegisterTableRequest, RegisterTableResponse,
};
use wyrd_spec::vala::ids::RunId;

use crate::bifrost::query::BifrostClientError;

/// The server-authoritative identity of a registered table.
///
/// Minted by the server on register and echoed by describe. The client never
/// computes either half: a client-side fingerprint would be a second authority
/// for one identity and would drift the moment the schema mapping changes on
/// either side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedTable {
    /// Lower-case hex of the 16-byte table uid.
    pub table_uid: String,
    /// Lower-case hex of the 32-byte user-schema fingerprint.
    pub fingerprint: String,
}

/// One Bifrost table a writer is bound to.
///
/// A config is inert until [`crate::bifrost::Bifrost::register`] or
/// [`TableConfig::describe`] resolves it against the server; until then
/// [`TableConfig::resolved`] is `None`.
///
/// The declared schema holds **user columns only**. Correlation inputs
/// (`card_uid`, `run_id`) and managed columns (`wyrd_*`) are appended by the
/// producer and the server respectively, so declaring one here is a reserved
/// name error rather than a way to control it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(into = "TableConfigWire", try_from = "TableConfigWire")]
pub struct TableConfig {
    /// Target namespace, the half of the FQN before the final dot.
    namespace: String,
    /// Table name, the half of the FQN after the final dot.
    name: String,
    /// The user columns this config declares, in declaration order.
    user_schema: SchemaRef,
    /// The layout to request at register time; `None` takes the server default.
    physical_layout: Option<PhysicalLayoutWire>,
    /// Explicit Forge compaction file target; `None` follows the deployment
    /// default.
    compaction_target_file_size_bytes: Option<u64>,
    /// Explicit Forge compaction type; `None` compacts with the `small-files`
    /// default.
    compaction_type: Option<CompactionTypeWire>,
    /// Server-assigned once registered or described; `None` while inert.
    resolved: Option<ResolvedTable>,
}

impl TableConfig {
    /// Build a config from an explicit Arrow schema.
    ///
    /// The precision path: a caller who needs `Int32`, a non-UTC timezone,
    /// `Decimal128`, or `FixedSizeBinary` supplies Arrow directly, exactly as
    /// [`wyrd_queue::arrow_schema_to_fieldspec`] documents.
    ///
    /// # Arguments
    /// * `fqn` - The table's `<namespace>.<name>`.
    /// * `schema` - The user columns, excluding reserved and correlation columns.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostClientError::Queue`] with code `WYRD_VALA_400_SCHEMA_PARSE`
    /// when `fqn` is not `<namespace>.<name>`, and
    /// `WYRD_VALA_400_BIFROST_RESERVED_COLUMN` when a column uses a reserved
    /// `wyrd_*`, `card_uid`, or `run_id` name.
    pub fn from_arrow(fqn: &str, schema: SchemaRef) -> Result<Self, BifrostClientError> {
        let (namespace, name) = split_fqn(fqn)?;
        reject_reserved_columns(&schema)?;
        Ok(Self {
            namespace,
            name,
            user_schema: schema,
            physical_layout: None,
            compaction_target_file_size_bytes: None,
            compaction_type: None,
            resolved: None,
        })
    }

    /// Build a config from a JSON Schema document.
    ///
    /// This is the Pydantic (`model_json_schema()`) and Zod
    /// (`z.toJSONSchema()`) door. Mapping is delegated verbatim to
    /// [`wyrd_queue::json_schema_to_arrow`], the single owner of the
    /// JSON-Schema-to-Arrow table, so all three languages agree on the columns
    /// one model produces.
    ///
    /// # Arguments
    /// * `fqn` - The table's `<namespace>.<name>`.
    /// * `schema` - A JSON Schema object document describing the user columns.
    ///
    /// # Errors
    ///
    /// As [`TableConfig::from_arrow`], plus a schema-parse failure for a
    /// free-form object, an untyped array, an unsupported type, or an
    /// unresolvable `$ref`.
    pub fn from_json_schema(
        fqn: &str,
        schema: &serde_json::Value,
    ) -> Result<Self, BifrostClientError> {
        let arrow = wyrd_queue::json_schema_to_arrow(schema)?;
        Self::from_arrow(fqn, Arc::new(arrow))
    }

    /// Build a config from a Rust type that derives [`schemars::JsonSchema`].
    ///
    /// The Rust twin of the Pydantic and Zod doors: `from_model::<Prediction>`
    /// is [`TableConfig::from_json_schema`] over `schemars::schema_for!(T)`, so
    /// a Rust struct, a Pydantic model, and a Zod object that describe the same
    /// columns reach the same Arrow schema through the same single mapper.
    ///
    /// The declared type must be a struct of supported scalar, list, or nested
    /// struct fields; `schemars` emits a `$ref` for a nested type, which the
    /// shared mapper resolves only from a `$defs` section.
    ///
    /// # Arguments
    /// * `fqn` - The table's `<namespace>.<name>`.
    ///
    /// # Errors
    ///
    /// As [`TableConfig::from_json_schema`].
    pub fn from_model<T: schemars::JsonSchema>(fqn: &str) -> Result<Self, BifrostClientError> {
        let schema = serde_json::to_value(schemars::schema_for!(T)).map_err(|error| {
            wyrd_queue::WyrdQueueError::SchemaParse(format!(
                "model schema is not representable as JSON: {error}"
            ))
        })?;
        Self::from_json_schema(fqn, &schema)
    }

    /// Fetch an already-registered table's config by name.
    ///
    /// Reads `GET /v1/bifrost/tables/{namespace}/{name}` and takes the schema
    /// from the description's `user_fields`, so a writer joining an existing
    /// table never restates a schema it does not own. The returned config is
    /// already resolved and carries the server's own physical layout.
    ///
    /// # Arguments
    /// * `client` - The authenticated client the describe request is sent over.
    /// * `fqn` - The registered table's `<namespace>.<name>`.
    ///
    /// # Errors
    ///
    /// Returns the stable not-found, authentication, authorization,
    /// availability, or protocol error the server reported, or a schema-parse
    /// error when the description cannot be mapped back to Arrow.
    ///
    /// # Cancellation
    ///
    /// Abandoning the future leaves no server state behind; describe is a read.
    pub async fn describe(client: &WyrdClient, fqn: &str) -> Result<Self, BifrostClientError> {
        let (namespace, name) = split_fqn(fqn)?;
        let description = crate::bifrost::query::QueryClient::new(client)
            .describe_table(&namespace, &name)
            .await?;
        Self::from_description(&description)
    }

    /// As [`TableConfig::describe`], resolving the transport itself.
    ///
    /// # Arguments
    /// * `fqn` - The registered table's `<namespace>.<name>`.
    ///
    /// # Errors
    ///
    /// As [`TableConfig::describe`], plus the stable no-credentials error when
    /// the credential chain yields nothing.
    pub async fn describe_from_env(fqn: &str) -> Result<Self, BifrostClientError> {
        let client = crate::bifrost::facade::client_from_env()?;
        Self::describe(&client, fqn).await
    }

    /// Project one server description onto a resolved config.
    ///
    /// Only `user_fields` become the declared schema: the correlation and
    /// managed classes are what the write path appends, and re-declaring them
    /// here would double them in the sealed batch.
    ///
    /// # Errors
    ///
    /// Returns a schema-parse error when a described column's type cannot be
    /// mapped back onto Arrow.
    pub(crate) fn from_description(
        description: &BifrostTableDescription,
    ) -> Result<Self, BifrostClientError> {
        let user_schema = wyrd_queue::fieldspec_to_arrow(&description.user_fields)?;
        Ok(Self {
            namespace: description.entry.namespace.clone(),
            name: description.entry.name.clone(),
            user_schema: Arc::new(user_schema),
            physical_layout: Some(description.physical_layout.clone()),
            compaction_target_file_size_bytes: description.compaction_target_file_size_bytes,
            compaction_type: description.compaction_type,
            resolved: Some(ResolvedTable {
                table_uid: description.entry.table_uid.clone(),
                fingerprint: description.entry.fingerprint.clone(),
            }),
        })
    }

    /// Declare the physical layout the register call should request.
    ///
    /// Omitted, the server resolves `hour(wyrd_event_time)`, `wyrd_event_time`
    /// descending nulls-last, and the managed Bloom floor.
    ///
    /// # Arguments
    /// * `layout` - The partitioning, sort, and Bloom layout to request at register.
    #[must_use]
    pub fn with_layout(mut self, layout: PhysicalLayoutWire) -> Self {
        self.physical_layout = Some(layout);
        self
    }

    /// Declare the soft file target Forge compacts this table toward, in bytes.
    ///
    /// Omitted, the table follows the Forge deployment default (1 GiB unless
    /// the operator moved it). The server refuses a target below the 128 MiB
    /// row-group target with `WYRD_VALA_400_BIFROST_INVALID_COMPACTION_TARGET`,
    /// and a re-register whose target differs from the stored one with
    /// `WYRD_VALA_409_BIFROST_COMPACTION_TARGET_MISMATCH`.
    ///
    /// # Arguments
    /// * `bytes` - The soft compaction file target, in bytes; at least 128 MiB.
    #[must_use]
    pub fn with_compaction_target_file_size_bytes(mut self, bytes: u64) -> Self {
        self.compaction_target_file_size_bytes = Some(bytes);
        self
    }

    /// The explicit compaction file target, when declared or described.
    #[must_use]
    pub fn compaction_target_file_size_bytes(&self) -> Option<u64> {
        self.compaction_target_file_size_bytes
    }

    /// Declare the physical compaction type Forge applies to this table.
    ///
    /// Omitted, the table stores no type and Forge compacts it `small-files`. A
    /// copy-on-write table compacts `full` whatever it declares. The server
    /// refuses a re-register whose type differs from the stored one with
    /// `WYRD_VALA_409_BIFROST_COMPACTION_TYPE_MISMATCH`.
    ///
    /// # Arguments
    /// * `compaction_type` - The compaction type Forge applies to this table.
    #[must_use]
    pub fn with_compaction_type(mut self, compaction_type: CompactionTypeWire) -> Self {
        self.compaction_type = Some(compaction_type);
        self
    }

    /// The explicit compaction type, when declared or described.
    #[must_use]
    pub fn compaction_type(&self) -> Option<CompactionTypeWire> {
        self.compaction_type
    }

    /// `<namespace>.<name>` — the name SQL and `SealedBatch.table` both use.
    #[must_use]
    pub fn fqn(&self) -> String {
        format!("{}.{}", self.namespace, self.name)
    }

    /// The namespace half of the identity.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// The name half of the identity.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The user columns this config declares, without correlation or managed
    /// columns. The producer appends those itself.
    #[must_use]
    pub fn user_schema(&self) -> &SchemaRef {
        &self.user_schema
    }

    /// The server-assigned identity, present only after register or describe.
    #[must_use]
    pub fn resolved(&self) -> Option<&ResolvedTable> {
        self.resolved.as_ref()
    }

    /// The register-call body this config asks the server to create.
    pub(crate) fn register_request(&self) -> RegisterTableRequest {
        RegisterTableRequest {
            namespace: self.namespace.clone(),
            name: self.name.clone(),
            fields: wyrd_queue::arrow_schema_to_fieldspec(&self.user_schema),
            physical_layout: self.physical_layout.clone(),
            compaction_target_file_size_bytes: self.compaction_target_file_size_bytes,
            compaction_type: self.compaction_type,
        }
    }

    /// Record the identity the server minted for this table.
    pub(crate) fn resolve(&mut self, response: &RegisterTableResponse) {
        self.resolved = Some(ResolvedTable {
            table_uid: response.table_uid.clone(),
            fingerprint: response.fingerprint.clone(),
        });
    }
}

/// The serializable projection of [`TableConfig`].
///
/// Declared columns travel as [`FieldSpec`] rather than an Arrow schema because
/// that is already the register wire shape, so a config crossing a language
/// boundary and coming back is the same config — including the server identity
/// a described table already carries.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TableConfigWire {
    /// Target namespace.
    namespace: String,
    /// Table name.
    name: String,
    /// The declared user columns, in declaration order.
    fields: Vec<FieldSpec>,
    /// The layout to request at register time, when one was declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    physical_layout: Option<PhysicalLayoutWire>,
    /// The explicit compaction file target, when one was declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compaction_target_file_size_bytes: Option<u64>,
    /// The explicit compaction type, when one was declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compaction_type: Option<CompactionTypeWire>,
    /// The server-minted identity, when the config is already resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resolved: Option<ResolvedTable>,
}

impl From<TableConfig> for TableConfigWire {
    /// Project one config onto its wire form for a language boundary.
    fn from(config: TableConfig) -> Self {
        Self {
            namespace: config.namespace,
            name: config.name,
            fields: wyrd_queue::arrow_schema_to_fieldspec(&config.user_schema),
            physical_layout: config.physical_layout,
            compaction_target_file_size_bytes: config.compaction_target_file_size_bytes,
            compaction_type: config.compaction_type,
            resolved: config.resolved,
        }
    }
}

impl TryFrom<TableConfigWire> for TableConfig {
    type Error = BifrostClientError;

    /// Rebuild one config from its wire form.
    ///
    /// # Errors
    ///
    /// Returns a schema-parse error when a declared column's type cannot be
    /// mapped onto Arrow.
    fn try_from(wire: TableConfigWire) -> Result<Self, Self::Error> {
        Ok(Self {
            namespace: wire.namespace,
            name: wire.name,
            user_schema: Arc::new(wyrd_queue::fieldspec_to_arrow(&wire.fields)?),
            physical_layout: wire.physical_layout,
            compaction_target_file_size_bytes: wire.compaction_target_file_size_bytes,
            compaction_type: wire.compaction_type,
            resolved: wire.resolved,
        })
    }
}

/// One table this connected writer has described, with its cached user schema.
///
/// Holding this value is the proof that the table was described once for this
/// writer's lifetime, which is what lets `Bifrost::insert_into`
/// stay a synchronous, IO-free enqueue: there is no way to name a destination
/// the writer has not already resolved, so no insert path can smuggle in a
/// per-observation describe or a caller-supplied schema.
///
/// One schema is authoritative per table name for that lifetime. A server-side
/// schema change therefore requires a new writer after shutdown; the existing
/// fingerprint fence refuses a stale batch rather than silently replacing a
/// live producer's schema.
#[derive(Debug, Clone)]
pub struct WriterTable {
    /// Fully-qualified `<namespace>.<name>` destination.
    fqn: Arc<str>,
    /// The user schema the server described for this table.
    schema: SchemaRef,
}

impl WriterTable {
    /// Build a described destination from its resolved config.
    #[must_use]
    pub(crate) fn new(fqn: &str, schema: SchemaRef) -> Self {
        Self {
            fqn: Arc::from(fqn),
            schema,
        }
    }

    /// The fully-qualified table name rows are routed to.
    #[must_use]
    pub fn fqn(&self) -> &str {
        &self.fqn
    }

    /// The described user schema rows are parsed against when a batch seals.
    #[must_use]
    pub fn user_schema(&self) -> &SchemaRef {
        &self.schema
    }
}

/// Optional per-row correlation.
///
/// Both fields are optional on the wire: the server stores an uncorrelated row
/// against the authenticated `principal_id` with a null `card_uid`, so the
/// default is a valid write rather than a degraded one.
#[derive(Debug, Clone, Default)]
pub struct Correlation {
    /// The UID of the Card this row belongs to, which the server stores as
    /// `card_uid` once it confirms the writer may correlate to that Card.
    pub card_uid: Option<CardUid>,
    /// The run that produced this row.
    pub run_id: Option<RunId>,
}

/// Split `<namespace>.<name>` on its final dot.
///
/// The final dot rather than the first, so a dotted namespace stays intact and
/// the table name is always the trailing segment SQL addresses.
///
/// # Errors
///
/// Returns a schema-parse error when either half is empty or the dot is absent.
pub(crate) fn split_fqn(fqn: &str) -> Result<(String, String), BifrostClientError> {
    let (namespace, name) = fqn.rsplit_once('.').ok_or_else(|| {
        wyrd_queue::WyrdQueueError::SchemaParse(format!(
            "table `{fqn}` is not `<namespace>.<name>`"
        ))
    })?;
    if namespace.is_empty() || name.is_empty() {
        return Err(wyrd_queue::WyrdQueueError::SchemaParse(format!(
            "table `{fqn}` is not `<namespace>.<name>`"
        ))
        .into());
    }
    Ok((namespace.to_owned(), name.to_owned()))
}

/// Refuse a declared column the write path already owns.
///
/// The server rejects these at register time too; catching them here names the
/// offending column before a round trip, and keeps a config that cannot be
/// registered from ever being bound as the active table.
///
/// # Errors
///
/// Returns a reserved-column error naming the first offending column.
fn reject_reserved_columns(schema: &SchemaRef) -> Result<(), BifrostClientError> {
    for field in schema.fields() {
        let name = field.name().as_str();
        if wyrd_queue::is_reserved_column(name) {
            return Err(wyrd_queue::WyrdQueueError::ReservedColumn(format!(
                "column `{name}` is server-owned and must not be declared"
            ))
            .into());
        }
    }
    Ok(())
}
