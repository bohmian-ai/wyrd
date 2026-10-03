//! Tenant-authorized logical lifecycle controls over all current Oracle owners.

use std::collections::BTreeMap;
use std::sync::Arc;

use futures_util::StreamExt as _;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::oracle::{OracleQueryStream, RunningQueryRegistry};
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{
    CancelOracleLifecycleRequest, CancelOracleLifecycleResponse, ListOracleLifecyclesRequest,
    OracleLifecycleLookupRequest, QueryStreamFrame, QueryTerminalFrame, QueryTerminalOutcome,
    RunningQuerySummary,
};

use super::{OracleLifecycleOutcome, OracleLifecycleTransport};
use vala_bifrost_redux::cluster::ClusterRegistry;

/// One logical lifecycle policy owner over local and current-ready peers.
#[derive(Clone)]
pub struct RunningQueryControls {
    /// Exact local owner registry, absent on forwarding-only ingress.
    registry: Option<Arc<RunningQueryRegistry>>,
    /// Canonical authenticated current-ready remote transport.
    lifecycle_transport: Arc<OracleLifecycleTransport>,
    /// Shared current-ready cluster membership owner.
    cluster: Arc<ClusterRegistry>,
}

impl RunningQueryControls {
    /// Cancels one trusted query and retains its response until the owner settles.
    ///
    /// Local signaling is immediate; authenticated remote cancellation runs once
    /// alongside response draining. Only a validated terminal proves settlement.
    /// Failed terminals may retain cleanup residue; successful terminals require
    /// clean EOF. Neither control acknowledgement nor owner absence proves cleanup.
    /// Dropping this future abandons confirmation without retrying cancellation.
    /// Frames come from the Oracle encoder or validated forwarding converter;
    /// this drain retains no second Arrow result or independent row accounting.
    ///
    /// # Errors
    /// Returns incomplete-stream on deadline/transport/EOF loss, control-unavailable
    /// if routing failed without terminal proof, or protocol error for invalid
    /// terminal metadata or frames following a provisional success.
    pub async fn cancel_and_settle(
        &self,
        tenant_id: DataTenantId,
        request_id: RequestId,
        mut stream: OracleQueryStream,
        mut terminal: Option<QueryTerminalFrame>,
    ) -> Result<(), WyrdError> {
        stream.request_cancel();
        let remaining = u64::try_from(
            stream
                .deadline_ms
                .saturating_sub(chrono::Utc::now().timestamp_millis()),
        )
        .unwrap_or(0);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(remaining);
        tokio::time::timeout_at(deadline, async {
            if let Some(observed) = &terminal {
                observed
                    .validate()
                    .map_err(|_| BifrostError::QueryStreamProtocol)?;
                if observed.outcome == QueryTerminalOutcome::Failed {
                    return Ok(());
                }
            }
            let cancel = self.cancel(tenant_id, request_id);
            tokio::pin!(cancel);
            let mut control_failed = None;
            loop {
                let frame = tokio::select! {
                    biased;
                    outcome = &mut cancel, if control_failed.is_none() => {
                        control_failed = Some(outcome.is_err());
                        continue;
                    }
                    frame = stream.frames.next() => frame,
                };
                match frame {
                    None if terminal.is_some() => return Ok(()),
                    None | Some(Err(_)) => {
                        let failed = match control_failed {
                            Some(failed) => failed,
                            None => cancel.await.is_err(),
                        };
                        return Err(if failed {
                            BifrostError::RunningQueryControlUnavailable
                        } else {
                            BifrostError::QueryStreamIncomplete
                        }
                        .into());
                    }
                    Some(Ok(_)) if terminal.is_some() => {
                        return Err(BifrostError::QueryStreamProtocol.into());
                    }
                    Some(Ok(QueryStreamFrame::Terminal(observed))) => {
                        observed
                            .validate()
                            .map_err(|_| BifrostError::QueryStreamProtocol)?;
                        if observed.outcome == QueryTerminalOutcome::Failed {
                            return Ok(());
                        }
                        terminal = Some(observed);
                    }
                    Some(Ok(QueryStreamFrame::Schema(_) | QueryStreamFrame::Batch(_))) => {}
                }
            }
        })
        .await
        .map_err(|_| WyrdError::from(BifrostError::QueryStreamIncomplete))?
    }

    /// Awaits a query's stream `open`, cancelling the query at its owner if
    /// `cancel` fires first.
    ///
    /// An Analytical leader opens its stream only with its first batch, so a
    /// cancellation that waited for the open would wait for the query itself.
    /// The cancellation is routed to the request's registered owner through
    /// [`Self::cancel`] and the same open is still awaited rather than dropped,
    /// so whatever it returns is settled by the caller instead of being
    /// abandoned mid-flight. A query its owner has not registered yet is not
    /// found; the caller cancels it through [`Self::cancel_and_settle`] once its
    /// stream opens.
    ///
    /// # Errors
    /// Returns the open's own error; a refused cancellation is not an error.
    pub async fn open_cancellable(
        &self,
        open: impl Future<Output = Result<OracleQueryStream, WyrdError>>,
        tenant_id: DataTenantId,
        request_id: &RequestId,
        cancel: &CancellationToken,
    ) -> Result<OracleQueryStream, WyrdError> {
        cancel_while_opening(open, self.cancel(tenant_id, request_id.clone()), cancel).await
    }

    /// Creates the facade from explicit process-owned dependencies.
    /// A forwarding-only ingress passes no registry and contributes no local owner.
    #[must_use]
    pub fn new(
        registry: Option<Arc<RunningQueryRegistry>>,
        lifecycle_transport: Arc<OracleLifecycleTransport>,
        cluster: Arc<ClusterRegistry>,
    ) -> Self {
        Self {
            registry,
            lifecycle_transport,
            cluster,
        }
    }

    /// Lists all current-ready owners for the authenticated tenant.
    ///
    /// # Errors
    /// Returns a stable permission, audit, contradiction, or unavailable error.
    pub async fn list(
        &self,
        tenant_id: DataTenantId,
    ) -> Result<Vec<RunningQuerySummary>, WyrdError> {
        let _membership = self.cluster.snapshot();
        let remote = self
            .lifecycle_transport
            .list(ListOracleLifecyclesRequest { tenant_id })
            .await;
        let mut merged = BTreeMap::new();
        for summary in self
            .registry
            .iter()
            .flat_map(|registry| registry.list(tenant_id))
        {
            merged.insert(summary.request_id.as_str().to_owned(), summary);
        }
        for node in remote {
            match node.outcome {
                OracleLifecycleOutcome::Found(summaries) => {
                    for summary in summaries {
                        if let Some(current) = merged.get(summary.request_id.as_str())
                            && current != &summary
                        {
                            return Err(BifrostError::RunningQueryConflict.into());
                        }
                        merged.insert(summary.request_id.as_str().to_owned(), summary);
                    }
                }
                OracleLifecycleOutcome::Absent => {}
                OracleLifecycleOutcome::Unavailable => {
                    return Err(BifrostError::RunningQueryControlUnavailable.into());
                }
            }
        }
        Ok(merged.into_values().collect())
    }

    /// Gets the sole current-ready owner for one authenticated request.
    ///
    /// # Errors
    /// Returns the locked permission, audit, not-found, conflict, or unavailable error.
    pub async fn get(
        &self,
        tenant_id: DataTenantId,
        request_id: RequestId,
    ) -> Result<RunningQuerySummary, WyrdError> {
        let _membership = self.cluster.snapshot();
        let lookup = OracleLifecycleLookupRequest {
            tenant_id,
            request_id: request_id.clone(),
        };
        let remote = self.lifecycle_transport.get(lookup).await;
        let mut owners = self
            .registry
            .iter()
            .flat_map(|registry| registry.list(tenant_id))
            .filter(|summary| summary.request_id == request_id)
            .collect::<Vec<_>>();
        for node in remote {
            match node.outcome {
                OracleLifecycleOutcome::Found(summary) => owners.push(summary),
                OracleLifecycleOutcome::Absent => {}
                OracleLifecycleOutcome::Unavailable => {
                    return Err(BifrostError::RunningQueryControlUnavailable.into());
                }
            }
        }
        match owners.len() {
            0 => Err(BifrostError::RunningQueryNotFound.into()),
            1 => Ok(owners.remove(0)),
            _ => Err(BifrostError::RunningQueryConflict.into()),
        }
    }

    /// Cancels the sole current-ready owner without retrying ambiguous mutation.
    ///
    /// # Errors
    /// Returns the locked permission, audit, not-found, conflict, or unavailable error.
    pub async fn cancel(
        &self,
        tenant_id: DataTenantId,
        request_id: RequestId,
    ) -> Result<CancelOracleLifecycleResponse, WyrdError> {
        let _membership = self.cluster.snapshot();
        let request = CancelOracleLifecycleRequest {
            tenant_id,
            request_id: request_id.clone(),
        };
        let remote = self.lifecycle_transport.cancel(request).await;
        let mut owners = Vec::new();
        if let Some(cancelled) = self
            .registry
            .as_ref()
            .and_then(|registry| registry.cancel(tenant_id, &request_id))
        {
            owners.push(CancelOracleLifecycleResponse {
                request_id: cancelled.request_id,
                cancellation_started: cancelled.cancellation_started,
            });
        }
        for node in remote {
            match node.outcome {
                OracleLifecycleOutcome::Found(response) => owners.push(response),
                OracleLifecycleOutcome::Absent => {}
                OracleLifecycleOutcome::Unavailable => {
                    return Err(BifrostError::RunningQueryControlUnavailable.into());
                }
            }
        }
        match owners.len() {
            0 => Err(BifrostError::RunningQueryNotFound.into()),
            1 => Ok(owners.remove(0)),
            _ => Err(BifrostError::RunningQueryConflict.into()),
        }
    }
}

/// Awaits `open`; if `cancel` fires first, awaits `cancel_owner` once and then
/// the same `open` to completion.
///
/// The open is never dropped, so its result always reaches the caller. The
/// cancellation request's own refusal is only logged, because the caller
/// settles whatever the open returns.
///
/// # Errors
/// Returns the open's own error.
async fn cancel_while_opening<T, C>(
    open: impl Future<Output = Result<T, WyrdError>>,
    cancel_owner: impl Future<Output = Result<C, WyrdError>>,
    cancel: &CancellationToken,
) -> Result<T, WyrdError> {
    tokio::pin!(open);
    tokio::select! {
        biased;
        opened = &mut open => return opened,
        () = cancel.cancelled() => {}
    }
    if let Err(error) = cancel_owner.await {
        tracing::debug!(
            code = error.code(),
            "the opening query had no cancellable owner yet"
        );
    }
    open.await
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use tokio_util::sync::CancellationToken;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::vala::BifrostError;

    use super::cancel_while_opening;

    /// An open that finishes first is returned without any cancellation; a
    /// cancellation during the open is requested once and the same open still
    /// completes and is returned, even when the owner refuses the request.
    ///
    /// # Panics
    /// Panics if an uncancelled open requests cancellation, a cancelled open
    /// is dropped instead of completed, or a refused request hides the open.
    #[tokio::test]
    async fn cancel_while_opening_requests_cancellation_and_keeps_the_open() {
        let requested = AtomicBool::new(false);
        let owner = || async {
            requested.store(true, Ordering::Release);
            Ok::<(), WyrdError>(())
        };
        let opened = cancel_while_opening(async { Ok(7) }, owner(), &CancellationToken::new())
            .await
            .expect("an uncancelled open returns its stream");
        assert_eq!(opened, 7);
        assert!(!requested.load(Ordering::Acquire));

        // The open completes only after the owner was asked to cancel, so a
        // dropped open or a skipped request cannot return its result.
        let cancel = CancellationToken::new();
        let (opening, open) = tokio::sync::oneshot::channel::<u32>();
        let (asked, request) = tokio::sync::oneshot::channel::<()>();
        let open = async { Ok(open.await.expect("the test sends the open result")) };
        let owner = async {
            asked.send(()).expect("the test awaits the request");
            Ok::<(), WyrdError>(())
        };
        let signal = async {
            cancel.cancel();
            request.await.expect("cancellation reaches the owner");
            opening.send(9).expect("the open is still awaited");
        };
        let (opened, ()) = tokio::join!(cancel_while_opening(open, owner, &cancel), signal);
        assert_eq!(opened.expect("a cancelled open still returns"), 9);

        let refused = async { Err::<(), WyrdError>(BifrostError::RunningQueryNotFound.into()) };
        let cancel = CancellationToken::new();
        cancel.cancel();
        let opened = cancel_while_opening(async { Ok(3) }, refused, &cancel).await;
        assert_eq!(opened.expect("a refused cancellation keeps the open"), 3);
    }
}
