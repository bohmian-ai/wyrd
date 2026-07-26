//! Wyrd foundation types.
//!
//! `wyrd-spec` is pure data: no IO, no async runtime, no PyO3, and no server
//! framework dependencies. It defines the Card envelope, v1 Spec payloads,
//! cross-cutting identifiers, and generated schema surfaces used by every
//! downstream crate.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod actor;
pub mod api_version;
pub mod auth;
pub mod card;
pub mod envelope;
pub mod error;
pub mod format;
pub mod graph;
pub mod ids;
pub mod intel;
pub mod metadata;
pub mod origin;
pub mod query;
pub mod redaction;
pub mod reference;
pub mod refs;
pub mod registry;
pub mod request_id;
pub mod run;
pub mod schema;
pub mod security;
pub mod storage;
pub mod trace;
pub mod vala;

pub use card::agent::{AgentCard, AgentRunConfigSpec, AgentSpec};
pub use card::data::{
    ArrowFormat, ArrowMeta, ColValue, ColorMode, CustomDataMeta, DataInterface, DataSchema,
    DataSpec, DataSplit, DataStats, HuggingfaceMeta, ImageFormat, ImageMeta, Inequality,
    JsonlCompression, JsonlMeta, NumpyFormat, NumpyMeta, PandasMeta, ParquetCompression,
    ParquetMeta, PolarsMeta, SplitStrategy, SqlLogic, SqlMeta, TextMeta, TorchMeta,
    TorchSaveFormat,
};
pub use card::field::{Dim, FieldSpec};
pub use card::prompt::{
    CardLoadFormat, ParameterName, PromptRef, PromptSpec, extract_text_placeholders,
    parse_card_bytes, parse_spec_bytes, serialize_card, serialize_spec_bytes,
};
pub use card::workflow::{
    WorkflowAction, WorkflowCard, WorkflowCardError, WorkflowRetryPolicy, WorkflowSpec,
    WorkflowStep, WorkflowValidationError,
};
pub use ids::{ColumnName, DataTenantId, QueryName, RoleName, SplitName, TenantSlug, uuid7};
pub use intel::{Evidence, Lineage, TimeRange};
pub use metadata::{
    AnnotationKey, AnnotationValue, Annotations, CardMetadata, LabelKey, LabelValue, Labels,
    MetadataError,
};
pub use origin::{CommitSha, Origin, OriginValidationError};
pub use query::{FieldRef, MetadataQuery, Operator, Predicate, QueryFieldErrorDetail, Value};
#[cfg(any(test, feature = "test-utils"))]
pub use security::InlineSecret;
pub use security::{SecretRef, SecretRefError, TlsConfig};
pub use skald_spec::{MessageNum, Prompt, ProviderRequest, ProviderResponse, ResponseType};

#[cfg(test)]
mod foundation_contracts_tests {
    use crate::error::WyrdError;
    use crate::ids::{CardUid, DataTenantId, SpaceName, TenantSlug};
    use crate::request_id::RequestId;
    use crate::trace::TraceContext;
    use wyrd_semver::{VersionBlock, VersionBump, VersionRange};

    #[test]
    fn ids_enforce_type_specific_canonical_forms() {
        assert!(SpaceName::new("prod_space").is_ok());
        assert!(SpaceName::new("ProdSpace").is_err());
        assert!(SpaceName::new("ab").is_err());

        assert!(CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00").is_ok());
        assert!(CardUid::new("550e8400-e29b-41d4-a716-446655440000").is_err());

        assert!(TenantSlug::new("a").is_ok());
        assert!(TenantSlug::new("0_acme-corp").is_ok());
        assert!(TenantSlug::new("Acme").is_err());
        assert!(TenantSlug::new("acme.corp").is_err());
    }

    #[test]
    fn request_id_accepts_uuid7_only() {
        assert!(RequestId::parse("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00").is_ok());
        assert!(RequestId::parse("550e8400-e29b-41d4-a716-446655440000").is_err());
        assert!(RequestId::parse("01ARZ3NDEKTSV4RRFFQ69G5FAV").is_err());
    }

    #[test]
    fn data_tenant_id_is_uuid7_backed() {
        let generated = DataTenantId::new_v7();
        assert_eq!(generated.as_uuid().get_version_num(), 7);

        let parsed: DataTenantId = generated.to_string().parse().unwrap();
        assert_eq!(parsed, generated);
        assert!(DataTenantId::new(uuid::Uuid::nil()).is_err());
        assert!(
            "550e8400-e29b-41d4-a716-446655440000"
                .parse::<DataTenantId>()
                .is_err()
        );
    }

    #[test]
    fn traceparent_validation_is_w3c_canonical() {
        let context = TraceContext::parse_traceparent(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            String::new(),
        )
        .unwrap();
        assert_eq!(
            context.to_traceparent(),
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        );
        assert!(
            TraceContext::parse_traceparent(
                "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
                String::new(),
            )
            .is_err()
        );
        assert!(
            TraceContext::parse_traceparent(
                "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
                String::new(),
            )
            .is_err()
        );
    }

    #[test]
    fn version_helpers_match_bump_and_sort() {
        let range = VersionRange::parse("^1.2").unwrap();
        assert!(range.matches(&VersionBlock::parse("1.4.0").unwrap()));
        assert!(!range.matches(&VersionBlock::parse("2.0.0").unwrap()));

        let version = VersionBlock::parse("1.2.3").unwrap();
        assert_eq!(version.bump(VersionBump::Minor).unwrap().as_str(), "1.3.0");

        let mut versions = vec![
            VersionBlock::parse("1.10.0").unwrap(),
            VersionBlock::parse("1.2.0").unwrap(),
        ];
        VersionBlock::sort_versions(&mut versions);
        assert_eq!(versions[0].as_str(), "1.2.0");
    }

    #[test]
    fn foundation_error_has_stable_problem_payload() {
        let error = WyrdError::Validation {
            message: "bad card".to_string(),
            details: serde_json::json!({ "field": "kind" }),
        };
        let problem = error.as_problem_json();
        assert_eq!(error.code(), "WYRD_SPEC_400_VALIDATION");
        assert_eq!(problem["status"], 400);
        assert_eq!(problem["code"], "WYRD_SPEC_400_VALIDATION");
        assert!(problem["remediation"].as_str().unwrap().contains("schema"));
    }
}

#[cfg(test)]
mod foundation_run_kind_tests {
    use crate::run::RunKind;
    use serde_json::json;

    #[test]
    fn remediation_run_kind_round_trips() {
        let encoded = serde_json::to_value(RunKind::Remediation).unwrap();
        assert_eq!(encoded, json!({"type": "remediation"}));

        let decoded: RunKind = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, RunKind::Remediation);
    }
}

#[cfg(test)]
mod generated_schema_goldens_tests {
    #[test]
    fn generated_schemas_match_goldens() {
        let generated = std::path::Path::new("schemas");
        let golden = std::path::Path::new("tests/schemas");
        for entry in std::fs::read_dir(golden).expect("golden schema dir") {
            let entry = entry.expect("schema entry");
            let expected = std::fs::read_to_string(entry.path()).expect("read golden");
            let actual =
                std::fs::read_to_string(generated.join(entry.file_name())).expect("read generated");
            assert_eq!(actual, expected, "schema drift: {:?}", entry.file_name());
        }
    }
}
