//! A verification's target, read in one round trip.
//!
//! Every Verifier execution, direct or queued, needs its Verifier Card, that
//! Verifier's readiness, its subject Card, and the tenant's SYSTEM principal
//! before anything runs. [`VerificationTargets`] reads all of them through
//! `wyrd.verification_target`, which binds its own tenant for RLS, so the read
//! runs as one statement either on a plain connection with no transaction
//! around it, like Oracle's table cut, or inside a transaction already bound
//! to the same tenant.
// raw-query grep allowlist: the function post-dates the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sqlx::FromRow;
use sqlx::types::Uuid;
use wyrd_spec::DataTenantId;
use wyrd_spec::ids::CardUid;
use wyrd_spec::verification::VerifierReadiness;

use crate::queries::verifier_executions::EnqueueRefusal;
use crate::row_types::cards::CardRow;

/// The rows a verification resolves before it runs.
///
/// A Card is `None` when no non-deleted Card has that UID in the tenant, so a
/// present Card is active or deprecated.
#[derive(Debug)]
pub struct VerificationTarget {
    /// The Verifier Card the request names.
    pub verifier: Option<CardRow>,
    /// `wyrd.verifier_readiness` of the Verifier: the one rule deciding
    /// whether it can run; [`VerifierReadiness::VerifierUnavailable`] when
    /// neither Card exists.
    pub readiness: VerifierReadiness,
    /// The Card under verification.
    pub subject: Option<CardRow>,
    /// The tenant's active SYSTEM principal, which executes the Verifier.
    pub system_principal: Option<Uuid>,
}

impl VerificationTarget {
    /// Why this target cannot run, or `None` when it can.
    ///
    /// Applies the database's answer in the precedence acceptance and every
    /// execution share: a Verifier that is not ready or absent, then an
    /// absent subject. Pure: no IO.
    #[must_use]
    pub fn refusal(&self) -> Option<EnqueueRefusal> {
        if self.readiness != VerifierReadiness::Ready {
            return Some(EnqueueRefusal::NotReady(self.readiness));
        }
        if self.verifier.is_none() {
            return Some(EnqueueRefusal::NotReady(
                VerifierReadiness::VerifierUnavailable,
            ));
        }
        self.subject
            .is_none()
            .then_some(EnqueueRefusal::SubjectUnavailable)
    }
}

/// One `wyrd.verification_target` row: a Card and the tenant's principal.
#[derive(FromRow)]
struct TargetRow {
    /// One of the two requested Cards.
    #[sqlx(flatten)]
    card: CardRow,
    /// The Verifier's stored [`VerifierReadiness`] name, repeated on every row.
    readiness: String,
    /// The tenant's active SYSTEM principal, repeated on every row.
    system_principal_id: Option<Uuid>,
}

/// Reads verification targets for one tenant on one connection.
///
/// The statement binds its tenant for RLS itself, so it runs as one
/// autocommit round trip on a plain connection, or inside the caller's
/// transaction bound to the same tenant; it takes no lock and writes nothing,
/// so nothing is released afterwards.
pub struct VerificationTargets<'conn> {
    /// Connection the statement runs on.
    conn: &'conn mut sqlx::PgConnection,
    /// Tenant the statement binds, scoping every row it reads.
    data_tenant_id: DataTenantId,
}

impl<'conn> VerificationTargets<'conn> {
    /// Binds the owner to one connection and the tenant it reads for.
    ///
    /// `conn` must not be inside a transaction bound to another tenant; the
    /// statement refuses that rather than rebinding.
    pub fn new(conn: &'conn mut sqlx::PgConnection, data_tenant_id: DataTenantId) -> Self {
        Self {
            conn,
            data_tenant_id,
        }
    }

    /// Reads the Verifier Card, its readiness, the subject Card, and the
    /// SYSTEM principal in one statement.
    ///
    /// When both UIDs name the same Card, it fills both sides. The readiness
    /// and principal are known only when at least one Card exists, which is
    /// the only case a caller can use them.
    ///
    /// # Errors
    /// Returns the database error when the statement fails, including the
    /// refusal of a connection already bound to another tenant, or a decode
    /// error when the stored readiness is unknown.
    pub async fn read(
        &mut self,
        verifier: &CardUid,
        subject: &CardUid,
    ) -> Result<VerificationTarget, sqlx::Error> {
        let rows: Vec<TargetRow> =
            sqlx::query_as("SELECT * FROM wyrd.verification_target($1, $2, $3)")
                .bind(self.data_tenant_id.as_uuid())
                .bind(verifier.as_uuid())
                .bind(subject.as_uuid())
                .fetch_all(&mut *self.conn)
                .await?;
        let mut target = VerificationTarget {
            verifier: None,
            readiness: VerifierReadiness::VerifierUnavailable,
            subject: None,
            system_principal: None,
        };
        for row in rows {
            target.readiness = row
                .readiness
                .parse()
                .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
            target.system_principal = row.system_principal_id;
            let uid = row.card.card_uid;
            if uid == verifier.as_uuid() {
                if uid == subject.as_uuid() {
                    // A Verifier verifying itself: the one row fills both sides.
                    target.subject = Some(row.card.clone());
                }
                target.verifier = Some(row.card);
            } else {
                target.subject = Some(row.card);
            }
        }
        Ok(target)
    }
}
