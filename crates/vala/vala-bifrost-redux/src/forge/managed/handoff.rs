//! The complete, exact result one managed rewrite hands to publication.
//!
//! A handoff is deliberately narrow. It names the snapshot the rewrite was
//! planned against, the live files it consumed, and the objects it produced —
//! and nothing else. It carries no catalog handle, no commit permission, and no
//! transaction: whether these objects are ever published is a separate decision
//! made by a separate owner against the same base snapshot.

use iceberg::spec::DataFile;

use crate::forge::error::ForgeError;

/// Everything publication needs, and nothing it does not.
///
/// The five fields are the complete contract. `rewritten_data_files` and the
/// two delete lists are the live paths this rewrite consumed, which is what a
/// publication needs in order to remove exactly what was replaced;
/// `output_data_files` are the core's own descriptors for the objects it wrote,
/// carried through untouched because their column statistics, partition values,
/// and sizes are measurements the core made and this owner has no better
/// source for.
#[derive(Debug, Clone)]
pub struct RewriteHandoff {
    /// Snapshot the plan and every consumed path were resolved against.
    pub base_snapshot_id: i64,
    /// Live data-file paths this rewrite consumed and replaced.
    pub rewritten_data_files: Vec<String>,
    /// Live position-delete paths whose effect is now materialized in the outputs.
    pub applied_position_delete_files: Vec<String>,
    /// Live equality-delete paths whose effect is now materialized in the outputs.
    pub applied_equality_delete_files: Vec<String>,
    /// Core-produced descriptors for the objects this rewrite wrote.
    pub output_data_files: Vec<DataFile>,
}

impl RewriteHandoff {
    /// Assembles one handoff after proving its identities are consistent.
    ///
    /// The checks are cheap and the failures they catch are not: a duplicated
    /// input path would ask publication to remove the same file twice, and an
    /// output path that is also an input would ask it to remove a file the
    /// rewrite just wrote. Both are silent data loss at commit time, so they
    /// are refused here where the only cost is a failed attempt.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the base snapshot is not a
    /// positive Iceberg snapshot id, when the rewrite consumed no data file,
    /// when it produced no output, when any path appears twice across the
    /// consumed sets, or when a produced object path is also a consumed path.
    pub(crate) fn try_new(
        base_snapshot_id: i64,
        rewritten_data_files: Vec<String>,
        applied_position_delete_files: Vec<String>,
        applied_equality_delete_files: Vec<String>,
        output_data_files: Vec<DataFile>,
    ) -> Result<Self, ForgeError> {
        let invariant = |detail: String| ForgeError::Invariant { detail };
        if base_snapshot_id <= 0 {
            return Err(invariant(format!(
                "rewrite handoff carries a non-snapshot base id {base_snapshot_id}"
            )));
        }
        if rewritten_data_files.is_empty() {
            return Err(invariant(
                "rewrite handoff consumed no data file".to_owned(),
            ));
        }
        if output_data_files.is_empty() {
            return Err(invariant(
                "rewrite handoff produced no output object".to_owned(),
            ));
        }
        let mut consumed = std::collections::BTreeSet::new();
        for path in rewritten_data_files
            .iter()
            .chain(&applied_position_delete_files)
            .chain(&applied_equality_delete_files)
        {
            if !consumed.insert(path.as_str()) {
                return Err(invariant(format!(
                    "rewrite handoff names consumed path {path} more than once"
                )));
            }
        }
        let mut produced = std::collections::BTreeSet::new();
        for file in &output_data_files {
            let path = file.file_path();
            if !produced.insert(path) {
                return Err(invariant(format!(
                    "rewrite handoff names produced path {path} more than once"
                )));
            }
            if consumed.contains(path) {
                return Err(invariant(format!(
                    "rewrite handoff names {path} as both consumed and produced"
                )));
            }
        }
        Ok(Self {
            base_snapshot_id,
            rewritten_data_files,
            applied_position_delete_files,
            applied_equality_delete_files,
            output_data_files,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one core-shaped output descriptor at `path`.
    ///
    /// # Panics
    ///
    /// Panics when the descriptor cannot be built, which is a fixture
    /// construction invariant rather than an input.
    fn output(path: &str) -> DataFile {
        iceberg::spec::DataFileBuilder::default()
            .content(iceberg::spec::DataContentType::Data)
            .file_path(path.to_owned())
            .file_format(iceberg::spec::DataFileFormat::Parquet)
            .partition(iceberg::spec::Struct::empty())
            .record_count(1)
            .file_size_in_bytes(1)
            .partition_spec_id(0)
            .build()
            .expect("fixture output descriptor")
    }

    /// The handoff is exactly five fields and nothing else can be added silently.
    ///
    /// The destructuring binding is the proof: it names every field, so a sixth
    /// field added to the struct stops this test compiling rather than shipping
    /// as an unreviewed part of the publication contract. The source assertion
    /// covers the other direction, over code lines only: the handoff must not
    /// acquire a catalog, a transaction, or publication authority, which is
    /// what would turn it from a description of a result into a permission.
    #[test]
    fn forge_rewrite_handoff_accepts_exact_five_fields_only() {
        const SOURCE: &str = include_str!("handoff.rs");

        let handoff = RewriteHandoff::try_new(
            7,
            vec!["a.parquet".to_owned()],
            vec!["p.parquet".to_owned()],
            vec!["e.parquet".to_owned()],
            vec![output("out.parquet")],
        )
        .expect("a consistent handoff");
        let RewriteHandoff {
            base_snapshot_id,
            rewritten_data_files,
            applied_position_delete_files,
            applied_equality_delete_files,
            output_data_files,
        } = handoff;
        assert_eq!(base_snapshot_id, 7);
        assert_eq!(rewritten_data_files, vec!["a.parquet".to_owned()]);
        assert_eq!(applied_position_delete_files, vec!["p.parquet".to_owned()]);
        assert_eq!(applied_equality_delete_files, vec!["e.parquet".to_owned()]);
        assert_eq!(output_data_files.len(), 1);
        assert_eq!(output_data_files[0].file_path(), "out.parquet");

        let body = SOURCE
            .split_once("#[cfg(test)]")
            .expect("the module has a test section")
            .0
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<String>();
        for forbidden in [
            "Catalog",
            "Transaction",
            "update_table",
            "commit",
            "snapshot_produce",
        ] {
            assert!(
                !body.contains(forbidden),
                "the handoff must not name '{forbidden}'"
            );
        }
    }

    /// Every way one handoff could describe two things at once is refused.
    ///
    /// Each case is a distinct failure at publication time — removing a file
    /// twice, removing a file that was just written, or publishing against a
    /// base that is not a snapshot — and each is cheaper to refuse here than to
    /// diagnose from a committed table.
    #[test]
    fn forge_rewrite_handoff_rejects_duplicate_or_contradictory_identities() {
        let consistent = |rewritten: Vec<String>, deletes: Vec<String>, out: Vec<DataFile>| {
            RewriteHandoff::try_new(7, rewritten, deletes, Vec::new(), out)
        };
        assert!(
            matches!(
                consistent(
                    vec!["a.parquet".to_owned(), "a.parquet".to_owned()],
                    Vec::new(),
                    vec![output("out.parquet")]
                ),
                Err(ForgeError::Invariant { .. })
            ),
            "one consumed path named twice would be removed twice"
        );
        assert!(
            matches!(
                consistent(
                    vec!["a.parquet".to_owned()],
                    vec!["a.parquet".to_owned()],
                    vec![output("out.parquet")]
                ),
                Err(ForgeError::Invariant { .. })
            ),
            "a path cannot be both a consumed data file and a consumed delete"
        );
        assert!(
            matches!(
                consistent(
                    vec!["a.parquet".to_owned()],
                    Vec::new(),
                    vec![output("out.parquet"), output("out.parquet")]
                ),
                Err(ForgeError::Invariant { .. })
            ),
            "one produced path named twice would be published twice"
        );
        assert!(
            matches!(
                consistent(
                    vec!["a.parquet".to_owned()],
                    Vec::new(),
                    vec![output("a.parquet")]
                ),
                Err(ForgeError::Invariant { .. })
            ),
            "a produced object must never also be scheduled for removal"
        );
        assert!(
            matches!(
                RewriteHandoff::try_new(
                    0,
                    vec!["a.parquet".to_owned()],
                    Vec::new(),
                    Vec::new(),
                    vec![output("out.parquet")]
                ),
                Err(ForgeError::Invariant { .. })
            ),
            "publication needs a real base snapshot to validate against"
        );
        assert!(
            matches!(
                consistent(Vec::new(), Vec::new(), vec![output("out.parquet")]),
                Err(ForgeError::Invariant { .. })
            ),
            "a rewrite that consumed nothing replaced nothing"
        );
        assert!(
            matches!(
                consistent(vec!["a.parquet".to_owned()], Vec::new(), Vec::new()),
                Err(ForgeError::Invariant { .. })
            ),
            "a rewrite that produced nothing has nothing to hand off"
        );
    }
}
