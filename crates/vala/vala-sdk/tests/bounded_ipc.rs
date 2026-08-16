//! Rust-only Vala consumer proof for the shared bounded Arrow IPC owner.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use wyrd_queue::{ArrowIpcMaterialFacts, BoundedArrowIpc};

/// Client-owned test guard that records terminal release of admitted bytes.
struct ClientGuard {
    /// Bytes charged to the simulated handle-wide client budget.
    bytes: usize,
    /// Shared counter used to prove exact terminal settlement.
    released: Arc<AtomicUsize>,
}

impl Drop for ClientGuard {
    /// Releases the guard's complete charge exactly once at the terminal owner.
    fn drop(&mut self) {
        self.released.fetch_add(self.bytes, Ordering::SeqCst);
    }
}

/// Vala retains the opaque client guard while filling and transferring fixed IPC storage.
#[test]
fn bounded_ipc_retains_client_authority_through_transfer() {
    let released = Arc::new(AtomicUsize::new(0));
    let mut reserved = 0;
    let facts = ArrowIpcMaterialFacts {
        retained_input_bytes: 32,
        destination_values_bytes: 16,
        destination_offsets_bytes: 8,
        destination_validity_bits: 8,
        ipc_metadata_bytes: 9,
        ipc_body_bytes: 17,
        ipc_prefix_bytes: 8,
        ipc_alignment: 8,
    };
    let mut owned = BoundedArrowIpc::try_new(facts, |peak| {
        reserved = peak;
        Ok::<_, ()>(ClientGuard {
            bytes: peak,
            released: Arc::clone(&released),
        })
    })
    .expect("client budget admits complete peak");

    assert_eq!(reserved, owned.plan().simultaneous_peak_bytes());
    owned.ipc_mut().fill(0x5a);
    let mut transferred = transfer(owned);
    assert_eq!(released.load(Ordering::SeqCst), 0);
    assert!(transferred.ipc_mut().iter().all(|byte| *byte == 0x5a));
    drop(transferred);
    assert_eq!(released.load(Ordering::SeqCst), reserved);
}

/// Moves the shared bounded owner through the Vala consumer boundary intact.
fn transfer<G>(owned: BoundedArrowIpc<G>) -> BoundedArrowIpc<G> {
    owned
}
