//! Storage handle shared by server-tier crates.

use crate::signer::BackendSigner;
use object_store::ObjectStore;
use std::sync::Arc;
use wyrd_spec::storage::StorageBackendKind;

/// Shared storage handle.
///
/// The handle carries both the active backend signer and the single
/// process-wide object-store substrate used by server-side readers.
#[derive(Clone)]
pub struct StorageHandle {
    signer: BackendSigner,
    object_store: Arc<dyn ObjectStore>,
}

impl std::fmt::Debug for StorageHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageHandle")
            .field("backend", &self.signer.kind())
            .field("object_store", &"<dyn ObjectStore>")
            .finish()
    }
}

impl StorageHandle {
    /// Build a storage handle from already-constructed components.
    #[must_use]
    pub fn new(signer: BackendSigner, object_store: Arc<dyn ObjectStore>) -> Self {
        Self {
            signer,
            object_store,
        }
    }

    /// Return the configured backend kind.
    #[must_use]
    pub fn backend(&self) -> StorageBackendKind {
        self.signer.kind()
    }

    /// Borrow the active backend signer.
    #[must_use]
    pub fn signer(&self) -> &BackendSigner {
        &self.signer
    }

    /// Clone the shared object-store substrate.
    #[must_use]
    pub fn object_store(&self) -> Arc<dyn ObjectStore> {
        Arc::clone(&self.object_store)
    }
}
