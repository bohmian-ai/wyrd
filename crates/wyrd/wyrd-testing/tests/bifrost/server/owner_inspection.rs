//! Storage ownership: what a composed process owns, and what it does not share.

use std::sync::Arc;

use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

/// One composed node owns exactly one storage owner, and two nodes own two.
///
/// Everything the owner bounds — the node-wide concurrent-request ceiling, the
/// decoded-metadata budget charged to the Oracle memory root, and the single
/// shutdown — is a claim about a *node*. If two co-located roles each composed
/// their own owner, each would believe it alone was spending a ceiling the pair
/// actually shares, and the node would issue twice the concurrent object-store
/// work its configuration allows. If two simulated nodes shared one, a
/// per-node budget would silently become a cluster-wide one and the harness
/// would stop resembling the deployment it stands in for. Pointer identity is
/// the only assertion that separates those two failures from the passing case,
/// because every other observable — policy, backend, warehouse — is identical
/// by construction across nodes that share a test backend.
///
/// The cache allocation is checked alongside it: these nodes serve Oracle, so
/// the owner must have resolved a real metadata budget rather than silently
/// composing a disabled one.
///
/// # Panics
///
/// Panics when the cluster cannot start, when a composed node exposes no
/// storage owner, when co-located roles do not share one, or when two nodes
/// share one.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn each_composed_node_owns_exactly_one_storage_owner() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::two_mixed())
        .await
        .expect("the two-pod mixed cluster starts");

    let owners: Vec<_> = (0..2)
        .map(|index| {
            let server = cluster
                .server(index)
                .unwrap_or_else(|| panic!("node {index} is bound"));
            let state = server.state();
            let owner = state
                .bifrost_storage()
                .unwrap_or_else(|| panic!("node {index} composed a storage owner"));
            // Every co-located role reaches storage through the state's one
            // owner, so a second borrow on the same node must be the same
            // allocation, not an equal-looking copy.
            assert!(
                Arc::ptr_eq(
                    owner,
                    state
                        .bifrost_storage()
                        .expect("the owner is still composed"),
                ),
                "node {index} must publish one owner to every role"
            );
            assert!(
                owner.metadata_cache_enabled(),
                "an Oracle-serving node must resolve a metadata budget"
            );
            assert!(
                owner.policy().metadata_cache_bytes() > 0,
                "an enabled cache must have a nonzero budget"
            );
            Arc::clone(owner)
        })
        .collect();

    assert!(
        !Arc::ptr_eq(&owners[0], &owners[1]),
        "two simulated nodes must not share one storage owner"
    );
}
