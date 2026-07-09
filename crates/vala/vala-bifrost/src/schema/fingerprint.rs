use arrow::datatypes::{FieldRef, Schema};
use sha2::{Digest, Sha256};

const SYSTEM_COL_PREFIX: &str = "wyrd_";
const RESERVED_COLS: &[&str] = &["data_tenant_id", "run_id", "card_uid", "principal_id"];

fn is_reserved(name: &str) -> bool {
    name.starts_with(SYSTEM_COL_PREFIX) || RESERVED_COLS.contains(&name)
}

/// Compute a schema fingerprint over user-only fields using Arrow IPC serialization.
///
/// System columns (`wyrd_*`, `data_tenant_id`) and correlation columns
/// (`run_id`, `card_uid`, `principal_id`) are stripped before hashing so the
/// fingerprint matches the build-time `SCHEMA_FINGERPRINT` constants, which are
/// also computed over user fields only.
pub fn fingerprint_user_fields(schema: &Schema) -> [u8; 32] {
    let user_fields: Vec<FieldRef> = schema
        .fields()
        .iter()
        .filter(|f| !is_reserved(f.name()))
        .cloned()
        .collect();
    let user_schema = Schema::new(user_fields);
    let mut buf = vec![];
    {
        let mut writer = arrow::ipc::writer::FileWriter::try_new(&mut buf, &user_schema)
            .expect("ipc fingerprint writer init");
        writer.finish().expect("ipc fingerprint writer finish");
    }
    let mut hasher = Sha256::new();
    hasher.update(&buf);
    hasher.finalize().into()
}
