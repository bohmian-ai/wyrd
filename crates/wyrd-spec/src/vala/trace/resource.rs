//! OTel `Resource` projection -- the producing entity.
//!
//! (Proto: `opentelemetry.proto.resource.v1.Resource`.)
//!
//! Resource describes the immutable producer of telemetry: service name,
//! deployment-instance metadata, host, k8s, and cloud identity. OTel keeps it
//! flat as one attribute set; Wyrd promotes the four `service.*` fields into
//! typed columns so the OLAP query path can index them without scanning a JSON
//! map. Everything else stays in the `attributes` bag.

use serde::{Deserialize, Serialize};

use crate::error::WyrdError;

/// OTel-canonical resource projection.
///
/// Fields:
///
/// - `service_name` -- required OTel `service.name` semantic-convention
///   attribute. Empty values are rejected; non-empty values are capped at 256
///   characters.
/// - `service_namespace` -- optional OTel `service.namespace`.
/// - `service_version` -- optional OTel `service.version`.
/// - `service_instance_id` -- optional OTel `service.instance.id`.
/// - `attributes` -- every other OTel resource attribute the producer emitted,
///   such as `host.name`, `k8s.pod.uid`, `telemetry.sdk.name`, or
///   `process.pid`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    /// `service.name`, required by OTel semantic conventions.
    pub service_name: String,
    /// `service.namespace`, when emitted by the producer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_namespace: Option<String>,
    /// `service.version`, when emitted by the producer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_version: Option<String>,
    /// `service.instance.id`, when emitted by the producer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_instance_id: Option<String>,
    /// Remaining OTel resource attributes, such as `host.*`, `k8s.*`, and
    /// `cloud.*`.
    #[serde(default)]
    pub attributes: serde_json::Map<String, serde_json::Value>,
}

impl Resource {
    /// Validate the resource-level invariants.
    ///
    /// Enforces:
    ///
    /// - `service_name` is non-empty and no longer than 256 characters.
    /// - `service_namespace`, `service_version`, and `service_instance_id` are
    ///   no longer than 256 characters when present.
    /// - `attributes` contains no more than 256 entries.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the resource violates one of
    /// these invariants.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.service_name.is_empty() {
            return Err(WyrdError::Validation {
                message:
                    "resource.service_name must be non-empty (OTel semconv: required attribute)"
                        .to_string(),
                details: serde_json::Value::Null,
            });
        }
        if self.service_name.len() > 256 {
            return Err(WyrdError::Validation {
                message: format!(
                    "resource.service_name length must be <= 256, got {}",
                    self.service_name.len()
                ),
                details: serde_json::Value::Null,
            });
        }
        for (field, value) in [
            ("service_namespace", &self.service_namespace),
            ("service_version", &self.service_version),
            ("service_instance_id", &self.service_instance_id),
        ] {
            if let Some(v) = value
                && v.len() > 256
            {
                return Err(WyrdError::Validation {
                    message: format!("resource.{field} length must be <= 256, got {}", v.len()),
                    details: serde_json::Value::Null,
                });
            }
        }
        if self.attributes.len() > 256 {
            return Err(WyrdError::Validation {
                message: format!(
                    "resource.attributes count must be <= 256, got {}",
                    self.attributes.len()
                ),
                details: serde_json::Value::Null,
            });
        }
        Ok(())
    }
}
