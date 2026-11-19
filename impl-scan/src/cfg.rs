//! Evaluation of `#[cfg(...)]` predicates against the active build configuration.

use std::collections::{BTreeMap, BTreeSet};

use syn::punctuated::Punctuated;
use syn::{Attribute, Expr, ExprLit, Lit, Meta, Token};

/// The configuration `#[cfg(...)]` attributes are evaluated against.
///
/// In a build script, [`Cfg::from_env`] reads what Cargo exports: `CARGO_FEATURE_*`
/// for features and `CARGO_CFG_*` for everything else (`unix`, `target_os = "linux"`,
/// `debug_assertions`, ...). `cfg(test)` is always false: a build script cannot know
/// whether the crate is being compiled for tests.
#[derive(Debug, Clone, Default)]
pub struct Cfg {
    features: BTreeSet<String>,
    flags: BTreeSet<String>,
    values: BTreeMap<String, BTreeSet<String>>,
}

impl Cfg {
    /// An empty configuration: no features, no flags.
    pub fn new() -> Self {
        Self::default()
    }

    /// The configuration Cargo exports to the running build script.
    pub fn from_env() -> Self {
        let mut cfg = Self::new();
        for (key, value) in std::env::vars() {
            if let Some(feature) = key.strip_prefix("CARGO_FEATURE_") {
                cfg.features.insert(normalize_feature(feature));
            } else if let Some(name) = key.strip_prefix("CARGO_CFG_") {
                let name = name.to_ascii_lowercase();
                if value.is_empty() {
                    cfg.flags.insert(name);
                } else {
                    let set = cfg.values.entry(name).or_default();
                    set.extend(value.split(',').map(str::to_owned));
                }
            }
        }
        cfg
    }

    /// Enables a feature (`cfg(feature = "name")`).
    pub fn feature(mut self, name: &str) -> Self {
        self.features.insert(normalize_feature(name));
        self
    }

    /// Sets a name-only cfg (`cfg(unix)`).
    pub fn flag(mut self, name: &str) -> Self {
        self.flags.insert(name.to_owned());
        self
    }

    /// Sets a key-value cfg (`cfg(target_os = "linux")`).
    pub fn value(mut self, key: &str, value: &str) -> Self {
        self.values
            .entry(key.to_owned())
            .or_default()
            .insert(value.to_owned());
        self
    }

    /// Whether every `#[cfg]` among `attrs` holds. Other attributes are ignored.
    pub(crate) fn enabled(&self, attrs: &[Attribute]) -> bool {
        attrs
            .iter()
            .filter(|a| a.path().is_ident("cfg"))
            .all(|a| match a.parse_args::<Meta>() {
                Ok(meta) => self.eval(&meta),
                // An unparsable predicate would not compile either; keep the item so the
                // compiler reports it instead of the registry silently dropping it.
                Err(_) => true,
            })
    }

    fn eval(&self, meta: &Meta) -> bool {
        match meta {
            Meta::Path(path) => match path.get_ident() {
                Some(id) if id == "test" => false,
                Some(id) => self.flags.contains(&id.to_string()),
                None => false,
            },
            Meta::NameValue(nv) => {
                let Some(key) = nv.path.get_ident().map(|i| i.to_string()) else {
                    return false;
                };
                let Expr::Lit(ExprLit {
                    lit: Lit::Str(s), ..
                }) = &nv.value
                else {
                    return false;
                };
                if key == "feature" {
                    self.features.contains(&normalize_feature(&s.value()))
                } else {
                    self.values
                        .get(&key)
                        .is_some_and(|set| set.contains(&s.value()))
                }
            }
            Meta::List(list) => {
                let Ok(inner) =
                    list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                else {
                    return true;
                };
                match list.path.get_ident().map(|i| i.to_string()).as_deref() {
                    Some("all") => inner.iter().all(|m| self.eval(m)),
                    Some("any") => inner.iter().any(|m| self.eval(m)),
                    Some("not") => inner.len() == 1 && !self.eval(&inner[0]),
                    _ => false,
                }
            }
        }
    }
}

fn normalize_feature(name: &str) -> String {
    name.to_ascii_lowercase().replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(src: &str) -> Vec<Attribute> {
        let item: syn::ItemStruct = syn::parse_str(&format!("{src} struct S;")).unwrap();
        item.attrs
    }

    #[test]
    fn features_flags_and_combinators() {
        let cfg = Cfg::new()
            .feature("fast-path")
            .flag("unix")
            .value("target_os", "linux");
        assert!(cfg.enabled(&attrs(r#"#[cfg(feature = "fast_path")]"#)));
        assert!(cfg.enabled(&attrs(r#"#[cfg(all(unix, target_os = "linux"))]"#)));
        assert!(cfg.enabled(&attrs(r#"#[cfg(any(windows, unix))]"#)));
        assert!(cfg.enabled(&attrs(r#"#[cfg(not(feature = "slow"))]"#)));
        assert!(!cfg.enabled(&attrs(r#"#[cfg(test)]"#)));
        assert!(!cfg.enabled(&attrs(r#"#[cfg(target_os = "macos")]"#)));
        assert!(!cfg.enabled(&attrs(r#"#[cfg(unix)] #[cfg(windows)]"#)));
        assert!(cfg.enabled(&attrs(r#"#[derive(Debug)]"#)));
    }
}
