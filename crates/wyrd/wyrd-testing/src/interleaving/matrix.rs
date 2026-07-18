//! Bounded phase-order matrix builder.

use std::fmt::Debug;

use rand::seq::SliceRandom;
use rand::{SeedableRng, rngs::StdRng};
use thiserror::Error;

use super::DEFAULT_MAX_PERMUTATIONS;

/// Canonical Bifrost phase names used by ordering tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Phase {
    /// Freeze mutable state before a durable transition.
    Freeze,
    /// Commit a sealed result.
    Seal,
    /// Fetch a result or snapshot.
    Fetch,
    /// Retire an immutable source.
    Retire,
    /// Replay a durable request.
    Replay,
}

/// Error returned by a matrix assertion.
#[derive(Debug, Error)]
#[error("matrix assertion failed for permutation {permutation:?}: {source:?}")]
pub struct MatrixError<E: Debug> {
    permutation: Vec<Phase>,
    source: E,
}

impl<E: Debug> MatrixError<E> {
    /// Return the phase ordering that failed.
    #[must_use]
    pub fn permutation(&self) -> &[Phase] {
        &self.permutation
    }

    /// Return the assertion's original error.
    #[must_use]
    pub const fn source(&self) -> &E {
        &self.source
    }
}

/// Summary of a completed matrix run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatrixReport {
    seed: u64,
    permutations_run: usize,
}

impl MatrixReport {
    /// Return the seed used to order the matrix.
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// Return the number of phase orderings exercised.
    #[must_use]
    pub const fn permutations_run(&self) -> usize {
        self.permutations_run
    }
}

/// Builder and runner for bounded phase-order matrices.
#[derive(Debug, Clone)]
pub struct Matrix {
    seed: u64,
    max_permutations: usize,
    phases: Vec<Phase>,
}

impl Default for Matrix {
    fn default() -> Self {
        Self::new()
    }
}

impl Matrix {
    /// Create an empty matrix with the default seed and cap.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            seed: 0,
            max_permutations: DEFAULT_MAX_PERMUTATIONS,
            phases: Vec::new(),
        }
    }

    /// Set the reproducible matrix seed.
    #[must_use]
    pub const fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Set the maximum number of phase permutations to run.
    #[must_use]
    pub const fn with_max_permutations(mut self, max_permutations: usize) -> Self {
        self.max_permutations = if max_permutations == 0 {
            1
        } else {
            max_permutations
        };
        self
    }

    /// Add one phase to the ordering matrix.
    #[must_use]
    pub fn phase(mut self, phase: Phase) -> Self {
        self.phases.push(phase);
        self
    }

    /// Add several phases in declaration order.
    #[must_use]
    pub fn phases<I>(mut self, phases: I) -> Self
    where
        I: IntoIterator<Item = Phase>,
    {
        self.phases.extend(phases);
        self
    }

    /// Return the bounded, seeded set of unique phase orderings.
    #[must_use]
    pub fn permutations(&self) -> Vec<Vec<Phase>> {
        let mut output = Vec::new();
        let mut remaining = self.phases.clone();
        let mut prefix = Vec::with_capacity(remaining.len());
        let mut rng = StdRng::seed_from_u64(self.seed);
        build_permutations(
            &mut remaining,
            &mut prefix,
            self.max_permutations,
            &mut rng,
            &mut output,
        );
        output
    }

    /// Replay an assertion for every phase ordering.
    pub fn run<F, E>(&self, mut assertion: F) -> Result<MatrixReport, MatrixError<E>>
    where
        F: FnMut(&[Phase]) -> Result<(), E>,
        E: Debug,
    {
        let permutations = self.permutations();
        for permutation in &permutations {
            assertion(permutation).map_err(|source| MatrixError {
                permutation: permutation.clone(),
                source,
            })?;
        }
        Ok(MatrixReport {
            seed: self.seed,
            permutations_run: permutations.len(),
        })
    }
}

fn build_permutations(
    remaining: &mut Vec<Phase>,
    prefix: &mut Vec<Phase>,
    cap: usize,
    rng: &mut StdRng,
    output: &mut Vec<Vec<Phase>>,
) {
    if output.len() >= cap {
        return;
    }
    if remaining.is_empty() {
        if !output.contains(prefix) {
            output.push(prefix.clone());
        }
        return;
    }

    let mut indices: Vec<_> = (0..remaining.len()).collect();
    indices.shuffle(rng);
    for index in indices {
        let phase = remaining.remove(index);
        prefix.push(phase);
        build_permutations(remaining, prefix, cap, rng, output);
        prefix.pop();
        remaining.insert(index, phase);
        if output.len() >= cap {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn matrix_replays_unique_bounded_phase_orders() {
        let matrix = Matrix::new()
            .with_seed(42)
            .with_max_permutations(32)
            .phases([Phase::Freeze, Phase::Seal, Phase::Fetch]);
        let permutations = matrix.permutations();
        let unique: HashSet<_> = permutations.iter().collect();
        assert_eq!(permutations.len(), 6);
        assert_eq!(unique.len(), 6);
    }

    #[test]
    fn matrix_stops_at_cap_and_reports_seed() {
        let matrix = Matrix::new().with_seed(9).with_max_permutations(2).phases([
            Phase::Freeze,
            Phase::Seal,
            Phase::Fetch,
        ]);
        let report = matrix
            .run(|_| Ok::<_, &'static str>(()))
            .expect("matrix passes");
        assert_eq!(report.seed(), 9);
        assert_eq!(report.permutations_run(), 2);
    }
}
