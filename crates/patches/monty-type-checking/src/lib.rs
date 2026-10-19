//! Wyrd's stand-in for `monty-type-checking`, without the type checker.
//!
//! `wyrd-server` re-executes itself as the Monty worker, so it enables
//! `monty-proto/worker`, and that feature unconditionally depends on
//! `monty-type-checking`. That crate brings the whole `ty`, `ruff`, `salsa`,
//! and typeshed stack into the server binary, yet Wyrd never opens a session
//! with type checking. The workspace `[patch.crates-io]` replaces the crate
//! with this one, which keeps the exact API `monty-proto`'s worker calls and
//! refuses every check.
//!
//! A session that asks for type checking therefore fails with
//! [`UNAVAILABLE`] instead of running unchecked. If a `monty-proto` upgrade
//! changes how it calls the checker, this crate stops compiling; if it moves
//! `monty-type-checking` past 1.1, the patch stops applying and cargo reports
//! it unused. Drop this crate once upstream makes the checker optional.

use std::convert::Infallible;
use std::fmt;
use std::marker::PhantomData;

use monty_types::TypeCheckingConfig;

/// The error every type check returns in this build.
pub const UNAVAILABLE: &str = "type checking is not available in this Wyrd build";

/// A Python source file named for type checking, as `monty-proto` builds it.
pub struct SourceFile<'a> {
    /// The Python source text.
    pub source_code: &'a str,
    /// The path the source is checked as.
    pub path: &'a str,
}

impl<'a> SourceFile<'a> {
    /// Name `source_code` as the file at `path`.
    #[must_use]
    pub fn new(source_code: &'a str, path: &'a str) -> Self {
        Self { source_code, path }
    }
}

/// A type checker that holds no state and checks nothing.
#[derive(Debug, Default)]
pub struct TypeChecker;

impl TypeChecker {
    /// Refuse to type check `python_source`.
    ///
    /// Keeps the upstream signature so `monty-proto` compiles unchanged; the
    /// borrow of `self` ties the never-built diagnostics to the checker as
    /// upstream's do.
    ///
    /// # Errors
    /// Always returns [`UNAVAILABLE`].
    pub fn run<'a>(
        &'a mut self,
        _python_source: &SourceFile<'_>,
        _stubs_file: Option<&SourceFile<'_>>,
        _config: TypeCheckingConfig,
    ) -> Result<Option<TypeCheckingDiagnostics<'a>>, String> {
        Err(UNAVAILABLE.to_owned())
    }

    /// Reset the checker between sessions; there is no state to drop.
    ///
    /// # Errors
    /// Never fails.
    pub fn reset(&mut self) -> Result<(), String> {
        Ok(())
    }
}

/// Diagnostics of a failed check; uninhabited, because no check ever runs.
pub struct TypeCheckingDiagnostics<'a> {
    /// Makes the type impossible to construct.
    never: Infallible,
    /// Keeps upstream's borrow of the checker in the type.
    checker: PhantomData<&'a TypeChecker>,
}

impl fmt::Display for TypeCheckingDiagnostics<'_> {
    /// Unreachable: no value of this type can exist.
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.never {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session that asks for type checking is refused rather than run unchecked.
    #[test]
    fn every_check_is_refused() {
        let mut checker = TypeChecker::default();
        let source = SourceFile::new("x: int = 1", "main.py");

        let refused = checker.run(&source, None, TypeCheckingConfig::default());

        assert_eq!(refused.err().as_deref(), Some(UNAVAILABLE));
        assert_eq!(checker.reset(), Ok(()));
    }
}
