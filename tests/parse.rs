//! Parsing the example shop: functions, kinds, calls and `use` aliases.

use std::path::Path;

use blast_radius::parse::{Call, Kind, parse_dir};

fn shop() -> blast_radius::parse::Codebase {
    parse_dir(Path::new("examples/shop")).expect("the example parses")
}

#[test]
fn finds_functions_methods_tests_and_endpoints() {
    let codebase = shop();
    let kind = |name: &str| {
        codebase
            .functions
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.kind.clone())
    };
    assert_eq!(kind("pricing::apply_discount"), Some(Kind::Function));
    assert_eq!(kind("cart::Cart::total"), Some(Kind::Method));
    assert_eq!(kind("pricing::tests::ten_percent_off"), Some(Kind::Test));
    assert_eq!(
        kind("tests::checkout::quote_includes_vat"),
        Some(Kind::Test)
    );
    assert_eq!(
        kind("api::get_quote"),
        Some(Kind::Endpoint {
            method: "GET".into(),
            route: "/cart/quote".into()
        })
    );
}

#[test]
fn records_calls_including_those_inside_macros() {
    let codebase = shop();
    let calls = |name: &str| {
        codebase
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap()
            .calls
            .clone()
    };
    assert!(calls("cart::Cart::total").contains(&Call::Path(vec!["apply_discount".into()])));
    assert!(calls("cart::Cart::total").contains(&Call::Method("subtotal".into())));
    // assert_eq!(apply_discount(1000, 10), 900): the call is inside a macro.
    assert!(
        calls("pricing::tests::ten_percent_off")
            .contains(&Call::Path(vec!["apply_discount".into()]))
    );
}

#[test]
fn reads_use_items_and_line_numbers() {
    let codebase = shop();
    let cart = &codebase.scopes[&vec!["cart".to_string()]];
    assert_eq!(
        cart.aliases["apply_discount"],
        ["crate", "pricing", "apply_discount"]
    );
    let f = codebase
        .functions
        .iter()
        .find(|f| f.name == "pricing::apply_discount")
        .unwrap();
    assert_eq!((f.line, f.end_line), (4, 6));
    assert!(
        codebase
            .lines(&f.file, f.line, f.end_line)
            .starts_with("pub fn apply_discount")
    );
}
