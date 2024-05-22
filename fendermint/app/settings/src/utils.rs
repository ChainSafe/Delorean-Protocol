// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use config::{ConfigError, Source, Value, ValueKind};
use lazy_static::lazy_static;
use regex::Regex;
use std::path::{Path, PathBuf};

#[macro_export]
macro_rules! home_relative {
    // Using this inside something that has a `.home_dir()` function.
    ($($name:ident),+) => {
        $(
        pub fn $name(&self) -> std::path::PathBuf {
            $crate::utils::expand_path(&self.home_dir(), &self.$name)
        }
        )+
    };

    // Using this outside something that requires a `home_dir` parameter to be passed to it.
    ($settings:ty { $($name:ident),+ } ) => {
      impl $settings {
        $(
        pub fn $name(&self, home_dir: &std::path::Path) -> std::path::PathBuf {
            $crate::utils::expand_path(home_dir, &self.$name)
        }
        )+
      }
    };
}

/// Expand a path which can either be :
/// * absolute, e.g. "/foo/bar"
/// * relative to the system `$HOME` directory, e.g. "~/foo/bar"
/// * relative to the configured `--home-dir` directory, e.g. "foo/bar"
pub fn expand_path(home_dir: &Path, path: &Path) -> PathBuf {
    if path.starts_with("/") {
        PathBuf::from(path)
    } else if path.starts_with("~") {
        expand_tilde(path)
    } else {
        expand_tilde(home_dir.join(path))
    }
}

/// Expand paths that begin with "~" to `$HOME`.
pub fn expand_tilde<P: AsRef<Path>>(path: P) -> PathBuf {
    let p = path.as_ref().to_path_buf();
    if !p.starts_with("~") {
        return p;
    }
    if p == Path::new("~") {
        return dirs::home_dir().unwrap_or(p);
    }
    dirs::home_dir()
        .map(|mut h| {
            if h == Path::new("/") {
                // `~/foo` becomes just `/foo` instead of `//foo` if `/` is home.
                p.strip_prefix("~").unwrap().to_path_buf()
            } else {
                h.push(p.strip_prefix("~/").unwrap());
                h
            }
        })
        .unwrap_or(p)
}

#[derive(Clone, Debug)]
pub struct EnvInterpol<T>(pub T);

impl<T: Source + Clone + Send + Sync + 'static> Source for EnvInterpol<T> {
    fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
        Box::new(self.clone())
    }

    fn collect(&self) -> Result<config::Map<String, config::Value>, ConfigError> {
        let mut values = self.0.collect()?;
        for value in values.values_mut() {
            interpolate_values(value);
        }
        Ok(values)
    }
}

/// Find values in the string that can be interpolated, e.g. "${NOMAD_HOST_ADDRESS_cometbft_p2p}"
fn find_vars(value: &str) -> Vec<&str> {
    lazy_static! {
        /// Capture env variables like `${VARIABLE_NAME}`
        static ref ENV_VAR_RE: Regex = Regex::new(r"\$\{([^}]+)\}").expect("env var regex parses");
    }
    ENV_VAR_RE
        .captures_iter(value)
        .map(|c| c.extract())
        .map(|(_, [n])| n)
        .collect()
}

/// Find variables and replace them from the environment.
///
/// Returns `None` if there are no env vars in the value.
fn interpolate_vars(value: &str) -> Option<String> {
    let keys = find_vars(value);
    if keys.is_empty() {
        return None;
    }
    let mut value = value.to_string();
    for k in keys {
        if let Ok(v) = std::env::var(k) {
            value = value.replace(&format!("${{{k}}}"), &v);
        }
    }
    Some(value)
}

/// Find strings which have env vars in them and do the interpolation.
///
/// It does not change the kind of the values, ie. it doesn't try to parse
/// into primitives or arrays *after* the interpolation. It does recurse
/// into arrays, though, so if there are variables within array items,
/// they get replaced.
fn interpolate_values(value: &mut Value) {
    match value.kind {
        ValueKind::String(ref mut s) => {
            if let Some(i) = interpolate_vars(s) {
                // TODO: We could try to parse into primitive values,
                // but the only reason we do it with `Environment` is to support list separators,
                // otherwise it was fine with just strings, so I think we can skip this for now.
                *s = i;
            }
        }
        ValueKind::Array(ref mut vs) => {
            for v in vs.iter_mut() {
                interpolate_values(v);
            }
        }
        // Leave anything else as it is.
        _ => {}
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::PathBuf;

    use crate::utils::find_vars;

    use super::{expand_tilde, interpolate_vars};

    /// Set some env vars, run a fallible piece of code, then unset the variables otherwise they would affect the next test.
    pub fn with_env_vars<F, T, E>(vars: Vec<(&str, &str)>, f: F) -> Result<T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        for (k, v) in vars.iter() {
            std::env::set_var(k, v);
        }
        let result = f();
        for (k, _) in vars {
            std::env::remove_var(k);
        }
        result
    }

    #[test]
    fn tilde_expands_to_home() {
        let home = std::env::var("HOME").expect("should work on Linux");
        let home_project = PathBuf::from(format!("{}/.project", home));
        assert_eq!(expand_tilde("~/.project"), home_project);
        assert_eq!(expand_tilde("/foo/bar"), PathBuf::from("/foo/bar"));
        assert_eq!(expand_tilde("~foo/bar"), PathBuf::from("~foo/bar"));
    }

    #[test]
    fn test_find_vars() {
        assert_eq!(
            find_vars("FOO_${NAME}_${NUMBER}_BAR"),
            vec!["NAME", "NUMBER"]
        );
        assert!(find_vars("FOO_${NAME").is_empty());
        assert!(find_vars("FOO_$NAME").is_empty());
    }

    #[test]
    fn test_interpolate_vars() {
        let s = "FOO_${NAME}_${NUMBER}_BAR";
        let i = with_env_vars::<_, _, ()>(vec![("NAME", "spam")], || Ok(interpolate_vars(s)))
            .unwrap()
            .expect("non empty vars");
        assert_eq!(i, "FOO_spam_${NUMBER}_BAR");
    }
}
