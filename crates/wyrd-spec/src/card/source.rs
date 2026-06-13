//! Source Card spec.
//!
//! A Source is a read-side reference to an external data system. Wyrd reads,
//! never writes (Doctrine #7). The `source` field is a *read-shape bucket* —
//! the shape of data a consuming Drift/Eval Card sees — not a vendor. Vendors
//! (BigQuery vs Snowflake, Prometheus vs Datadog) are a connection detail
//! nested below the bucket, so a consumer binds to "row set" or "time series",
//! never to a specific vendor. Adding a vendor is a new `*Connection` variant
//! plus a runtime read adapter; it never touches the bucket set or any
//! consuming Card.
//!
//! Cards never carry secret material (Doctrine #7, `NonSecretValue`). All
//! credentials are named server-side environment variables resolved at read
//! time; only the env-var *name* lives on the Card. For secret-store backends
//! (Vault, SSM, GCP Secret Manager), use `SourceAuth::SecretStore` which
//! carries only the provider identifier and secret path, never the value.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::card::common::NonSecretValue;

/// Read-side reference to an external data system. Wyrd reads, never writes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SourceSpec {
    /// Source description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Read-shape bucket plus its vendor connection.
    pub source: SourceKind,
    /// Non-secret read defaults (projection, page size, time-window hints).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub defaults: BTreeMap<String, NonSecretValue>,
}

impl SourceSpec {
    /// Validate non-secret connection invariants (non-empty coordinates and
    /// non-empty env-var names). Credential *values* are never present.
    ///
    /// # Errors
    /// Returns a validation error when a required coordinate or env-var name
    /// is empty, or when the URI carries embedded credentials.
    pub fn validate(&self) -> Result<(), SourceValidationError> {
        self.source.validate()
    }
}

/// Read-shape bucket. The top-level discriminator (`kind`) is the *shape of
/// data a consumer reads*, not the vendor. Vendors live inside each variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[non_exhaustive]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceKind {
    /// Blobs at a URI parsed by `format`. Vendor (GCS/S3/Azure/file) is the
    /// URI scheme. Yields a record stream.
    ObjectStore {
        /// Object URI, e.g. `gs://bucket/prefix`, `s3://bucket/prefix`,
        /// `az://container/prefix`, or `file:///path`. Must not embed
        /// credentials in the authority component (Doctrine #7).
        uri: String,
        /// On-disk record format.
        format: ObjectFormat,
        /// Credential reference for the object store. Defaults to `None`
        /// (unauthenticated / ambient IAM). Use `Env` for service-account
        /// keys or `SecretStore` for Vault/SSM-managed credentials.
        #[serde(default)]
        auth: SourceAuth,
    },
    /// SQL query against a warehouse. Yields a row set.
    SqlWarehouse {
        /// Vendor connection coordinates and auth.
        connection: SqlConnection,
    },
    /// Metric/time-series query. Yields a labeled time series.
    Metrics {
        /// Vendor connection coordinates and auth.
        connection: MetricsConnection,
    },
    /// Log query. Yields log records.
    Logs {
        /// Vendor connection coordinates and auth.
        connection: LogConnection,
    },
    /// Trace/span query. Yields spans.
    Traces {
        /// Vendor connection coordinates and auth.
        connection: TraceConnection,
    },
}

impl SourceKind {
    fn validate(&self) -> Result<(), SourceValidationError> {
        match self {
            Self::ObjectStore { uri, auth, .. } => {
                non_empty("uri", uri)?;
                if uri_has_embedded_credentials(uri) {
                    return Err(SourceValidationError::EmbeddedCredential { field: "uri" });
                }
                auth.validate()
            }
            Self::SqlWarehouse { connection } => connection.validate(),
            Self::Metrics { connection } => connection.validate(),
            Self::Logs { connection } => connection.validate(),
            Self::Traces { connection } => connection.validate(),
        }
    }
}

/// Object-store record format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ObjectFormat {
    /// Apache Parquet.
    Parquet,
    /// JSON Lines.
    Jsonl,
    /// Arrow IPC stream.
    ArrowIpc,
    /// Comma-separated values.
    Csv,
}

/// SQL-warehouse vendor connection. Discriminated by `vendor`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[non_exhaustive]
#[serde(tag = "vendor", rename_all = "snake_case")]
pub enum SqlConnection {
    /// Google BigQuery.
    BigQuery {
        /// GCP project id.
        project: String,
        /// Default dataset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dataset: Option<String>,
        /// Processing location, e.g. `US` or `EU`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        location: Option<String>,
        /// Credential reference.
        #[serde(default)]
        auth: SourceAuth,
    },
    /// Snowflake.
    Snowflake {
        /// Account identifier.
        account: String,
        /// Virtual warehouse.
        warehouse: String,
        /// Database.
        database: String,
        /// Default schema.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schema: Option<String>,
        /// Session role.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        role: Option<String>,
        /// Credential reference.
        #[serde(default)]
        auth: SourceAuth,
    },
    /// PostgreSQL (and wire-compatible engines).
    Postgres {
        /// Host name.
        host: String,
        /// TCP port.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        port: Option<u16>,
        /// Database name.
        database: String,
        /// TLS mode, e.g. `require` or `verify-full`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sslmode: Option<String>,
        /// Credential reference.
        #[serde(default)]
        auth: SourceAuth,
    },
}

impl SqlConnection {
    fn validate(&self) -> Result<(), SourceValidationError> {
        match self {
            Self::BigQuery { project, auth, .. } => {
                non_empty("project", project)?;
                auth.validate()
            }
            Self::Snowflake {
                account,
                warehouse,
                database,
                auth,
                ..
            } => {
                non_empty("account", account)?;
                non_empty("warehouse", warehouse)?;
                non_empty("database", database)?;
                auth.validate()
            }
            Self::Postgres {
                host,
                database,
                auth,
                ..
            } => {
                non_empty("host", host)?;
                non_empty("database", database)?;
                auth.validate()
            }
        }
    }
}

/// Metrics vendor connection. Discriminated by `vendor`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[non_exhaustive]
#[serde(tag = "vendor", rename_all = "snake_case")]
pub enum MetricsConnection {
    /// Prometheus / PromQL. Any PromQL-compatible backend (Grafana Mimir,
    /// Grafana Cloud, Thanos, VictoriaMetrics) targets this variant via
    /// `endpoint`.
    Prometheus {
        /// Query endpoint URL.
        endpoint: String,
        /// Credential reference.
        #[serde(default)]
        auth: SourceAuth,
    },
    /// Datadog metrics.
    Datadog {
        /// API site, e.g. `datadoghq.com` or `datadoghq.eu`.
        site: String,
        /// Required API read scopes, e.g. `metrics_read`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        api_scopes: Vec<String>,
        /// Credential reference.
        #[serde(default)]
        auth: SourceAuth,
    },
    /// AWS CloudWatch.
    Cloudwatch {
        /// AWS region.
        region: String,
        /// Default metric namespace.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        /// Credential reference.
        #[serde(default)]
        auth: SourceAuth,
    },
}

impl MetricsConnection {
    fn validate(&self) -> Result<(), SourceValidationError> {
        match self {
            Self::Prometheus { endpoint, auth } => {
                non_empty("endpoint", endpoint)?;
                auth.validate()
            }
            Self::Datadog { site, auth, .. } => {
                non_empty("site", site)?;
                auth.validate()
            }
            Self::Cloudwatch { region, auth, .. } => {
                non_empty("region", region)?;
                auth.validate()
            }
        }
    }
}

/// Log vendor connection. Discriminated by `vendor`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[non_exhaustive]
#[serde(tag = "vendor", rename_all = "snake_case")]
pub enum LogConnection {
    /// Grafana Loki.
    Loki {
        /// Query endpoint URL.
        endpoint: String,
        /// Credential reference.
        #[serde(default)]
        auth: SourceAuth,
    },
    /// Elasticsearch / OpenSearch.
    Elasticsearch {
        /// Query endpoint URL.
        endpoint: String,
        /// Default index or data stream.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<String>,
        /// Credential reference.
        #[serde(default)]
        auth: SourceAuth,
    },
    /// Splunk.
    Splunk {
        /// Query endpoint URL.
        endpoint: String,
        /// Credential reference.
        #[serde(default)]
        auth: SourceAuth,
    },
}

impl LogConnection {
    fn validate(&self) -> Result<(), SourceValidationError> {
        match self {
            Self::Loki { endpoint, auth } | Self::Splunk { endpoint, auth } => {
                non_empty("endpoint", endpoint)?;
                auth.validate()
            }
            Self::Elasticsearch { endpoint, auth, .. } => {
                non_empty("endpoint", endpoint)?;
                auth.validate()
            }
        }
    }
}

/// Trace vendor connection. Discriminated by `vendor`.
///
/// No `OtlpHttp` variant: OTLP (OpenTelemetry Protocol) is an
/// ingestion/push protocol. Wyrd reads traces from vendor query APIs
/// (Jaeger HTTP Query API, Tempo HTTP API, Datadog APM REST), which are
/// distinct from OTLP ingest endpoints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[non_exhaustive]
#[serde(tag = "vendor", rename_all = "snake_case")]
pub enum TraceConnection {
    /// Grafana Tempo.
    Tempo {
        /// Query endpoint URL.
        endpoint: String,
        /// Credential reference.
        #[serde(default)]
        auth: SourceAuth,
    },
    /// Datadog APM.
    DatadogApm {
        /// API site, e.g. `datadoghq.com`.
        site: String,
        /// Credential reference.
        #[serde(default)]
        auth: SourceAuth,
    },
    /// Jaeger.
    Jaeger {
        /// Query endpoint URL.
        endpoint: String,
        /// Credential reference.
        #[serde(default)]
        auth: SourceAuth,
    },
}

impl TraceConnection {
    fn validate(&self) -> Result<(), SourceValidationError> {
        match self {
            Self::Tempo { endpoint, auth } | Self::Jaeger { endpoint, auth } => {
                non_empty("endpoint", endpoint)?;
                auth.validate()
            }
            Self::DatadogApm { site, auth } => {
                non_empty("site", site)?;
                auth.validate()
            }
        }
    }
}

/// Credential reference for Source read access. Every secret is a named
/// server-side environment variable resolved at read time; the Card carries
/// only the env-var *name*, never a secret value (Doctrine #7).
///
/// `CredentialRef` (see `common.rs`) is a distinct type used on ServiceSpec
/// for outbound service identity. `SourceAuth` is the read-path credential
/// shape, scoped to Source Cards.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[non_exhaustive]
#[serde(tag = "scheme", rename_all = "snake_case")]
pub enum SourceAuth {
    /// No credential (public/unauthenticated endpoint).
    #[default]
    None,
    /// Single secret read from one env var (bearer token, password, connection
    /// secret, key-pair, or service-account JSON delivered via env).
    Env {
        /// Server-side env-var name holding the secret.
        env: String,
    },
    /// Username (non-secret) plus a password read from an env var.
    Basic {
        /// Login user (redacted in Debug output; omitted from logs to avoid accidental exposure).
        username: String,
        /// Server-side env-var name holding the password.
        password_env: String,
    },
    /// Multiple named secrets, each read from its own env var. Keys are logical
    /// credential names (e.g. `api_key`, `app_key`); values are env-var names.
    MultiEnv {
        /// Logical credential name → server-side env-var name.
        vars: BTreeMap<String, String>,
    },
    /// Secret stored in a server-side secret store (Vault, AWS SSM, GCP
    /// Secret Manager). Carries only the provider identifier and secret path,
    /// never the secret value.
    SecretStore {
        /// Provider identifier, e.g. `vault`, `aws-ssm`, `gcp-secretmanager`.
        provider: String,
        /// Secret name or path within the provider.
        name: String,
    },
}

impl fmt::Debug for SourceAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Env { env } => f.debug_struct("Env").field("env", env).finish(),
            Self::Basic { password_env, .. } => f
                .debug_struct("Basic")
                .field("username", &"[redacted]")
                .field("password_env", password_env)
                .finish(),
            Self::MultiEnv { vars } => f.debug_struct("MultiEnv").field("vars", vars).finish(),
            Self::SecretStore { provider, name } => f
                .debug_struct("SecretStore")
                .field("provider", provider)
                .field("name", name)
                .finish(),
        }
    }
}

impl SourceAuth {
    fn validate(&self) -> Result<(), SourceValidationError> {
        match self {
            Self::None => Ok(()),
            Self::Env { env } => non_empty("auth.env", env),
            Self::Basic {
                username,
                password_env,
            } => {
                non_empty("auth.username", username)?;
                non_empty("auth.password_env", password_env)
            }
            Self::MultiEnv { vars } => {
                if vars.is_empty() {
                    return Err(SourceValidationError::EmptyField { field: "auth.vars" });
                }
                for (key, env) in vars {
                    non_empty("auth.vars.key", key)?;
                    non_empty("auth.vars.value", env)?;
                }
                Ok(())
            }
            Self::SecretStore { provider, name } => {
                non_empty("auth.provider", provider)?;
                non_empty("auth.name", name)
            }
        }
    }
}

fn non_empty(field: &'static str, value: &str) -> Result<(), SourceValidationError> {
    if value.trim().is_empty() {
        return Err(SourceValidationError::EmptyField { field });
    }
    Ok(())
}

/// Returns `true` if the URI has embedded credentials in the authority
/// component (`scheme://user:password@host`). Wyrd Cards must never carry
/// secret values (Doctrine #7).
///
/// Only the authority segment (between `://` and the first `/`) is inspected so
/// that path components containing `:` or `@` (e.g. version tags in GCS/S3 keys)
/// are not falsely rejected. Percent-encoded delimiters (`%40`, `%3A`) are
/// decoded before checking so they cannot be used to smuggle credentials past
/// the check.
fn uri_has_embedded_credentials(uri: &str) -> bool {
    let Some((_, rest)) = uri.split_once("://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    let decoded = authority
        .replace("%40", "@")
        .replace("%3A", ":")
        .replace("%3a", ":");
    decoded.contains('@')
        && decoded
            .split('@')
            .next()
            .is_some_and(|info| info.contains(':'))
}

/// Source validation failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourceValidationError {
    /// A required coordinate or env-var name was empty or whitespace-only.
    #[error("source field `{field}` must not be empty")]
    EmptyField {
        /// Offending field path.
        field: &'static str,
    },
    /// A URI field contained embedded credentials (Doctrine #7 violation).
    #[error("source field `{field}` must not contain embedded credentials")]
    EmbeddedCredential {
        /// Offending field path.
        field: &'static str,
    },
}

impl From<SourceValidationError> for crate::error::WyrdError {
    fn from(e: SourceValidationError) -> Self {
        let field = match &e {
            SourceValidationError::EmptyField { field }
            | SourceValidationError::EmbeddedCredential { field } => *field,
        };
        Self::SourceValidation {
            message: e.to_string(),
            details: serde_json::json!({ "field": field }),
        }
    }
}
