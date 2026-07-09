use arrow::datatypes::{Field, FieldRef, Schema};
use sha2::{Digest, Sha256};

const SYSTEM_COL_PREFIX: &str = "wyrd_";
const RESERVED_COLS: &[&str] = &["data_tenant_id", "run_id", "card_uid", "principal_id"];

fn is_reserved(name: &str) -> bool {
    name.starts_with(SYSTEM_COL_PREFIX) || RESERVED_COLS.contains(&name)
}

/// SHA-256 over the Arrow IPC bytes of `fields` **exactly as given** (no filtering).
///
/// This is the declared-schema fingerprint for a pre-declared domain table: it
/// hashes the table's user fields precisely as declared in `arrow_fields()`,
/// byte-identical to the removed build-time `compute_fingerprint`. It does NOT
/// strip correlation/system names, so a table that declares `run_id`/`principal_id`
/// as a genuine content column (`agent_traces`, `audit_log`) fingerprints them too.
///
/// # Panics
///
/// Panics if the Arrow IPC writer cannot be initialized or finalized, which
/// should never happen for an in-memory buffer and a valid schema.
pub fn fingerprint_fields(fields: &[Field]) -> [u8; 32] {
    let schema = Schema::new(fields.to_vec());
    let mut buf = vec![];
    {
        let mut writer = arrow::ipc::writer::FileWriter::try_new(&mut buf, &schema)
            .expect("ipc fingerprint writer init");
        writer.finish().expect("ipc fingerprint writer finish");
    }
    let mut hasher = Sha256::new();
    hasher.update(&buf);
    hasher.finalize().into()
}

/// Compute a schema fingerprint over user-only fields, stripping system columns
/// (`wyrd_*`, `data_tenant_id`) and universal correlation columns (`run_id`,
/// `card_uid`, `principal_id`).
///
/// Used to compute the diagnostic `actual:` fingerprint from a *physical* Iceberg
/// schema (which carries the appended system/correlation columns). This is a
/// best-effort diagnostic only — drift detection itself is field-name/type
/// comparison in `physical_matches_declared`, not fingerprint equality.
///
/// This is NOT equal to `T::schema_fingerprint()` for tables whose policy declares
/// `run_id`/`principal_id` as content columns (`dev.agent_traces`, `system.audit_log`),
/// and is never a gate.
pub fn fingerprint_user_fields(schema: &Schema) -> [u8; 32] {
    let user_fields: Vec<FieldRef> = schema
        .fields()
        .iter()
        .filter(|f| !is_reserved(f.name()))
        .cloned()
        .collect();
    fingerprint_fields(
        &user_fields
            .iter()
            .map(|f| f.as_ref().clone())
            .collect::<Vec<_>>(),
    )
}
