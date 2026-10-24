//! The object-path grammar every Forge rewrite output must satisfy.
//!
//! A produced object's path is load-bearing in three separate ways, and this
//! module is where all three are checked at once. It carries the table version
//! whose definition the output was written under, so the output states the
//! complete definition it was selected for. It carries the attempt identity, which is what
//! makes an abandoned attempt's objects reclaimable without consulting any
//! durable state. And it carries a per-writer ordinal plus a per-writer UUID,
//! which together keep concurrent writers inside one attempt from colliding.
//!
//! The grammar is the pinned core's, not this repository's invention:
//!
//! ```text
//! {table}/data/forge/{table_version}/[{partition}/]{attempt}-{ordinal:05}-{writer}.parquet
//! ```
//!
//! The optional partition segment is inserted by Iceberg's own location
//! generator for a partitioned table and is accepted, not required. The
//! attempt-global ordinal reported by the core's ledger is deliberately *not*
//! in the path: it is drain and reconciliation evidence, and reconstructing it
//! from a filename that carries a resettable per-writer counter would be wrong.

use uuid::Uuid;

use crate::forge::error::ForgeError;

/// Minimum width the core's file-name generator pads its ordinal to.
const ORDINAL_WIDTH: usize = 5;

/// Object suffix every rewrite output carries.
const OUTPUT_SUFFIX: &str = ".parquet";

/// Length of a hyphenated UUID in its canonical text form.
///
/// Both identities in a produced file name are UUIDs and the ordinal between
/// them is unbounded, so the file name is split at the fixed identity widths
/// rather than on hyphens, which appear inside the UUIDs themselves.
const UUID_TEXT_LEN: usize = 36;

/// One produced object path, decomposed after it was proven well-formed.
///
/// Borrowed from the path it describes: every field is a view into the original
/// string, so validating a path costs no allocation and the result cannot drift
/// from its source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForgeOutputIdentity<'path> {
    /// Selected output table version the version segment names.
    pub(crate) table_version: wyrd_spec::vala::ids::TableVersion,
    /// Partition segments between the version segment and the file name, if any.
    pub(crate) partition_path: &'path str,
    /// Attempt that opened the writer which produced this object.
    pub(crate) attempt_id: Uuid,
    /// Per-writer sequence number, which resets for each new writer.
    pub(crate) writer_ordinal: u64,
    /// Per-writer identity that keeps concurrent writers from colliding.
    pub(crate) writer_uuid: Uuid,
}

impl<'path> ForgeOutputIdentity<'path> {
    /// Recovers the output identity a produced path carries, on its own terms.
    ///
    /// Cleanup is the caller that has no prior identity to check against: it
    /// finds an object under the Forge root and must decide who produced it
    /// before it may decide whether that producer is finished. This is the one
    /// grammar that answers that, so no cleanup-local compatibility parser can
    /// drift away from what the writer actually emitted.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the path does not sit under
    /// `forge_root`, when its version segment is not a canonical table
    /// version, when a partition segment is non-canonical, when the
    /// file name is not `{attempt}-{ordinal}-{writer}.parquet`, when the
    /// ordinal is not a canonical decimal of at least [`ORDINAL_WIDTH`]
    /// digits, or when either UUID is unparseable.
    pub(crate) fn parse(path: &'path str, forge_root: &str) -> Result<Self, ForgeError> {
        let invariant = |detail: String| ForgeError::Invariant { detail };
        let root = forge_root.trim_end_matches('/');
        let (version_text, relative) = path
            .strip_prefix(root)
            .and_then(|rest| rest.strip_prefix('/'))
            .and_then(|rest| rest.split_once('/'))
            .ok_or_else(|| {
                invariant(format!(
                    "rewrite output {path} is not under a version of the Forge root {root}"
                ))
            })?;
        let table_version = version_text.parse().map_err(|_| {
            invariant(format!(
                "rewrite output {path} names no canonical table version"
            ))
        })?;
        let (partition_path, file_name) = match relative.rsplit_once('/') {
            Some((partition, name)) => (partition, name),
            None => ("", relative),
        };
        if !partition_path.is_empty()
            && partition_path
                .split('/')
                .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            return Err(invariant(format!(
                "rewrite output {path} carries a non-canonical partition segment"
            )));
        }
        let stem = file_name.strip_suffix(OUTPUT_SUFFIX).ok_or_else(|| {
            invariant(format!(
                "rewrite output {path} is not a {OUTPUT_SUFFIX} object"
            ))
        })?;
        let (attempt_text, remainder) = stem
            .split_at_checked(UUID_TEXT_LEN)
            .and_then(|(attempt, rest)| Some((attempt, rest.strip_prefix('-')?)))
            .ok_or_else(|| {
                invariant(format!(
                    "rewrite output {path} does not begin with an attempt identity"
                ))
            })?;
        let attempt_id = Uuid::parse_str(attempt_text).map_err(|error| {
            invariant(format!(
                "rewrite output {path} carries an unreadable attempt identity: {error}"
            ))
        })?;
        let (ordinal_text, writer_text) = remainder
            .len()
            .checked_sub(UUID_TEXT_LEN)
            .and_then(|split| remainder.split_at_checked(split))
            .and_then(|(ordinal, writer)| Some((ordinal.strip_suffix('-')?, writer)))
            .ok_or_else(|| {
                invariant(format!(
                    "rewrite output {path} carries no per-writer identity"
                ))
            })?;
        let writer_uuid = Uuid::parse_str(writer_text).map_err(|error| {
            invariant(format!(
                "rewrite output {path} carries an unreadable writer identity: {error}"
            ))
        })?;
        let writer_ordinal = canonical_ordinal(ordinal_text).ok_or_else(|| {
            invariant(format!(
                "rewrite output {path} carries a non-canonical ordinal {ordinal_text:?}"
            ))
        })?;
        Ok(Self {
            table_version,
            partition_path,
            attempt_id,
            writer_ordinal,
            writer_uuid,
        })
    }

    /// Proves one produced path belongs to this attempt and selected version.
    ///
    /// This is [`Self::parse`] plus the checks the producing side can make and
    /// cleanup cannot: that the recovered attempt is the attempt whose result
    /// set the object arrived in, and that the version is the one it selected. An object attributed to another
    /// attempt there is either a leaked handle or a reused writer, and both are
    /// unrecoverable ambiguities about who may reclaim the object.
    ///
    /// # Errors
    ///
    /// Returns every [`Self::parse`] failure, plus [`ForgeError::Invariant`]
    /// when the recovered attempt is not `attempt_id` or the version is not
    /// `table_version`.
    pub(crate) fn validate(
        path: &'path str,
        forge_root: &str,
        attempt_id: Uuid,
        table_version: wyrd_spec::vala::ids::TableVersion,
    ) -> Result<Self, ForgeError> {
        let identity = Self::parse(path, forge_root)?;
        if identity.attempt_id != attempt_id {
            return Err(ForgeError::Invariant {
                detail: format!("rewrite output {path} is not attributed to attempt {attempt_id}"),
            });
        }
        if identity.table_version != table_version {
            return Err(ForgeError::Invariant {
                detail: format!(
                    "rewrite output {path} is not written under version {table_version}"
                ),
            });
        }
        Ok(identity)
    }

    /// Returns the key that makes two produced objects distinguishable.
    ///
    /// A per-writer ordinal resets, so it identifies nothing on its own; paired
    /// with the writer UUID it is unique across every writer in the attempt.
    pub(crate) fn writer_key(&self) -> (Uuid, u64) {
        (self.writer_uuid, self.writer_ordinal)
    }
}

/// Parses one zero-padded decimal ordinal, rejecting every other spelling.
///
/// Canonical means: at least [`ORDINAL_WIDTH`] ASCII digits, and no extra
/// leading zero once the value has outgrown that width. Accepting a loose
/// spelling would let two different strings name the same ordinal, which is
/// exactly the ambiguity the ordinal exists to remove.
fn canonical_ordinal(text: &str) -> Option<u64> {
    if text.len() < ORDINAL_WIDTH || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if text.len() > ORDINAL_WIDTH && text.starts_with('0') {
        return None;
    }
    let value = text.parse::<u64>().ok()?;
    (format!("{value:0ORDINAL_WIDTH$}") == text).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Forge root of the fixture table.
    fn root() -> String {
        crate::catalog::layout::forge_data_root("file:///warehouse/tenant/table")
    }

    /// The version the fixture outputs are selected under.
    const VERSION: wyrd_spec::vala::ids::TableVersion = wyrd_spec::vala::ids::TableVersion::INITIAL;

    /// The version path is accepted, and per-writer ordinals stay per-writer.
    ///
    /// Two writers inside one attempt both start at ordinal `00000`, which is
    /// correct: the ordinal is a writer-local counter and the writer UUID is
    /// what separates them. The test pins both halves of that — the ordinals
    /// genuinely collide, and the identities do not — because treating the
    /// reset as a collision would fail every real concurrent rewrite, while
    /// dropping the writer UUID would let two writers overwrite each other.
    ///
    /// The refusals cover the other direction: a missing root, a
    /// non-canonical or other version, a foreign attempt, a missing writer identity, and a non-canonical
    /// ordinal are each unrecoverable ambiguities about which attempt owns an
    /// object, so each is refused rather than normalized.
    #[test]
    fn forge_output_identity_accepts_version_path_and_per_writer_ordinals() {
        let root = root();
        let attempt = Uuid::now_v7();
        let first_writer = Uuid::now_v7();
        let second_writer = Uuid::now_v7();
        let path = |writer: Uuid, ordinal: &str, partition: &str| {
            format!("{root}/1/{partition}{attempt}-{ordinal}-{writer}.parquet")
        };

        let first_path = path(first_writer, "00000", "created_at_day=2026-08-29/");
        let first = ForgeOutputIdentity::validate(&first_path, &root, attempt, VERSION)
            .expect("a partitioned version path is well-formed");
        assert_eq!(first.table_version, VERSION);
        assert_eq!(first.partition_path, "created_at_day=2026-08-29");
        assert_eq!(first.writer_ordinal, 0);
        assert_eq!(first.writer_uuid, first_writer);

        let flat = path(first_writer, "00001", "");
        let unpartitioned = ForgeOutputIdentity::validate(&flat, &root, attempt, VERSION)
            .expect("an unpartitioned version path is well-formed");
        assert_eq!(unpartitioned.partition_path, "");
        assert_eq!(unpartitioned.writer_ordinal, 1);

        let second_path = path(second_writer, "00000", "created_at_day=2026-08-29/");
        let second = ForgeOutputIdentity::validate(&second_path, &root, attempt, VERSION)
            .expect("a concurrent writer restarts its own ordinal");
        assert_eq!(
            second.writer_ordinal, first.writer_ordinal,
            "per-writer ordinals reset, so a collision here is expected"
        );
        assert_ne!(
            second.writer_key(),
            first.writer_key(),
            "the writer identity is what keeps concurrent outputs distinct"
        );

        let wide = path(first_writer, "123456", "");
        assert_eq!(
            ForgeOutputIdentity::validate(&wide, &root, attempt, VERSION)
                .expect("an ordinal past five digits is still canonical")
                .writer_ordinal,
            123_456
        );

        for (bad, why) in [
            (
                format!(
                    "file:///warehouse/tenant/table/data/{attempt}-00000-{first_writer}.parquet"
                ),
                "an object outside the Forge root names no version",
            ),
            (
                format!("{root}/v1/{attempt}-00000-{first_writer}.parquet"),
                "a non-canonical version segment names no version",
            ),
            (
                format!("{root}/2/{attempt}-00000-{first_writer}.parquet"),
                "an output under another version was not selected for this one",
            ),
            (
                path(first_writer, "00000", "")
                    .replace(&attempt.to_string(), &Uuid::now_v7().to_string()),
                "an object from another attempt is not this attempt's to reclaim",
            ),
            (
                format!("{root}/1/{attempt}-00000.parquet"),
                "without a writer identity two writers would collide",
            ),
            (
                path(first_writer, "0", ""),
                "a short ordinal is a second spelling of the same value",
            ),
            (
                path(first_writer, "0000012", ""),
                "an over-padded ordinal is a second spelling of the same value",
            ),
            (
                path(first_writer, "0000a", ""),
                "a non-decimal ordinal is not an ordinal",
            ),
            (
                format!("{root}/1/{attempt}-00000-{first_writer}.avro"),
                "a rewrite output is always Parquet",
            ),
        ] {
            assert!(
                matches!(
                    ForgeOutputIdentity::validate(&bad, &root, attempt, VERSION),
                    Err(ForgeError::Invariant { .. })
                ),
                "{why}: {bad}"
            );
        }
    }

    /// Cleanup resolves an attempt out of a path it has no prior identity for.
    ///
    /// Never-published orphan cleanup sees an object before it knows which
    /// attempt produced it: that is the whole question it must answer before it
    /// may delete. This pins that the answer comes from the same grammar the
    /// writer used — `parse` recovers the attempt and `validate` is exactly
    /// `parse` plus an attempt equality check — so no second, drifting
    /// compatibility parser can appear in the cleanup owner. The refusals are
    /// the cases where accepting a loose spelling would let cleanup attribute
    /// an object to the wrong attempt, or to no attempt at all, and then delete
    /// it while its real producer is still open.
    #[test]
    fn forge_output_path_parser_matches_version_identity_grammar_exactly() {
        let root = root();
        let attempt = Uuid::now_v7();
        let writer = Uuid::now_v7();
        let path = |ordinal: &str, partition: &str| {
            format!("{root}/1/{partition}{attempt}-{ordinal}-{writer}.parquet")
        };

        for (case, partition, ordinal, expected_ordinal) in [
            ("unpartitioned", "", "00000", 0_u64),
            ("partitioned", "created_at_day=2026-08-29/", "00007", 7),
            ("wide ordinal", "", "123456", 123_456),
        ] {
            let object = path(ordinal, partition);
            let parsed = ForgeOutputIdentity::parse(&object, &root)
                .unwrap_or_else(|error| panic!("{case} path parses: {error}"));
            assert_eq!(parsed.table_version, VERSION, "{case} recovers its version");
            assert_eq!(parsed.attempt_id, attempt, "{case} recovers its attempt");
            assert_eq!(parsed.writer_ordinal, expected_ordinal, "{case} ordinal");
            assert_eq!(parsed.writer_uuid, writer, "{case} writer identity");
            assert_eq!(
                parsed.partition_path,
                partition.trim_end_matches('/'),
                "{case} partition"
            );
            assert_eq!(
                ForgeOutputIdentity::validate(&object, &root, attempt, VERSION)
                    .expect("validate accepts what parse accepted"),
                parsed,
                "{case}: validate is parse plus an attempt check, not a second grammar"
            );
            assert!(
                ForgeOutputIdentity::validate(&object, &root, Uuid::now_v7(), VERSION).is_err(),
                "{case}: validate still refuses a foreign attempt"
            );
        }

        for (bad, why) in [
            (
                format!("file:///warehouse/tenant/table/data/{attempt}-00000-{writer}.parquet"),
                "an object outside the Forge root has no version to recover",
            ),
            (
                format!("{root}/0/{attempt}-00000-{writer}.parquet"),
                "version zero is not a table version",
            ),
            (
                format!("{root}/01/{attempt}-00000-{writer}.parquet"),
                "a padded version is a second spelling of the same version",
            ),
            (
                format!("{root}/{attempt}-00000-{writer}.parquet"),
                "an object directly under the root names no version",
            ),
            (
                format!("{root}/1/{attempt}-00000.parquet"),
                "without a writer identity the object names no unique producer",
            ),
            (
                format!("{root}/1/not-a-uuid-00000-{writer}.parquet"),
                "an unreadable attempt cannot be checked against open work",
            ),
            (
                path("0", ""),
                "a short ordinal is a second spelling of the same value",
            ),
            (
                path("0000012", ""),
                "an over-padded ordinal is a second spelling of the same value",
            ),
            (path("0000a", ""), "a non-decimal ordinal is not an ordinal"),
            (
                format!("{root}/1/{attempt}-00000-{writer}.avro"),
                "a rewrite output is always Parquet",
            ),
            (
                format!("{root}/1/../{attempt}-00000-{writer}.parquet"),
                "a traversal segment escapes the version root",
            ),
        ] {
            assert!(
                matches!(
                    ForgeOutputIdentity::parse(&bad, &root),
                    Err(ForgeError::Invariant { .. })
                ),
                "{why}: {bad}"
            );
        }
    }
}
