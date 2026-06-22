/// Utility functions for contract operations

/// Validate amount is positive
pub fn validate_amount(amount: i128) -> bool {
    amount > 0
}

/// Validate donation amount (must be positive)
pub fn validate_donation_amount(amount: i128) -> Result<(), &'static str> {
    if amount <= 0 {
        Err("Donation amount must be positive")
    } else {
        Ok(())
    }
}

/// Calculate fee from amount (1% default)
pub fn calculate_fee(amount: i128) -> i128 {
    (amount * 1) / 100
}

/// Calculate net amount after fee
pub fn calculate_net_amount(amount: i128) -> i128 {
    amount - calculate_fee(amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_amount() {
        assert!(validate_amount(100));
        assert!(!validate_amount(0));
        assert!(!validate_amount(-100));
    }

    #[test]
    fn test_validate_donation_amount() {
        assert!(validate_donation_amount(100).is_ok());
        assert!(validate_donation_amount(1).is_ok());
        assert_eq!(
            validate_donation_amount(0),
            Err("Donation amount must be positive")
        );
        assert_eq!(
            validate_donation_amount(-100),
            Err("Donation amount must be positive")
        );
    }

    #[test]
    fn test_calculate_fee() {
        assert_eq!(calculate_fee(100), 1);
        assert_eq!(calculate_fee(1000), 10);
    }

    #[test]
    fn test_calculate_net_amount() {
        assert_eq!(calculate_net_amount(100), 99);
        assert_eq!(calculate_net_amount(1000), 990);
    }
}
