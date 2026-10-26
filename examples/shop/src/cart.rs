//! The shopping cart.

use crate::pricing::apply_discount;
use crate::tax;

pub struct Line {
    pub sku: String,
    pub price_cents: u64,
    pub quantity: u32,
}

pub struct Cart {
    pub lines: Vec<Line>,
    pub coupon_percent: u8,
}

impl Cart {
    pub fn new() -> Self {
        Cart { lines: Vec::new(), coupon_percent: 0 }
    }

    pub fn add(&mut self, sku: &str, price_cents: u64, quantity: u32) {
        self.lines.push(Line { sku: sku.to_string(), price_cents, quantity });
    }

    pub fn subtotal(&self) -> u64 {
        self.lines.iter().map(|l| l.price_cents * l.quantity as u64).sum()
    }

    /// Subtotal, minus the coupon, plus VAT.
    pub fn total(&self) -> u64 {
        let discounted = apply_discount(self.subtotal(), self.coupon_percent);
        discounted + tax::vat(discounted)
    }
}
