//! Shared resolution of the user's Wyrd configuration directory.

use std::path::PathBuf;

/// Resolve the Wyrd configuration directory without touching the filesystem.
///
/// Precedence is `$WYRD_CONFIG_HOME`, `$XDG_CONFIG_HOME/wyrd`, and
/// `$HOME/.config/wyrd`. Empty environment values are treated as unset.
#[must_use]
pub fn wyrd_config_dir() -> Option<PathBuf> {
    non_empty_env("WYRD_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| non_empty_env("XDG_CONFIG_HOME").map(|path| PathBuf::from(path).join("wyrd")))
        .or_else(|| non_empty_env("HOME").map(|path| PathBuf::from(path).join(".config/wyrd")))
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

/// Return an environment value only when it is non-empty.
fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use std::ffi::OsString;
    use std::sync::Mutex;

    use super::{ConfigDirError, wyrd_config_dir, wyrd_config_dir_required};

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    struct EnvironmentGuard {
        previous: Vec<(String, Option<OsString>)>,
    }

    impl Drop for EnvironmentGuard {
        fn drop(&mut self) {
            unsafe {
                for (name, value) in self.previous.drain(..) {
                    match value {
                        Some(value) => std::env::set_var(name, value),
                        None => std::env::remove_var(name),
                    }
                }
            }
        }
    }

    /// Run a configuration-directory assertion with a controlled environment.
    fn with_env<T>(values: &[(&str, Option<&str>)], test: impl FnOnce() -> T) -> T {
        let _guard = ENV_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = values
            .iter()
            .map(|(name, _)| ((*name).to_owned(), std::env::var_os(name)))
            .collect::<Vec<(String, Option<OsString>)>>();
        let _environment = EnvironmentGuard { previous };
        unsafe {
            for (name, value) in values {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
        test()
    }

    #[test]
    fn wyrd_config_home_is_used_verbatim() {
        let dir = with_env(
            &[
                ("WYRD_CONFIG_HOME", Some("/tmp/wyrd-config")),
                ("XDG_CONFIG_HOME", None),
                ("HOME", None),
            ],
            wyrd_config_dir,
        );
        assert_eq!(
            dir.as_deref(),
            Some(std::path::Path::new("/tmp/wyrd-config"))
        );
    }

    #[test]
    fn xdg_config_home_adds_wyrd_component() {
        let dir = with_env(
            &[
                ("WYRD_CONFIG_HOME", None),
                ("XDG_CONFIG_HOME", Some("/tmp/xdg")),
                ("HOME", None),
            ],
            wyrd_config_dir,
        );
        assert_eq!(dir.as_deref(), Some(std::path::Path::new("/tmp/xdg/wyrd")));
    }

    #[test]
    fn home_adds_standard_config_components() {
        let dir = with_env(
            &[
                ("WYRD_CONFIG_HOME", None),
                ("XDG_CONFIG_HOME", None),
                ("HOME", Some("/home/u")),
            ],
            wyrd_config_dir,
        );
        assert_eq!(
            dir.as_deref(),
            Some(std::path::Path::new("/home/u/.config/wyrd"))
        );
    }

    #[test]
    fn empty_override_is_skipped() {
        let dir = with_env(
            &[
                ("WYRD_CONFIG_HOME", Some("")),
                ("XDG_CONFIG_HOME", None),
                ("HOME", Some("/home/u")),
            ],
            wyrd_config_dir,
        );
        assert_eq!(
            dir.as_deref(),
            Some(std::path::Path::new("/home/u/.config/wyrd"))
        );
    }

    #[test]
    fn required_resolution_reports_missing_environment() {
        let result = with_env(
            &[
                ("WYRD_CONFIG_HOME", None),
                ("XDG_CONFIG_HOME", None),
                ("HOME", None),
            ],
            wyrd_config_dir_required,
        );
        assert!(matches!(result, Err(ConfigDirError::Unresolvable)));
    }

    #[test]
    fn environment_is_restored_after_panic() {
        let values = [
            ("WYRD_CONFIG_HOME", Some("/tmp/panic-config")),
            ("XDG_CONFIG_HOME", Some("/tmp/panic-xdg")),
            ("HOME", Some("/tmp/panic-home")),
        ];
        let previous = values
            .iter()
            .map(|(name, _)| ((*name).to_owned(), std::env::var_os(name)))
            .collect::<Vec<_>>();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_env(&values, || panic!("deliberate environment panic"));
        }));
        assert!(result.is_err());

        with_env(&[], || {
            for (name, value) in previous {
                assert_eq!(std::env::var_os(name), value);
            }
        });
    }
}
