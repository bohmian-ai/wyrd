//! Task-local observer scope.

use std::future::Future;
use std::sync::Arc;

use crate::Observer;

tokio::task_local! {
    pub(crate) static SCOPED_OBSERVER: Arc<dyn Observer>;
}

/// Run a future with `observer` as the scoped observer for the current task.
///
/// The override applies only while the supplied future is awaited on this task.
/// Detached `tokio::spawn` child tasks do not inherit it; callers must pass the
/// observer explicitly or wrap the child future in another scope.
pub async fn with_observer<F>(observer: Arc<dyn Observer>, future: F) -> F::Output
where
    F: Future,
{
    SCOPED_OBSERVER.scope(observer, future).await
}
