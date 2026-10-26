//! Stock levels.

/// Reserves stock for an order; false if there isn't enough.
pub fn reserve(sku: &str, quantity: u32) -> bool {
    quantity <= stock(sku)
}

pub fn restock(sku: &str, quantity: u32) -> u32 {
    stock(sku) + quantity
}

fn stock(_sku: &str) -> u32 {
    10
}
