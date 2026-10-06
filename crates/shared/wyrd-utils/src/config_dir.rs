//! Shared resolution of the user's Wyrd configuration directory.

use std::path::PathBuf;

/// Resolve the Wyrd configuration directory from the process environment.
///
/// Delegates to [`wyrd_config_dir_from`] with [`std::env::var`] as the lookup.
#[must_use]
pub fn wyrd_config_dir() -> Option<PathBuf> {
    wyrd_config_dir_from(|name| std::env::var(name).ok())
}

/// Resolve the Wyrd configuration directory without touching the filesystem.
///
/// Precedence is `$WYRD_CONFIG_HOME`, `$XDG_CONFIG_HOME/wyrd`, and
/// `$HOME/.config/wyrd`. `lookup` returns a variable's value; empty values are
/// treated as unset. Callers outside tests use [`wyrd_config_dir`].
#[must_use]
pub fn wyrd_config_dir_from(lookup: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let non_empty = |name: &str| lookup(name).filter(|value| !value.is_empty());
    non_empty("WYRD_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| non_empty("XDG_CONFIG_HOME").map(|path| PathBuf::from(path).join("wyrd")))
        .or_else(|| non_empty("HOME").map(|path| PathBuf::from(path).join(".config/wyrd")))
}

/// Resolve the Wyrd configuration directory or return a descriptive error.
///
/// # Errors
/// Returns [`ConfigDirError::Unresolvable`] when none of the supported
/// environment variables contains a non-empty path.
pub fn wyrd_config_dir_required() -> Result<PathBuf, ConfigDirError> {
    wyrd_config_dir().ok_or(ConfigDirError::Unresolvable)
}

/// Error returned when the Wyrd configuration directory cannot be resolved.
#[derive(Debug, thiserror::Error)]
pub enum ConfigDirError {
    /// No supported environment variable supplied a configuration root.
    #[error(
        "cannot resolve Wyrd config directory: set $WYRD_CONFIG_HOME, $XDG_CONFIG_HOME, or $HOME"
    )]
    Unresolvable,
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::wyrd_config_dir_from;

    /// Resolve the configuration directory from exactly `values`.
    fn resolve(values: &[(&str, &str)]) -> Option<PathBuf> {
        wyrd_config_dir_from(|name| {
            values
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        })
    }

    /// `$WYRD_CONFIG_HOME` wins over every other source.
    #[test]
    fn wyrd_config_home_is_used_verbatim() {
        let dir = resolve(&[
            ("WYRD_CONFIG_HOME", "/tmp/wyrd-config"),
            ("XDG_CONFIG_HOME", "/tmp/xdg"),
            ("HOME", "/home/u"),
        ]);
        assert_eq!(dir.as_deref(), Some(Path::new("/tmp/wyrd-config")));
    }

    /// `$XDG_CONFIG_HOME` gains a `wyrd` component.
    #[test]
    fn xdg_config_home_adds_wyrd_component() {
        let dir = resolve(&[("XDG_CONFIG_HOME", "/tmp/xdg"), ("HOME", "/home/u")]);
        assert_eq!(dir.as_deref(), Some(Path::new("/tmp/xdg/wyrd")));
    }

    /// `$HOME` gains the standard `.config/wyrd` components.
    #[test]
    fn home_adds_standard_config_components() {
        let dir = resolve(&[("HOME", "/home/u")]);
        assert_eq!(dir.as_deref(), Some(Path::new("/home/u/.config/wyrd")));
    }

    /// An empty override falls through to the next source.
    #[test]
    fn empty_override_is_skipped() {
        let dir = resolve(&[("WYRD_CONFIG_HOME", ""), ("HOME", "/home/u")]);
        assert_eq!(dir.as_deref(), Some(Path::new("/home/u/.config/wyrd")));
    }

    /// No source resolves to no directory.
    #[test]
    fn missing_environment_resolves_nothing() {
        assert_eq!(resolve(&[]), None);
    }
}
