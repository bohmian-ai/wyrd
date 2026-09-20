//! Principal revocation contracts.

use schemars::r#gen::SchemaGenerator;
use schemars::schema::{InstanceType, Schema, SchemaObject, StringValidation};
use serde::{Deserialize, Serialize};

use crate::auth::PrincipalKindTag;

/// Byte ceiling shared by the wire reason and the audit detail it becomes.
///
/// The reason exists to be recorded, and the audit-detail value type refuses
/// anything longer, so advertising a wider bound here would document a request
/// the server must then refuse.
pub const REASON_MAX_BYTES: u32 = 1_024;

/// `POST /v1/principals/{id}/revoke` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RevokePrincipalRequest {
    /// Required kind discriminator; principal ids are only unique with their table.
    pub principal_kind: PrincipalKindTag,
    /// Required audit reason for the revocation.
    #[schemars(schema_with = "reason_schema")]
    pub reason: String,
}

/// Constrain the audit reason in the generated schema to a non-empty, bounded
/// string, so a client cannot publish a revocation with no stated cause or an
/// unbounded one.
fn reason_schema(_gen: &mut SchemaGenerator) -> Schema {
    Schema::Object(SchemaObject {
        instance_type: Some(InstanceType::String.into()),
        string: Some(Box::new(StringValidation {
            min_length: Some(1),
            max_length: Some(REASON_MAX_BYTES),
            pattern: None,
        })),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::RevokePrincipalRequest;
    use crate::auth::PrincipalKindTag;

    #[test]
    fn revoke_principal_request_roundtrips() {
        let request = RevokePrincipalRequest {
            principal_kind: PrincipalKindTag::Service,
            reason: "operator requested rotation".to_owned(),
        };

        let value = serde_json::to_value(&request).expect("serializes");

        assert_eq!(value["principal_kind"], "service");
        assert_eq!(
            serde_json::from_value::<RevokePrincipalRequest>(value).expect("deserializes"),
            request
        );
    }

    #[test]
    fn revoke_principal_request_requires_kind_and_reason() {
        let missing_kind = serde_json::json!({ "reason": "incident response" });
        let missing_reason = serde_json::json!({ "principal_kind": "user" });

        assert!(serde_json::from_value::<RevokePrincipalRequest>(missing_kind).is_err());
        assert!(serde_json::from_value::<RevokePrincipalRequest>(missing_reason).is_err());
    }

    #[test]
    fn revoke_principal_request_rejects_unknown_fields() {
        let request = serde_json::json!({
            "principal_kind": "agent",
            "reason": "incident response",
            "extra": true
        });

        assert!(serde_json::from_value::<RevokePrincipalRequest>(request).is_err());
    }
}
