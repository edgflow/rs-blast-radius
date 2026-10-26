use shop::cart::Cart;
use shop::checkout;

#[test]
fn quote_includes_vat() {
    let mut cart = Cart::new();
    cart.add("mug", 1000, 2);
    assert_eq!(checkout::quote(&cart).total_cents, 2400);
}

#[test]
fn orders_fail_when_out_of_stock() {
    let mut cart = Cart::new();
    cart.add("mug", 1000, 50);
    assert!(checkout::place_order(&cart).is_err());
}
