//! `bifrost.storage.objects` — the completed-object inventory.
//!
//! Owns the table's authored schema, physical layout, and the closed
//! vocabularies of its `origin` and `state` columns; the server stages its
//! rows through Scribe and reads them back for Card listing, hydration,
//! downloads, cleanup, and verification.

use arrow::datatypes::Field;

use crate::tables::fields::{int64, ts_us_utc, utf8};
use crate::tables::{DomainTable, attributed_layout, daily_layout, with_shared_dimensions};
use wyrd_spec::vala::api::PhysicalLayoutWire;

/// `bifrost.storage.objects` — one row per recorded object fact.
///
/// `storage_path` is the tenant-qualified stored object and the logical
/// object identity: transport and reconstructed completion retries record
/// the same path again, and readers keep one object per path. `upload_id` is
/// the upload session that completed it, null for an envelope the server
/// wrote itself. `backend` is the configured storage backend. `sha256` is the
/// declared SHA-256 as lowercase hex, null when the envelope write supplies
/// none; it is the declared digest, not an independently certified one.
/// `size_bytes`, `content_type`, and `sse_marker` are the completion's
/// existing metadata. `origin` is a [`ObjectOrigin`] spelling and grants
/// nothing. The shared `subject_card_uid` and `relative_path` are the exact
/// Card binding, null for an object no Card owns. `state` is a
/// [`ObjectState`] spelling: a `deleted` row records the existing lifecycle's
/// deletion of every object of its subject Card and dominates any completion row, earlier or replayed
/// later. `recorded_at` is the server's recording instant. Only the server
/// writes the table; Gate refuses public appends.
pub struct ObjectsTable;

/// Where a recorded object came from; descriptive only, never an
/// authorization grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectOrigin {
    /// A declared Card artifact file.
    CardFile,
    /// A verification File upload.
    FileUpload,
    /// A server-written immutable Card envelope.
    CardEnvelope,
}

impl ObjectOrigin {
    /// The stored `origin` spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CardFile => "card_file",
            Self::FileUpload => "file_upload",
            Self::CardEnvelope => "card_envelope",
        }
    }

    /// Parse a stored `origin` spelling; `None` for any other text.
    #[must_use]
    pub fn parse(stored: &str) -> Option<Self> {
        [Self::CardFile, Self::FileUpload, Self::CardEnvelope]
            .into_iter()
            .find(|origin| origin.as_str() == stored)
    }
}

/// Whether a row records a completed object or a deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectState {
    /// The object completed upload or its envelope write.
    Completed,
    /// The existing lifecycle deleted every object of the row's Card.
    Deleted,
}

impl ObjectState {
    /// The stored `state` spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Deleted => "deleted",
        }
    }

    /// Parse a stored `state` spelling; `None` for any other text.
    #[must_use]
    pub fn parse(stored: &str) -> Option<Self> {
        [Self::Completed, Self::Deleted]
            .into_iter()
            .find(|state| state.as_str() == stored)
    }
}

impl DomainTable for ObjectsTable {
    /// Lives in `bifrost.storage`, the namespace the catalog registers it under.
    const NAMESPACE: &'static str = "storage";
    /// Table segment of the fixed `bifrost.storage.objects` FQN.
    const NAME: &'static str = "objects";

    /// Authored columns in order; managed correlation and system columns are appended by the catalog.
    fn arrow_fields() -> Vec<Field> {
        with_shared_dimensions(vec![
            utf8("storage_path", false),
            utf8("upload_id", true),
            utf8("backend", false),
            utf8("sha256", true),
            int64("size_bytes", false),
            utf8("content_type", true),
            utf8("sse_marker", true),
            utf8("origin", false),
            utf8("relative_path", true),
            utf8("state", false),
            ts_us_utc("recorded_at", false),
        ])
    }

    /// Daily partitions on `recorded_at`, attribution-sorted, Bloom-pruned on
    /// the shared attribution set, whose `subject_card_uid` is the Card
    /// binding, and the storage identity and declared digest a lookup selects
    /// by.
    fn physical_layout() -> PhysicalLayoutWire {
        attributed_layout(daily_layout, "recorded_at", &["storage_path", "sha256"])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every origin and state round-trips through its stored spelling.
    ///
    /// # Panics
    /// Panics when a spelling does not parse back to its variant.
    #[test]
    fn vocabularies_round_trip() {
        for origin in [
            ObjectOrigin::CardFile,
            ObjectOrigin::FileUpload,
            ObjectOrigin::CardEnvelope,
        ] {
            assert_eq!(ObjectOrigin::parse(origin.as_str()), Some(origin));
        }
        for state in [ObjectState::Completed, ObjectState::Deleted] {
            assert_eq!(ObjectState::parse(state.as_str()), Some(state));
        }
        assert_eq!(ObjectOrigin::parse("artifact"), None);
        assert_eq!(ObjectState::parse("pending"), None);
    }
}
