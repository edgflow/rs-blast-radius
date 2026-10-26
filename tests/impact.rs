//! The call graph of the example shop, and the blast radius of changing a function.

use std::path::Path;

use blast_radius::graph::{Confidence, Graph};
use blast_radius::impact::{Level, blast_radius};
use blast_radius::parse::parse_dir;

fn shop() -> Graph {
    Graph::build(&parse_dir(Path::new("examples/shop")).unwrap())
}

fn edge(graph: &Graph, from: &str, to: &str) -> Option<Confidence> {
    let (from, to) = (graph.find(from).unwrap(), graph.find(to).unwrap());
    graph.callees[from]
        .iter()
        .find(|e| e.other == to)
        .map(|e| e.confidence)
}

#[test]
fn calls_resolve_through_use_items_modules_and_globs() {
    let g = shop();
    // use crate::pricing::apply_discount;  then  apply_discount(…)
    assert_eq!(
        edge(&g, "cart::Cart::total", "pricing::apply_discount"),
        Some(Confidence::Certain)
    );
    // use super::*;  inside pricing::tests
    assert_eq!(
        edge(
            &g,
            "pricing::tests::ten_percent_off",
            "pricing::apply_discount"
        ),
        Some(Confidence::Certain)
    );
    // a function in the same module
    assert_eq!(
        edge(&g, "checkout::place_order", "checkout::quote"),
        Some(Confidence::Certain)
    );
    // use shop::checkout;  in an integration test (the crate's own name)
    assert_eq!(
        edge(&g, "tests::checkout::quote_includes_vat", "checkout::quote"),
        Some(Confidence::Certain)
    );
    // cart.total(): a method, matched by name only
    assert_eq!(
        edge(&g, "checkout::quote", "cart::Cart::total"),
        Some(Confidence::Likely)
    );
}

#[test]
fn the_blast_radius_follows_callers_all_the_way_up() {
    let g = shop();
    let target = g.find("apply_discount").unwrap();
    let impact = blast_radius(&g, target);
    let names: Vec<&str> = impact
        .affected
        .iter()
        .map(|a| g.functions[a.id].name.as_str())
        .collect();
    for expected in [
        "cart::Cart::total",
        "checkout::quote",
        "checkout::place_order",
        "api::get_quote",
        "api::post_checkout",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
    for untouched in [
        "inventory::reserve",
        "tax::vat",
        "api::post_restock",
        "cart::Cart::add",
    ] {
        assert!(
            !names.contains(&untouched),
            "{untouched} should not be affected"
        );
    }
    assert_eq!(impact.tests(&g).count(), 3);
    assert_eq!(impact.endpoints(&g).count(), 2);

    let post = g.find("api::post_checkout").unwrap();
    let path: Vec<&str> = impact
        .path(post)
        .iter()
        .map(|&i| g.functions[i].name.as_str())
        .collect();
    assert_eq!(
        path,
        [
            "api::post_checkout",
            "checkout::place_order",
            "checkout::quote",
            "cart::Cart::total",
            "pricing::apply_discount"
        ]
    );
}

#[test]
fn the_risk_score_explains_itself() {
    let g = shop();
    let discount = blast_radius(&g, g.find("apply_discount").unwrap()).risk;
    assert_eq!((discount.level, discount.score), (Level::High, 78));

    // Only one endpoint calls restock, but nothing tests it.
    let restock = blast_radius(&g, g.find("inventory::restock").unwrap()).risk;
    assert_eq!(restock.level, Level::Medium);
    assert!(
        restock
            .reasons
            .iter()
            .any(|r| r.contains("no test reaches it"))
    );

    // A private helper with one caller and a test above it.
    let id = blast_radius(&g, g.find("next_order_id").unwrap()).risk;
    assert!(id.score < discount.score);
}

#[test]
fn functions_are_found_by_a_unique_ending() {
    let g = shop();
    assert_eq!(
        g.functions[g.find("total").unwrap()].name,
        "cart::Cart::total"
    );
    assert!(g.find("nope").unwrap_err().contains("no function"));
}
