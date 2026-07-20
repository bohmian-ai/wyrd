//! Diagnostic serialization for loader consumers.

pub use super::error::{Diagnostic, Severity, SourceSpan, diagnostic_schema, emit_json};

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Diagnostic, SourceSpan, emit_json};

    #[test]
    fn diagnose_collects_all_in_one_pass_and_emits_stable_json() {
        let diagnostics = vec![
            Diagnostic::invalid_envelope(PathBuf::from("c.yaml"), "third".to_owned()),
            Diagnostic::invalid_envelope(PathBuf::from("a.yaml"), "first".to_owned()),
            Diagnostic {
                span: Some(SourceSpan { line: 2, column: 4 }),
                ..Diagnostic::invalid_envelope(PathBuf::from("b.yaml"), "second".to_owned())
            },
        ];

        let json = emit_json(&diagnostics).expect("diagnostics serialize");
        let values: Vec<serde_json::Value> = serde_json::from_str(&json).expect("valid JSON");

        assert_eq!(values.len(), 3);
        assert_eq!(values[0]["path"], "a.yaml");
        assert_eq!(values[1]["path"], "b.yaml");
        assert_eq!(values[2]["path"], "c.yaml");
        assert!(values.iter().all(|value| value["severity"] == "error"));
        assert!(values.iter().all(|value| value["status"] == 400));
    }

    #[test]
    fn diagnostic_schema_exposes_stable_status_and_context() {
        let schema = super::diagnostic_schema();
        let value = serde_json::to_value(schema).expect("schema serializes");
        let properties = value["properties"]
            .as_object()
            .expect("diagnostic schema has properties");
        assert!(properties.contains_key("status"));
        assert!(properties.contains_key("context"));
    }
}
