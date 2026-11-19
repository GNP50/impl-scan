//! The generated registry, exercised as a user of the crate would.

use impl_scan_example::{default_strategy, Strategies, StrategyKind, TableKind, Tables};

#[test]
fn enum_lists_every_strategy_by_registered_name() {
    let names: Vec<&str> = StrategyKind::ALL.iter().map(|k| k.name()).collect();
    if cfg!(feature = "experimental") {
        assert_eq!(names, ["FIRST_FIT", "LAST_FIT", "MAX_FREE_SLOT"]);
    } else {
        assert_eq!(names, ["FIRST_FIT", "MAX_FREE_SLOT"]);
    }
    assert_eq!(
        StrategyKind::from_name("MFS"),
        Some(StrategyKind::MaxFreeSlot)
    );
    assert_eq!(StrategyKind::from_name("FF"), Some(StrategyKind::FirstFit));
    assert_eq!(StrategyKind::from_name("nope"), None);
    assert_eq!(StrategyKind::FirstFit.to_string(), "FIRST_FIT");
}

#[test]
fn store_hands_out_dyn_trait_by_id() {
    let mut store = Strategies::default();
    store
        .insert_json(StrategyKind::FirstFit, 1, r#"{"min_width": 3}"#)
        .unwrap();
    store
        .insert_json(StrategyKind::MaxFreeSlot, 2, "null")
        .unwrap();
    assert_eq!(store.len(), 2);

    let free = [2, 5, 9, 4];
    assert_eq!(store.kind_of(&1), Some(StrategyKind::FirstFit));
    assert_eq!(store.find(&1).unwrap().pick(&free, 1), Some(1)); // min_width 3 skips the 2
    assert_eq!(store.find(&2).unwrap().pick(&free, 1), Some(2));
    assert!(store.get(StrategyKind::MaxFreeSlot, &1).is_none());
    assert_eq!(store.first_fit(&1).unwrap().min_width, 3);

    // An id lives in one map only: storing another kind under it replaces the old one.
    store.insert_max_free_slot(1, Default::default());
    assert_eq!(store.kind_of(&1), Some(StrategyKind::MaxFreeSlot));
    assert_eq!(store.len(), 2);
    assert_eq!(store.remove(&1), Some(StrategyKind::MaxFreeSlot));
    assert!(store.find(&1).is_none());
    assert!(store
        .insert_json(StrategyKind::FirstFit, 3, "[1,2]")
        .is_err());
}

#[test]
fn dispatch_builds_defaults_and_metadata_links_tables() {
    for &kind in StrategyKind::ALL {
        assert!(default_strategy(kind).pick(&[1, 8], 4).is_some(), "{kind}");
    }
    assert_eq!(StrategyKind::FirstFit.table(), Some(TableKind::Linear));
    assert_eq!(StrategyKind::MaxFreeSlot.table(), Some(TableKind::Squares));

    let mut tables = Tables::default();
    tables.insert_squares(7, Default::default());
    let table = tables.find(&7).unwrap();
    assert_eq!(table.lookup(5), 25);
}
