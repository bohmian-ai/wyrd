//! Where a client reads ambient configuration.
//!
//! [`Environment`] is the one source of the `WYRD_*`, `HOME`, and
//! `XDG_CONFIG_HOME` values a [`ClientConfig`](crate::config::ClientConfig)
//! resolves endpoints, tenants, and credentials from. Production code uses
//! [`Environment::Process`]; tests pass [`Environment::Fixed`] instead of
//! mutating the process environment.

use std::collections::HashMap;
use std::path::PathBuf;

/// The variables a client reads ambient configuration from.
#[derive(Clone, Default)]
pub enum Environment {
    /// The running process's environment.
    #[default]
    Process,
    /// Exactly these variables; any other name is unset.
    Fixed(HashMap<String, String>),
}

impl Environment {
    /// The non-empty value of `name`, or `None` when it is unset or empty.
    #[must_use]
    pub fn var(&self, name: &str) -> Option<String> {
        match self {
            Self::Process => std::env::var(name).ok(),
            Self::Fixed(vars) => vars.get(name).cloned(),
        }
        .filter(|value| !value.is_empty())
    }

    /// The Wyrd configuration directory these variables resolve to.
    ///
    /// Follows the precedence of [`wyrd_utils::config_dir::wyrd_config_dir_from`].
    #[must_use]
    pub fn config_dir(&self) -> Option<PathBuf> {
        wyrd_utils::config_dir::wyrd_config_dir_from(|name| self.var(name))
    }
}

impl<const N: usize> From<[(&str, &str); N]> for Environment {
    /// A fixed environment holding exactly `vars`.
    fn from(vars: [(&str, &str); N]) -> Self {
        Self::Fixed(
            vars.into_iter()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect(),
        )
    }
}

impl std::fmt::Debug for Environment {
    /// Names only: values may hold credentials.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Process => f.write_str("Process"),
            Self::Fixed(vars) => {
                let mut names = vars.keys().collect::<Vec<_>>();
                names.sort();
                f.debug_tuple("Fixed").field(&names).finish()
            }
        }
    }
}
