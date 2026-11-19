//! Scanner behaviour on small source trees written to a temporary directory.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use impl_scan::{checks, Cfg, Registry, Scanner};

/// A fresh directory holding `files` (relative path, content).
fn tree(files: &[(&str, &str)]) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "impl-scan-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, content) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    dir
}

fn scan(files: &[(&str, &str)], trait_name: &str) -> Registry {
    Scanner::new(tree(files))
        .trait_impls(trait_name)
        .group("g")
        .cfg(Cfg::new())
        .emit_cargo(false)
        .scan()
        .unwrap()
}

fn names(registry: &Registry) -> Vec<(String, String)> {
    registry
        .group("g")
        .iter()
        .map(|i| (i.name.clone(), i.path.clone()))
        .collect()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

/// Every form a line-based regex (`impl Trait for (\w+)`) gets wrong.
#[test]
fn finds_what_a_regex_misses_and_skips_what_it_wrongly_finds() {
    let registry = scan(
        &[
            ("lib.rs", "pub mod a; pub mod b; pub trait Strategy {}"),
            (
                "a.rs",
                r#"
                use crate::Strategy;
                use crate::Strategy as S;
                pub struct Plain;
                impl Strategy for Plain {}
                pub struct Spaced;
                impl   Strategy   for   Spaced {}
                pub struct Qualified;
                impl crate::Strategy for Qualified {}
                pub struct Aliased;
                impl S for Aliased {}
                pub struct Generic<T>(T);
                impl<T> Strategy for Generic<T> {}
                pub struct Multi;
                impl
                    Strategy
                for Multi {}
                // impl Strategy for InComment {}
                /* impl Strategy for InBlockComment {} */
                const _: &str = "impl Strategy for InString {}";
                "#,
            ),
            (
                "b/mod.rs",
                r#"
                pub mod inner {
                    pub struct Nested;
                    impl super::super::Strategy for Nested {}
                }
                "#,
            ),
        ],
        "Strategy",
    );
    assert_eq!(
        names(&registry),
        pairs(&[
            ("ALIASED", "crate::a::Aliased"),
            ("GENERIC", "crate::a::Generic"),
            ("MULTI", "crate::a::Multi"),
            ("NESTED", "crate::b::inner::Nested"),
            ("PLAIN", "crate::a::Plain"),
            ("QUALIFIED", "crate::a::Qualified"),
            ("SPACED", "crate::a::Spaced"),
        ])
    );
    let generic = registry
        .group("g")
        .iter()
        .find(|i| i.type_name == "Generic")
        .unwrap();
    assert_eq!(generic.generics, ["T"]);
}

#[test]
fn path_queries_tell_same_named_traits_apart() {
    let files = [
        ("lib.rs", "pub mod x; pub mod y; pub mod impls;"),
        ("x.rs", "pub trait Key {}"),
        ("y.rs", "pub trait Key {}"),
        (
            "impls.rs",
            r#"
            use crate::x::Key;
            pub struct FromX;
            impl Key for FromX {}
            pub struct FromY;
            impl crate::y::Key for FromY {}
            "#,
        ),
    ];
    assert_eq!(names(&scan(&files, "Key")).len(), 2);
    assert_eq!(
        names(&scan(&files, "crate::x::Key")),
        pairs(&[("FROM_X", "crate::impls::FromX")])
    );
    assert_eq!(
        names(&scan(&files, "y::Key")),
        pairs(&[("FROM_Y", "crate::impls::FromY")])
    );
}

#[test]
fn cfg_is_evaluated_on_items_modules_and_files() {
    let files = [
        ("lib.rs", "pub trait T {} pub mod gated;"),
        ("gated.rs", "#![cfg(feature = \"gated\")]\npub struct InGatedFile; impl crate::T for InGatedFile {}"),
        (
            "more.rs",
            r#"
            pub struct Always; impl crate::T for Always {}
            pub struct Fast;
            #[cfg(feature = "fast")] impl crate::T for Fast {}
            pub struct NotFast;
            #[cfg(not(feature = "fast"))] impl crate::T for NotFast {}
            #[cfg(test)] mod tests { pub struct Mock; impl crate::T for Mock {} }
            "#,
        ),
    ];
    let scan_with = |cfg: Cfg| {
        let registry = Scanner::new(tree(&files))
            .trait_impls("T")
            .group("g")
            .cfg(cfg)
            .emit_cargo(false)
            .scan()
            .unwrap();
        registry
            .group("g")
            .iter()
            .map(|i| i.type_name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(scan_with(Cfg::new()), ["Always", "NotFast"]);
    assert_eq!(
        scan_with(Cfg::new().feature("fast").feature("gated")),
        ["Always", "Fast", "InGatedFile"]
    );
}

#[test]
fn register_attribute_and_docs() {
    let registry = scan(
        &[(
            "lib.rs",
            r#"
            pub trait Strategy {}
            /// Largest free block first.
            ///
            /// Second paragraph.
            pub struct MaxFreeSlot;
            #[impl_scan_attr::register(name = "MFS_V2", aliases = ["MFS", "MAX_FREE"], weight = 3, exact = true, ratio = 0.5, tags = ["a", "b"])]
            impl Strategy for MaxFreeSlot {}

            #[register(name = "ON_TYPE")]
            pub struct Annotated;
            /// Doc on the impl wins.
            impl Strategy for Annotated {}
            "#,
        )],
        "Strategy",
    );
    let items = registry.group("g");
    assert_eq!(items[0].name, "MFS_V2");
    assert_eq!(items[0].aliases, ["MFS", "MAX_FREE"]);
    assert_eq!(
        items[0].doc,
        "Largest free block first.\n\nSecond paragraph."
    );
    assert_eq!(items[0].meta["weight"], 3);
    assert_eq!(items[0].meta["exact"], true);
    assert_eq!(items[0].meta["ratio"], 0.5);
    assert_eq!(items[0].meta["tags"], serde_json::json!(["a", "b"]));
    assert_eq!(items[1].name, "ON_TYPE");
    assert_eq!(items[1].doc, "Doc on the impl wins.");
    assert_eq!(items[0].line, 8);
}

#[test]
fn duplicate_names_fail_with_both_locations() {
    let dir = tree(&[(
        "lib.rs",
        r#"
        pub trait T {}
        pub struct A; impl T for A {}
        pub struct B;
        #[register(aliases = ["A"])]
        impl T for B {}
        "#,
    )]);
    let err = Scanner::new(&dir)
        .trait_impls("T")
        .cfg(Cfg::new())
        .emit_cargo(false)
        .scan()
        .unwrap_err();
    let text = err.to_string();
    assert!(text.contains("duplicate name `A` in group `T`"), "{text}");
    assert!(text.contains("lib.rs:6"), "{text}");
    assert!(text.contains("also used by `crate::A`"), "{text}");
}

#[test]
fn parse_errors_point_at_the_file() {
    let dir = tree(&[
        ("lib.rs", "pub trait T {}"),
        ("broken.rs", "impl T for X {"),
        ("unrelated.rs", "fn main( {"),
    ]);
    let err = Scanner::new(&dir)
        .trait_impls("T")
        .cfg(Cfg::new())
        .emit_cargo(false)
        .scan()
        .unwrap_err();
    assert!(err.file().unwrap().ends_with("broken.rs"), "{err}");
    // A file that names neither the trait nor a found type is never parsed.
    let ok = tree(&[("lib.rs", "pub trait T {}"), ("unrelated.rs", "fn main( {")]);
    assert!(Scanner::new(&ok)
        .trait_impls("T")
        .cfg(Cfg::new())
        .emit_cargo(false)
        .scan()
        .is_ok());
}

#[test]
fn type_docs_and_register_are_read_from_other_files() {
    let registry = scan(
        &[
            (
                "lib.rs",
                "pub mod defs; pub mod impls; pub trait Strategy {}",
            ),
            (
                "defs.rs",
                "/// Defined elsewhere.\n#[register(name = \"FAR\")]\npub struct Far;",
            ),
            (
                "impls.rs",
                "use crate::defs::Far; impl crate::Strategy for Far {}",
            ),
        ],
        "Strategy",
    );
    let item = &registry.group("g")[0];
    assert_eq!(
        (item.name.as_str(), item.doc.as_str(), item.path.as_str()),
        ("FAR", "Defined elsewhere.", "crate::defs::Far")
    );
}

#[test]
fn checks_run_on_the_final_registry() {
    let files = [(
        "lib.rs",
        r#"
        pub trait Strategy {} pub trait Table {}
        pub struct Gnpy; impl Table for Gnpy {}
        /// Documented.
        #[register(table = "GNPY")] pub struct Good; impl Strategy for Good {}
        #[register(table = "Missing")] pub struct Bad; impl Strategy for Bad {}
        "#,
    )];
    let base = || {
        Scanner::new(tree(&files))
            .trait_impls("Strategy")
            .group("strategies")
            .trait_impls("Table")
            .group("tables")
            .cfg(Cfg::new())
            .emit_cargo(false)
    };
    let err = base()
        .check(checks::meta_refs("strategies", "table", "tables"))
        .scan()
        .unwrap_err();
    assert!(
        err.to_string().contains("references table \"Missing\""),
        "{err}"
    );
    assert!(err.to_string().contains("known: GNPY"), "{err}");
    let err = base()
        .check(checks::require_doc("strategies"))
        .scan()
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("`crate::Bad` in group `strategies` has no doc comment"),
        "{err}"
    );
}

#[test]
fn order_is_deterministic_and_empty_groups_exist() {
    let registry = Scanner::new(tree(&[
        ("z.rs", "pub struct Z; impl T for Z {}"),
        ("a.rs", "pub struct A; impl T for A {}"),
    ]))
    .trait_impls("T")
    .group("g")
    .trait_impls("Unused")
    .group("empty")
    .extra("g", "crate::macro_made::M")
    .cfg(Cfg::new())
    .emit_cargo(false)
    .scan()
    .unwrap();
    assert_eq!(
        names(&registry),
        pairs(&[
            ("A", "crate::a::A"),
            ("M", "crate::macro_made::M"),
            ("Z", "crate::z::Z")
        ])
    );
    assert!(registry.group_names().any(|g| g == "empty"));
    assert_eq!(registry.to_json()["empty"], serde_json::json!([]));
}
