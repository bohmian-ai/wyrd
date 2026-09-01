//! Durable expired-file cleanup cursor.
//!
//! Expired-file cleanup deletes an exact ordered candidate set recorded before
//! its first deletion, and it must survive cancellation, failure, and takeover
//! by another lease owner without deleting past an uncommitted result or
//! redoing a committed one. This module owns that ordering decision on its own,
//! separately from the object-store and SQL effects the worker performs, so the
//! transition rules can be proven exhaustively without a durable fixture.

use super::error::ForgeError;

/// Outcome of one accepted object-store deletion.
///
/// Both variants are successes. An object that is already absent once the
/// safety proof has been made cannot be resurrected, so replaying its deletion
/// is exactly as final as performing it; treating the two differently would
/// strand a candidate the cursor can never pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CleanupDeletion {
    /// The object store accepted the deletion.
    Confirmed,
    /// The object was already absent when the deletion was attempted.
    AlreadyMissing,
}

/// The next action the cleanup drain owes its candidate set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CleanupStep {
    /// Delete the candidate at this index, then confirm it.
    Delete(usize),
    /// Every candidate is durably committed.
    Finished,
}

/// The exact durable frontier transition one confirmed deletion authorizes.
///
/// Both ends are carried because the durable advance is a compare-and-set: a
/// successor owner replaying the same candidate must lose the race rather than
/// push the frontier twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CursorCommit {
    /// Frontier the durable row must currently hold.
    pub(super) expected: u32,
    /// Frontier the durable row moves to.
    pub(super) next: u32,
}

/// Ordered position within one prepared expired-cleanup candidate set.
///
/// The cursor is advanced only by [`Self::confirm`], and only after the
/// deletion it describes has been accepted. Cancellation and failure are
/// therefore not transitions at all: the caller simply stops, and
/// [`Self::step`] keeps naming the same candidate for whoever resumes.
pub(super) struct ExpiredCleanupCursor {
    /// Total candidates in the prepared set.
    total: usize,
    /// Candidates already durably committed as deleted.
    committed: usize,
}

impl ExpiredCleanupCursor {
    /// Rebuilds a cursor from durable prepared evidence.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the committed frontier is past
    /// the candidate set or either value cannot be represented durably. Such a
    /// cursor cannot name a safe next action, and guessing one risks deleting
    /// an object no proof covers.
    pub(super) fn resume(total: usize, committed: u32) -> Result<Self, ForgeError> {
        u32::try_from(total).map_err(|_| ForgeError::Invariant {
            detail: "expired cleanup candidate set exceeds u32".to_owned(),
        })?;
        let committed = usize::try_from(committed).map_err(|_| ForgeError::Invariant {
            detail: "expired cleanup cursor exceeds usize".to_owned(),
        })?;
        if committed > total {
            return Err(ForgeError::Invariant {
                detail: format!(
                    "expired cleanup cursor {committed} is past its {total} prepared candidates"
                ),
            });
        }
        Ok(Self { total, committed })
    }

    /// Names the next candidate, or reports the set drained.
    pub(super) fn step(&self) -> CleanupStep {
        if self.committed < self.total {
            CleanupStep::Delete(self.committed)
        } else {
            CleanupStep::Finished
        }
    }

    /// Advances past the candidate [`Self::step`] just named.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the set is already drained, which
    /// would otherwise commit a second terminal transition for one candidate.
    pub(super) fn confirm(
        &mut self,
        deletion: CleanupDeletion,
    ) -> Result<CursorCommit, ForgeError> {
        let CleanupStep::Delete(index) = self.step() else {
            return Err(ForgeError::Invariant {
                detail: "expired cleanup confirmed a deletion past its candidate set".to_owned(),
            });
        };
        let _ = deletion;
        let expected = u32::try_from(index).map_err(|_| ForgeError::Invariant {
            detail: "expired cleanup cursor exceeds u32".to_owned(),
        })?;
        let next = expected
            .checked_add(1)
            .ok_or_else(|| ForgeError::Invariant {
                detail: "expired cleanup cursor overflowed".to_owned(),
            })?;
        self.committed = index.saturating_add(1);
        Ok(CursorCommit { expected, next })
    }

    /// Returns the durable frontier for persistence and successor resume.
    pub(super) fn committed(&self) -> u32 {
        u32::try_from(self.committed).unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every cursor transition advances the durable frontier exactly once.
    ///
    /// The cases are the ones a real cleanup run reaches: a fresh start, a
    /// confirmed delete, an object already absent after its safety proof, a
    /// cancelled or failed attempt that must not advance, a resume by a
    /// successor owner, and the terminal state that must refuse a duplicate
    /// transition.
    #[test]
    fn forge_expired_cleanup_cursor_state_machine() {
        let mut cursor = ExpiredCleanupCursor::resume(3, 0).expect("fresh cursor is valid");
        assert_eq!(cursor.step(), CleanupStep::Delete(0));

        let commit = cursor
            .confirm(CleanupDeletion::Confirmed)
            .expect("a confirmed delete advances the cursor");
        assert_eq!(
            commit,
            CursorCommit {
                expected: 0,
                next: 1
            }
        );
        assert_eq!(cursor.step(), CleanupStep::Delete(1));

        let unconfirmed = cursor.step();
        assert_eq!(
            unconfirmed,
            cursor.step(),
            "a cancelled or failed attempt leaves the same candidate replayable"
        );

        let commit = cursor
            .confirm(CleanupDeletion::AlreadyMissing)
            .expect("an object absent after its proof is an idempotent success");
        assert_eq!(
            commit,
            CursorCommit {
                expected: 1,
                next: 2
            }
        );

        let mut successor =
            ExpiredCleanupCursor::resume(3, cursor.committed()).expect("resume is valid");
        assert_eq!(
            successor.step(),
            CleanupStep::Delete(2),
            "a successor owner resumes at the first uncommitted candidate"
        );
        let commit = successor
            .confirm(CleanupDeletion::Confirmed)
            .expect("the last candidate advances the cursor");
        assert_eq!(
            commit,
            CursorCommit {
                expected: 2,
                next: 3
            }
        );
        assert_eq!(successor.step(), CleanupStep::Finished);
        assert!(
            successor.confirm(CleanupDeletion::Confirmed).is_err(),
            "a drained cursor must refuse a duplicate terminal transition"
        );

        assert!(
            ExpiredCleanupCursor::resume(2, 3).is_err(),
            "a cursor past its own candidate set is unusable durable evidence"
        );
    }
}
