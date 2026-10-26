//! Prices and discounts. Amounts are in cents.

/// Applies a percentage discount to a price in cents.
pub fn apply_discount(price_cents: u64, percent: u8) -> u64 {
    price_cents - price_cents * percent as u64 / 100
}

/// Members always get 10% off.
pub fn member_price(price_cents: u64) -> u64 {
    apply_discount(price_cents, 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_percent_off() {
        assert_eq!(apply_discount(1000, 10), 900);
    }
}
