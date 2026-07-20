use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;
use wyrd_spec::DataTenantId;

use crate::catalog::table_ref::{TableRef, is_safe_name};
use crate::namespaces::BifrostNamespace;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForgeGroupKey {
    pub tenant: DataTenantId,
    pub table_ref: TableRef,
    pub partition_day: NaiveDate,
}

impl ForgeGroupKey {
    pub fn from_sql(
        tenant: DataTenantId,
        namespace: &str,
        table_name: &str,
        partition_day: NaiveDate,
    ) -> Result<Self, String> {
        let namespace = BifrostNamespace::from_wire(namespace)
            .ok_or_else(|| format!("unknown Bifrost namespace `{namespace}`"))?;
        if !is_safe_name(table_name) {
            return Err(format!("unsafe table name `{table_name}`"));
        }
        Ok(Self {
            tenant,
            table_ref: TableRef::new(namespace, table_name),
            partition_day,
        })
    }

    pub fn validate_sql_identity(
        &self,
        tenant: DataTenantId,
        namespace: &str,
        table_name: &str,
        partition_day: NaiveDate,
    ) -> Result<(), String> {
        let observed = Self::from_sql(tenant, namespace, table_name, partition_day)?;
        if observed == *self {
            Ok(())
        } else {
            Err(format!(
                "candidate identity does not match group `{}`",
                self.audit_resource()
            ))
        }
    }

    pub fn audit_resource(&self) -> String {
        format!(
            "bifrost://{}/{}/{}",
            self.tenant, self.table_ref.namespace, self.table_ref.name
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateFile {
    pub id: Uuid,
    pub path: String,
    pub size: u64,
    pub min_event_time: DateTime<Utc>,
    pub max_event_time: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteBin {
    pub files: Vec<CandidateFile>,
    pub total_bytes: u64,
}

pub fn stable_pack(
    mut files: Vec<CandidateFile>,
    target_bytes: u64,
    max_files: usize,
) -> Vec<RewriteBin> {
    files.sort_by_key(|file| (file.min_event_time, file.max_event_time, file.id));
    if max_files == 0 || target_bytes == 0 {
        return Vec::new();
    }

    let mut bins = Vec::new();
    let mut current = Vec::new();
    let mut current_bytes = 0_u64;
    for file in files {
        if file.size > target_bytes {
            continue;
        }
        let would_exceed = !current.is_empty()
            && (current.len() >= max_files
                || current_bytes.saturating_add(file.size) > target_bytes);
        if would_exceed {
            if current.len() >= 2 {
                bins.push(RewriteBin {
                    files: std::mem::take(&mut current),
                    total_bytes: current_bytes,
                });
            } else {
                current.clear();
            }
            current_bytes = 0;
        }
        current_bytes = current_bytes.saturating_add(file.size);
        current.push(file);
    }
    if current.len() >= 2 {
        bins.push(RewriteBin {
            files: current,
            total_bytes: current_bytes,
        });
    }
    bins
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::BifrostNamespace;

    fn file(id: u128, size: u64, min: i64, max: i64) -> CandidateFile {
        CandidateFile {
            id: Uuid::from_u128(id),
            path: format!("file-{id}"),
            size,
            min_event_time: DateTime::from_timestamp(min, 0).expect("timestamp"),
            max_event_time: DateTime::from_timestamp(max, 0).expect("timestamp"),
        }
    }

    #[test]
    fn group_key_is_exactly_tenant_table_day() {
        let tenant = DataTenantId::new_v7();
        let day = NaiveDate::from_ymd_opt(2026, 1, 2).expect("day");
        let key = ForgeGroupKey::from_sql(tenant, "vala.traces", "spans", day).expect("key");
        assert_eq!(key.tenant, tenant);
        assert_eq!(
            key.table_ref,
            TableRef::new(BifrostNamespace::Traces, "spans")
        );
        assert_eq!(key.partition_day, day);
    }

    #[test]
    fn group_key_rejects_tenant_namespace_table_or_day_mismatch() {
        let tenant = DataTenantId::new_v7();
        let other = DataTenantId::new_v7();
        let day = NaiveDate::from_ymd_opt(2026, 1, 2).expect("day");
        let key = ForgeGroupKey::from_sql(tenant, "vala.traces", "spans", day).expect("key");
        for observed in [
            (other, "vala.traces", "spans", day),
            (tenant, "vala.logs", "spans", day),
            (tenant, "vala.traces", "events", day),
            (
                tenant,
                "vala.traces",
                "spans",
                day.succ_opt().expect("next day"),
            ),
        ] {
            assert!(
                key.validate_sql_identity(observed.0, observed.1, observed.2, observed.3)
                    .is_err()
            );
        }
    }

    #[test]
    fn binpack_is_stable_and_never_exceeds_target() {
        let files = vec![file(3, 4, 2, 3), file(1, 4, 0, 1), file(2, 4, 1, 2)];
        let bins = stable_pack(files, 8, 10);
        assert_eq!(bins.len(), 1);
        assert_eq!(
            bins[0].files.iter().map(|f| f.id).collect::<Vec<_>>(),
            vec![Uuid::from_u128(1), Uuid::from_u128(2)]
        );
        assert!(bins.iter().all(|bin| bin.total_bytes <= 8));
    }

    #[test]
    fn binpack_skips_singletons_and_preserves_interval_order() {
        let files = vec![file(1, 9, 0, 1), file(2, 4, 2, 3), file(3, 4, 4, 5)];
        let bins = stable_pack(files, 8, 2);
        assert_eq!(bins.len(), 1);
        assert_eq!(
            bins[0].files.iter().map(|f| f.id).collect::<Vec<_>>(),
            vec![Uuid::from_u128(2), Uuid::from_u128(3)]
        );
    }
}
