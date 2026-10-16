//! Workspace admission, which opens through the execution's evidence
//! reader and therefore the shared Postgres fixture.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;
use vala_eval::blocks::{CallInput, CallValue};

use super::*;
use crate::test_support::{test_caller, test_state};

/// Dropping a workspace removes its directory and returns its admission.
///
/// # Panics
/// Panics when the directory or permit outlives the workspace.
#[test]
fn workspace_cleans_up_on_drop() {
    wyrd_runtime::runtime().block_on(async {
        let limits = HostedLimits {
            max_workspaces: 1,
            ..HostedLimits::default()
        };
        let evidence =
            TenantEvidence::new(test_state().await, test_caller([]), BTreeMap::new(), limits);
        let workspaces = Workspaces::new(limits);
        let files = BTreeMap::from([(
            "/inputs/a.json".to_owned(),
            CallInput {
                value: CallValue::Json(json!({"a": 1})),
                sources: BTreeSet::new(),
            },
        )]);

        let workspace = workspaces.open(&files, &evidence).await.expect("opens");
        let dir = workspace.inputs_dir();
        assert!(dir.join("a.json").is_file());
        assert_eq!(workspaces.permits.available_permits(), 0);
        drop(workspace);
        assert!(!dir.exists());
        assert_eq!(workspaces.permits.available_permits(), 1);
    });
}
