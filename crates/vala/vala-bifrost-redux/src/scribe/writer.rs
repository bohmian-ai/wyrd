//! Per-tenant/table FIFO writer actors and pod-global writer registry.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::{Notify, mpsc, watch};
use uuid::Uuid;

use crate::catalog::{TenantTableBinding, TenantTableKey};
use crate::contracts::ScribeError;
use crate::scribe::admission::{AdmissionController, WriterLease};
use crate::scribe::blocking_executor::{
    ScribeBlockingExecutor, ScribeBlockingOp, ScribeBlockingResult,
};
use crate::scribe::memtable::Memtable;
use crate::scribe::preprocess::{AdmittedAppend, AppendSliceId, PreparedAppend};
use crate::scribe::telemetry::ScribeTelemetry;
use crate::scribe::wal::{ScribeAppendMeta, WalHandle, WalWriter};

const WRITER_QUEUE_CAPACITY: usize = 1_024;
const CONTROL_QUEUE_CAPACITY: usize = 256;

#[derive(Debug)]
struct WriterState {
    accepting: AtomicBool,
    healthy: AtomicBool,
    pending: AtomicUsize,
    reserved_sends: AtomicUsize,
    last_activity: Mutex<Instant>,
    stopped: Notify,
}

#[derive(Debug)]
enum WriterControl {
    PersistenceCompleted,
    GraceExpired,
    Shutdown,
}

#[derive(Debug)]
struct WriterRuntime {
    state: Arc<WriterState>,
    admission: AdmissionController,
    memtable: Arc<Memtable>,
    wal: Arc<WalWriter>,
    executor: ScribeBlockingExecutor,
    telemetry: Option<Arc<ScribeTelemetry>>,
}

/// One FIFO writer for a tenant/table binding.
#[derive(Debug)]
pub(crate) struct TenantTableWriter {
    pub(crate) key: TenantTableKey,
    pub(crate) binding: TenantTableBinding,
    pub(crate) instance_id: Uuid,
    state: Arc<WriterState>,
    data_tx: mpsc::Sender<AdmittedAppend>,
    control_tx: mpsc::Sender<WriterControl>,
    age_tx: watch::Sender<Instant>,
    _lease: WriterLease,
    _task: tokio::task::JoinHandle<()>,
}

impl TenantTableWriter {
    fn new(
        binding: TenantTableBinding,
        lease: WriterLease,
        memtable: Arc<Memtable>,
        wal: Arc<WalWriter>,
        executor: ScribeBlockingExecutor,
        admission: AdmissionController,
        telemetry: Option<Arc<ScribeTelemetry>>,
    ) -> Arc<Self> {
        let (data_tx, data_rx) = mpsc::channel(WRITER_QUEUE_CAPACITY);
        let (control_tx, control_rx) = mpsc::channel(CONTROL_QUEUE_CAPACITY);
        let (age_tx, age_rx) = watch::channel(Instant::now());
        let state = Arc::new(WriterState {
            accepting: AtomicBool::new(true),
            healthy: AtomicBool::new(true),
            pending: AtomicUsize::new(0),
            reserved_sends: AtomicUsize::new(0),
            last_activity: Mutex::new(Instant::now()),
            stopped: Notify::new(),
        });
        let runtime = WriterRuntime {
            state: Arc::clone(&state),
            admission,
            memtable,
            wal,
            executor,
            telemetry,
        };
        let task = tokio::spawn(run_writer(runtime, data_rx, control_rx, age_rx));
        let key = (binding.tenant, binding.table_ref.clone());
        Arc::new(Self {
            key,
            binding,
            instance_id: Uuid::now_v7(),
            state,
            data_tx,
            control_tx,
            age_tx,
            _lease: lease,
            _task: task,
        })
    }

    pub(crate) fn is_available(&self) -> bool {
        self.state.accepting.load(Ordering::Acquire) && self.state.healthy.load(Ordering::Acquire)
    }

    pub(crate) fn pending(&self) -> usize {
        self.state.pending.load(Ordering::Acquire)
    }

    fn is_idle(&self, now: Instant) -> bool {
        let inactive = match self.state.last_activity.lock() {
            Ok(last) => now.saturating_duration_since(*last) >= std::time::Duration::from_mins(10),
            Err(_) => false,
        };
        self.state.accepting.load(Ordering::Acquire)
            && self.state.healthy.load(Ordering::Acquire)
            && self.pending() == 0
            && self.state.reserved_sends.load(Ordering::Acquire) == 0
            && inactive
    }

    fn close_admission(&self) {
        self.state.accepting.store(false, Ordering::Release);
    }

    async fn send_control(&self, control: WriterControl) -> Result<(), ScribeError> {
        self.control_tx
            .send(control)
            .await
            .map_err(|error| ScribeError::Internal {
                detail: format!("writer control queue closed: {error}"),
            })
    }
}

/// Pod-global registry of active tenant/table writers.
#[derive(Debug)]
pub(crate) struct TenantTableWriterRegistry {
    entries: Mutex<HashMap<TenantTableKey, Arc<TenantTableWriter>>>,
    admission: AdmissionController,
    memtable: Arc<Memtable>,
    wal: Arc<WalWriter>,
    executor: ScribeBlockingExecutor,
    telemetry: Mutex<Option<Arc<ScribeTelemetry>>>,
    drained: Notify,
}

impl TenantTableWriterRegistry {
    pub(crate) fn new(
        admission: AdmissionController,
        memtable: Arc<Memtable>,
        wal: Arc<WalWriter>,
        executor: ScribeBlockingExecutor,
        telemetry: Option<Arc<ScribeTelemetry>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            entries: Mutex::new(HashMap::new()),
            admission,
            memtable,
            wal,
            executor,
            telemetry: Mutex::new(telemetry),
            drained: Notify::new(),
        })
    }

    pub(crate) fn get_or_create(
        &self,
        binding: TenantTableBinding,
    ) -> Result<(Arc<TenantTableWriter>, bool), ScribeError> {
        let key = (binding.tenant, binding.table_ref.clone());
        let mut entries = self.entries.lock().map_err(|error| ScribeError::Internal {
            detail: format!("writer registry lock poisoned: {error}"),
        })?;
        if let Some(writer) = entries.get(&key) {
            if writer.is_available() {
                return Ok((Arc::clone(writer), false));
            }
            return Err(ScribeError::IngestBusy {
                table: binding.table_ref.fqn(),
            });
        }

        // Hold the registry lock through lease acquisition and insertion. This
        // makes the first-write path one atomic get-or-create operation: racing
        // callers cannot each spawn an unregistered FIFO consumer.
        let lease = self.admission.try_reserve_writer(binding.table_ref.fqn())?;
        let telemetry = match self.telemetry.lock() {
            Ok(telemetry) => telemetry.clone(),
            Err(_) => None,
        };
        let writer = TenantTableWriter::new(
            binding,
            lease,
            Arc::clone(&self.memtable),
            Arc::clone(&self.wal),
            self.executor.clone(),
            self.admission.clone(),
            telemetry,
        );
        entries.insert(key, Arc::clone(&writer));
        Ok((writer, true))
    }

    pub(crate) fn enqueue(
        writer: &Arc<TenantTableWriter>,
        append: AdmittedAppend,
    ) -> Result<(), ScribeError> {
        if !writer.is_available() {
            return Err(ScribeError::IngestBusy {
                table: writer.binding.table_ref.fqn(),
            });
        }

        let permit = writer
            .data_tx
            .try_reserve()
            .map_err(|_| ScribeError::IngestBusy {
                table: writer.binding.table_ref.fqn(),
            })?;
        writer.state.reserved_sends.fetch_add(1, Ordering::AcqRel);
        if !writer.state.accepting.load(Ordering::Acquire) {
            writer.state.reserved_sends.fetch_sub(1, Ordering::AcqRel);
            return Err(ScribeError::IngestBusy {
                table: writer.binding.table_ref.fqn(),
            });
        }
        writer.state.pending.fetch_add(1, Ordering::AcqRel);
        permit.send(append);
        writer.state.reserved_sends.fetch_sub(1, Ordering::AcqRel);
        if let Ok(mut last_activity) = writer.state.last_activity.lock() {
            *last_activity = Instant::now();
        }
        let _ = writer.age_tx.send(Instant::now());
        Ok(())
    }

    pub(crate) fn remove_if(&self, key: &TenantTableKey, instance_id: Uuid) {
        if let Ok(mut entries) = self.entries.lock()
            && entries
                .get(key)
                .is_some_and(|writer| writer.instance_id == instance_id)
        {
            entries.remove(key);
        }
        self.drained.notify_waiters();
    }

    pub(crate) async fn drain(&self) {
        loop {
            let pending = self.entries.lock().map_or(0, |entries| {
                entries.values().map(|writer| writer.pending()).sum()
            });
            if pending == 0 {
                return;
            }
            tokio::select! {
                () = self.drained.notified() => {},
                () = tokio::time::sleep(std::time::Duration::from_millis(1)) => {},
            }
        }
    }

    pub(crate) fn writer_count(&self) -> usize {
        self.entries.lock().map_or(0, |entries| entries.len())
    }

    pub(crate) async fn shutdown(&self) {
        let writers = match self.entries.lock() {
            Ok(entries) => entries.values().cloned().collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        for writer in &writers {
            writer.close_admission();
            let _ = writer
                .send_control(WriterControl::PersistenceCompleted)
                .await;
            let _ = writer.send_control(WriterControl::GraceExpired).await;
        }
        self.drain().await;
        for writer in writers {
            let _ = writer.send_control(WriterControl::Shutdown).await;
        }
    }

    /// Retire writers that have been idle for the lifecycle grace interval.
    pub(crate) async fn retire_idle(&self, now: Instant) {
        let writers = match self.entries.lock() {
            Ok(entries) => entries.values().cloned().collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        for writer in writers {
            if !writer.is_idle(now) || self.memtable.has_state_for_table(&writer.key) {
                continue;
            }
            let stopped = writer.state.stopped.notified();
            if writer
                .state
                .accepting
                .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                continue;
            }
            if writer.send_control(WriterControl::Shutdown).await.is_err() {
                self.remove_if(&writer.key, writer.instance_id);
                continue;
            }
            stopped.await;
            self.remove_if(&writer.key, writer.instance_id);
        }
    }

    pub(crate) fn set_telemetry(&self, telemetry: Arc<ScribeTelemetry>) {
        if let Ok(mut current) = self.telemetry.lock() {
            *current = Some(telemetry);
        }
    }
}

async fn run_writer(
    runtime: WriterRuntime,
    mut data_rx: mpsc::Receiver<AdmittedAppend>,
    mut control_rx: mpsc::Receiver<WriterControl>,
    mut age_rx: watch::Receiver<Instant>,
) {
    let mut seen_slices = HashSet::<AppendSliceId>::new();
    let mut wal_handles = HashMap::<crate::scribe::seal_key::SealKey, WalHandle>::new();
    let mut controls_closed = false;
    loop {
        tokio::select! {
            biased;
            control = control_rx.recv(), if !controls_closed => {
                match control {
                    Some(WriterControl::Shutdown) => break,
                    Some(WriterControl::PersistenceCompleted | WriterControl::GraceExpired) => {},
                    None => controls_closed = true,
                }
            }
            _ = age_rx.changed() => {}
            append = data_rx.recv() => {
                let Some(append) = append else { break; };
                let result = process_append(
                    &runtime,
                    append,
                    &mut seen_slices,
                    &mut wal_handles,
                ).await;
                if let Err(error) = result {
                    runtime.state.healthy.store(false, Ordering::Release);
                    if matches!(&error, ScribeError::WalDiskFull) {
                        runtime.admission.trip_wal_disk_full();
                    }
                    tracing::error!(error = %error, "Scribe writer became unhealthy");
                }
                runtime.state.pending.fetch_sub(1, Ordering::AcqRel);
            }
            else => break,
        }
    }
    for (_, wal) in wal_handles {
        let _ = runtime
            .executor
            .submit(ScribeBlockingOp::RetireWal { wal })
            .await;
    }
    runtime.state.stopped.notify_waiters();
}

async fn process_append(
    runtime: &WriterRuntime,
    append: AdmittedAppend,
    seen_slices: &mut HashSet<AppendSliceId>,
    wal_handles: &mut HashMap<crate::scribe::seal_key::SealKey, WalHandle>,
) -> Result<(), ScribeError> {
    let preprocess_started = Instant::now();
    let ScribeBlockingResult::Prepared(prepared) = runtime
        .executor
        .submit(ScribeBlockingOp::Preprocess(Box::new(append)))
        .await?
    else {
        return Err(ScribeError::Internal {
            detail: "blocking executor returned the wrong preprocessing result".to_string(),
        });
    };
    if let Some(telemetry) = &runtime.telemetry {
        let rows = prepared
            .slices
            .iter()
            .map(|slice| slice.rows.num_rows())
            .sum();
        telemetry.record("preprocess", preprocess_started.elapsed(), rows, 0);
    }

    process_prepared(runtime, prepared, seen_slices, wal_handles).await
}

async fn process_prepared(
    runtime: &WriterRuntime,
    prepared: PreparedAppend,
    seen_slices: &mut HashSet<AppendSliceId>,
    wal_handles: &mut HashMap<crate::scribe::seal_key::SealKey, WalHandle>,
) -> Result<(), ScribeError> {
    let _request_id = prepared.request_id;
    let batch_id = *prepared.batch_id.as_bytes();
    for slice in prepared.slices {
        if !seen_slices.insert(slice.id.clone()) {
            continue;
        }
        let seal_key = slice.seal_key.clone();
        let wal_handle = if let Some(handle) = wal_handles.get(&seal_key) {
            handle.clone()
        } else {
            let handle = runtime.wal.handle_for_seal_key(seal_key.clone())?;
            wal_handles.insert(seal_key.clone(), handle.clone());
            let _ = runtime
                .executor
                .submit(ScribeBlockingOp::CreateOrRollSegment {
                    wal: handle.clone(),
                })
                .await?;
            handle
        };
        let wal_started = Instant::now();
        let ScribeBlockingResult::WalWritten {
            wal: written_wal,
            lsn,
        } = runtime
            .executor
            .submit(ScribeBlockingOp::WriteFrame {
                wal: wal_handle.clone(),
                frame: slice.wal_frame,
            })
            .await?
        else {
            return Err(ScribeError::Internal {
                detail: "blocking executor returned the wrong WAL result".to_string(),
            });
        };
        drop(written_wal);
        if let Some(telemetry) = &runtime.telemetry {
            telemetry.record(
                "wal_write",
                wal_started.elapsed(),
                slice.rows.num_rows(),
                slice.wal_bytes,
            );
        }

        let row_count = slice.rows.num_rows();
        let memtable_bytes = slice.memtable_bytes;
        let memtable_started = Instant::now();
        runtime.memtable.insert(
            &seal_key,
            slice.audit_event,
            ScribeAppendMeta {
                batch_id,
                rows_accepted: slice.rows.num_rows(),
                wal_lsn_min: lsn,
                wal_lsn_max: lsn,
                seal_key: seal_key.as_path_components(),
            },
            slice.rows,
        )?;
        if let Some(telemetry) = &runtime.telemetry {
            telemetry.record(
                "memtable_insert",
                memtable_started.elapsed(),
                row_count,
                memtable_bytes,
            );
        }

        let sync_started = Instant::now();
        match runtime
            .executor
            .submit(ScribeBlockingOp::SyncWal { wal: wal_handle })
            .await?
        {
            ScribeBlockingResult::WalSynced { wal: synced_wal } => drop(synced_wal),
            _ => {
                return Err(ScribeError::Internal {
                    detail: "blocking executor returned the wrong sync result".to_string(),
                });
            }
        }
        if let Some(telemetry) = &runtime.telemetry {
            telemetry.record("wal_sync_data", sync_started.elapsed(), 0, 0);
        }
    }
    drop(prepared.reservation);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{ArrayRef, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use arrow::record_batch::RecordBatch;
    use std::path::Path;
    use tempfile::TempDir;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::ids::DataTenantId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

    use crate::catalog::{TableRef, TenantTableBinding};
    use crate::namespaces::BifrostNamespace;
    use crate::scribe::admission::AdmissionConfig;
    use crate::scribe::blocking_executor::ScribeBlockingExecutor;

    fn wal_root(temp_dir: &TempDir) -> Arc<WalWriter> {
        Arc::new(
            WalWriter::new(
                Path::new(temp_dir.path()),
                [0_u8; 16],
                1,
                DataTenantId::SYSTEM_OWNER,
                None,
            )
            .expect("wal"),
        )
    }

    #[tokio::test]
    async fn writer_processes_one_append_in_fifo_order() {
        let temp_dir = TempDir::new().expect("temp dir");
        let memtable = Arc::new(Memtable::new());
        let admission = AdmissionController::with_config(AdmissionConfig::default());
        let executor = ScribeBlockingExecutor::with_workers(1);
        let registry = TenantTableWriterRegistry::new(
            admission.clone(),
            Arc::clone(&memtable),
            wal_root(&temp_dir),
            executor,
            None,
        );
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        let binding = TenantTableBinding::resolve((DataTenantId::SYSTEM_OWNER, table.clone()))
            .expect("binding");
        let (writer, _) = registry.get_or_create(binding).expect("writer");
        let schema = Arc::new(Schema::new(vec![Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        )]));
        let rows = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(TimestampMicrosecondArray::from(vec![1_721_003_400_000_000])) as ArrayRef,
            ],
        )
        .expect("rows");
        let reservation = admission.try_reserve(table.fqn(), 1).expect("admission");
        TenantTableWriterRegistry::enqueue(
            &writer,
            AdmittedAppend {
                request_id: RequestId::now_v7(),
                batch_id: Uuid::now_v7(),
                audit_event: AuditEvent {
                    request_id: RequestId::now_v7(),
                    trace_id: None,
                    operation: "bifrost.append".to_string(),
                    resource: table.fqn(),
                    card_ref: None,
                    principal_id: PrincipalId::new(Uuid::new_v4()),
                    principal_kind: PrincipalKindTag::User,
                    auth_method: AuthMethod::Jwt,
                    permission: "bifrost:append".to_string(),
                    decision: AuditDecision::Allow,
                    result: AuditResult::Success,
                    payload_summary: "1 rows".to_string(),
                    detail: None,
                },
                rows,
                measured_wire_bytes: 0,
                admitted_bytes: 1,
                reservation,
                tenant: DataTenantId::SYSTEM_OWNER,
                table: table.clone(),
            },
        )
        .expect("enqueue");
        registry.drain().await;

        let key = crate::scribe::seal_key::SealKey::new(
            DataTenantId::SYSTEM_OWNER,
            table,
            crate::scribe::seal_key::EventDay::new(
                chrono::NaiveDate::from_ymd_opt(2024, 7, 15).expect("date"),
            ),
        );
        assert_eq!(memtable.row_count(&key).expect("row count"), 1);
        assert_eq!(admission.snapshot().items, 0);
    }
}
