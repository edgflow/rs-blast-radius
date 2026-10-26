//! Quotes and orders.

use crate::cart::Cart;
use crate::inventory;

pub struct Quote {
    pub total_cents: u64,
}

pub struct Order {
    pub id: u64,
    pub total_cents: u64,
}

pub fn quote(cart: &Cart) -> Quote {
    Quote { total_cents: cart.total() }
}

pub fn place_order(cart: &Cart) -> Result<Order, String> {
    for line in &cart.lines {
        if !inventory::reserve(&line.sku, line.quantity) {
            return Err(format!("{} is out of stock", line.sku));
        }
    }
    let quote = quote(cart);
    Ok(Order { id: next_order_id(), total_cents: quote.total_cents })
}

fn next_order_id() -> u64 {
    42
}
