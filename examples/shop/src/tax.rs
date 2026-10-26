//! Value-added tax.

/// VAT at 20%, in cents.
pub fn vat(amount_cents: u64) -> u64 {
    amount_cents * 20 / 100
}
