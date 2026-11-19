//! Ready-made checks for [`Scanner::check`](crate::Scanner::check).
//!
//! A check runs on the final registry and fails the build with a located error, so a
//! rule that would otherwise be found at run time (or in review) is found by `cargo build`.

use crate::error::Error;
use crate::scan::{Item, Registry};

fn located(item: &Item, message: String) -> Error {
    let error = Error::new(message);
    if item.file.is_empty() {
        error
    } else {
        error.at(&item.file, Some(item.line))
    }
}

/// Every item of `group` must have a doc comment, on the `impl` or on the type.
pub fn require_doc(group: &str) -> impl Fn(&Registry) -> Result<(), Error> {
    let group = group.to_owned();
    move |registry| {
        for item in registry.group(&group) {
            if item.doc.is_empty() {
                return Err(located(
                    item,
                    format!("`{}` in group `{group}` has no doc comment", item.path),
                )
                .help("add a `///` comment to the impl block or to the type"));
            }
        }
        Ok(())
    }
}

/// Every item of `group` that sets `#[register(<key> = "...")]` must name an item of
/// `target` (by registered name, alias or type name). Items without the key pass.
pub fn meta_refs(group: &str, key: &str, target: &str) -> impl Fn(&Registry) -> Result<(), Error> {
    let (group, key, target) = (group.to_owned(), key.to_owned(), target.to_owned());
    move |registry| {
        let targets = registry.group(&target);
        for item in registry.group(&group) {
            let Some(value) = item.meta.get(&key) else {
                continue;
            };
            let Some(wanted) = value.as_str() else {
                return Err(located(
                    item,
                    format!("`{key}` of `{}` must be a string", item.path),
                ));
            };
            let found = targets.iter().any(|t| {
                t.name == wanted || t.type_name == wanted || t.aliases.iter().any(|a| a == wanted)
            });
            if !found {
                let known: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
                return Err(located(
                    item,
                    format!("`{}` in group `{group}` references {key} \"{wanted}\", not found in group `{target}`", item.name),
                )
                .help(format!("known: {}", known.join(", "))));
            }
        }
        Ok(())
    }
}

/// Every item of `group` must be a concrete type (no generic `impl`). The built-in
/// `store` partial needs this: it stores each type in a map.
pub fn concrete(group: &str) -> impl Fn(&Registry) -> Result<(), Error> {
    let group = group.to_owned();
    move |registry| {
        for item in registry.group(&group) {
            if !item.generics.is_empty() {
                return Err(located(
                    item,
                    format!("`{}` in group `{group}` is generic over {}", item.path, item.generics.join(", ")),
                )
                .help("register a concrete alias instead, e.g. `type Small = Foo<4>;` with Scanner::extra"));
            }
        }
        Ok(())
    }
}
