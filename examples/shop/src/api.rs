//! HTTP handlers, in the style of actix-web or Rocket. Mobile apps and partner
//! integrations call these: they are the shop's public contract.

use crate::cart::Cart;
use crate::{checkout, inventory};

#[get("/cart/quote")]
pub fn get_quote(cart: Cart) -> String {
    format!("{{\"total_cents\": {}}}", checkout::quote(&cart).total_cents)
}

#[post("/checkout")]
pub fn post_checkout(cart: Cart) -> Result<String, String> {
    let order = checkout::place_order(&cart)?;
    Ok(format!("{{\"order\": {}, \"total_cents\": {}}}", order.id, order.total_cents))
}

#[post("/inventory/{sku}/restock")]
pub fn post_restock(sku: String, quantity: u32) -> String {
    format!("{{\"stock\": {}}}", inventory::restock(&sku, quantity))
}
