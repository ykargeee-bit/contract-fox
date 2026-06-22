use rusqlite::{Connection, params};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug)]
pub struct NewDonation {
    pub tx_hash: String,
    pub campaign_id: String,
    pub donor_address: String,
    pub donor_user_id: Option<i64>,
    pub amount: u64,
    pub status: String,
}

#[derive(Debug, PartialEq)]
pub struct Donation {
    pub id: i64,
    pub tx_hash: String,
    pub campaign_id: String,
    pub donor_address: String,
    pub donor_user_id: Option<i64>,
    pub amount: u64,
    pub status: String,
    pub created_at: String,
}

pub struct DonationsRepo {
    conn: Connection,
}

impl DonationsRepo {
    pub fn new(conn: Connection) -> Result<Self, DbError> {
        let repo = Self { conn };
        repo.init_schema()?;
        Ok(repo)
    }

    fn init_schema(&self) -> Result<(), DbError> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS donations (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                tx_hash         TEXT    NOT NULL UNIQUE,
                campaign_id     TEXT    NOT NULL,
                donor_address   TEXT    NOT NULL,
                donor_user_id    INTEGER,
                amount          INTEGER NOT NULL,
                status          TEXT    NOT NULL,
                created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            )",
            [],
        )?;
        Ok(())
    }

    /// Save a donation. Returns the existing record if `tx_hash` already exists (idempotent).
    pub fn save_donation(&self, donation: &NewDonation) -> Result<Donation, DbError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO donations
                (tx_hash, campaign_id, donor_address, donor_user_id, amount, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
            params![
                donation.tx_hash,
                donation.campaign_id,
                donation.donor_address,
                donation.donor_user_id,
                donation.amount,
                donation.status,
            ],
        )?;

        let record = self.conn.query_row(
            "SELECT id, tx_hash, campaign_id, donor_address, donor_user_id, amount, status, created_at
             FROM donations WHERE tx_hash = ?1",
            params![donation.tx_hash],
            |row| {
                Ok(Donation {
                    id: row.get(0)?,
                    tx_hash: row.get(1)?,
                    campaign_id: row.get(2)?,
                    donor_address: row.get(3)?,
                    donor_user_id: row.get(4)?,
                    amount: row.get(5)?,
                    status: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        )?;

        Ok(record)
    }

    /// Get all donations for a campaign, with anonymous display name logic
    pub fn get_campaign_donations(
        &self,
        campaign_id: &str,
    ) -> Result<Vec<(String, u64, String)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT donor_address, donor_user_id, amount, created_at 
             FROM donations 
             WHERE campaign_id = ?1 AND status = 'confirmed'
             ORDER BY created_at DESC",
        )?;

        let donations = stmt.query_map(params![campaign_id], |row| {
            let donor_address: String = row.get(0)?;
            let donor_user_id: Option<i64> = row.get(1)?;
            let amount: u64 = row.get(2)?;
            let created_at: String = row.get(3)?;

            // If no user is linked, display as "Anonymous Donor"
            let display_name = if donor_user_id.is_none() {
                "Anonymous Donor".to_string()
            } else {
                // For registered users, we could look up their username, but here we just use the address
                // In a real app, you would join with a users table to get the username
                donor_address
            };

            Ok((display_name, amount, created_at))
        })?;

        let mut results = Vec::new();
        for donation in donations {
            results.push(donation?);
        }

        Ok(results)
    }

    /// Get campaign stats (total raised, donation count)
    pub fn get_campaign_stats(&self, campaign_id: &str) -> Result<(u64, u64), DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(SUM(amount), 0) as total_raised, COUNT(*) as donation_count
             FROM donations 
             WHERE campaign_id = ?1 AND status = 'confirmed'",
        )?;

        let (total_raised, donation_count) = stmt.query_row(params![campaign_id], |row| {
            Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?))
        })?;

        Ok((total_raised, donation_count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_repo() -> DonationsRepo {
        DonationsRepo::new(Connection::open_in_memory().unwrap()).unwrap()
    }

    fn sample() -> NewDonation {
        NewDonation {
            tx_hash: "abc123".to_string(),
            campaign_id: "campaign-1".to_string(),
            donor_address: "GABC...".to_string(),
            donor_user_id: Some(1),
            amount: 1000,
            status: "confirmed".to_string(),
        }
    }

    #[test]
    fn saves_anonymous_donation() {
        let repo = in_memory_repo();
        let anonymous_donation = NewDonation {
            tx_hash: "anonymous456".to_string(),
            campaign_id: "campaign-1".to_string(),
            donor_address: "GANON...".to_string(),
            donor_user_id: None,
            amount: 500,
            status: "confirmed".to_string(),
        };
        let saved = repo.save_donation(&anonymous_donation).unwrap();
        assert_eq!(saved.tx_hash, "anonymous456");
        assert_eq!(saved.donor_user_id, None);
        assert_eq!(saved.amount, 500);
    }

    #[test]
    fn get_campaign_donations_displays_anonymous() {
        let repo = in_memory_repo();
        // Add a registered user donation
        repo.save_donation(&sample()).unwrap();
        // Add an anonymous donation
        let anonymous_donation = NewDonation {
            tx_hash: "anonymous456".to_string(),
            campaign_id: "campaign-1".to_string(),
            donor_address: "GANONXYZ...".to_string(),
            donor_user_id: None,
            amount: 500,
            status: "confirmed".to_string(),
        };
        repo.save_donation(&anonymous_donation).unwrap();

        let donations = repo.get_campaign_donations("campaign-1").unwrap();
        assert_eq!(donations.len(), 2);

        // Check that we have both donations, regardless of order (timestamps might be identical)
        let has_anonymous = donations
            .iter()
            .any(|(name, amount, _)| name == "Anonymous Donor" && *amount == 500);
        let has_registered = donations
            .iter()
            .any(|(name, amount, _)| name == "GABC..." && *amount == 1000);

        assert!(has_anonymous, "Anonymous donation not found");
        assert!(has_registered, "Registered user donation not found");
    }

    #[test]
    fn get_campaign_stats_calculates_correctly() {
        let repo = in_memory_repo();
        repo.save_donation(&sample()).unwrap();
        let anonymous_donation = NewDonation {
            tx_hash: "anonymous456".to_string(),
            campaign_id: "campaign-1".to_string(),
            donor_address: "GANONXYZ...".to_string(),
            donor_user_id: None,
            amount: 500,
            status: "confirmed".to_string(),
        };
        repo.save_donation(&anonymous_donation).unwrap();

        let (total_raised, donation_count) = repo.get_campaign_stats("campaign-1").unwrap();
        assert_eq!(total_raised, 1500);
        assert_eq!(donation_count, 2);
    }

    #[test]
    fn saves_new_donation() {
        let repo = in_memory_repo();
        let saved = repo.save_donation(&sample()).unwrap();
        assert_eq!(saved.tx_hash, "abc123");
        assert_eq!(saved.amount, 1000);
        assert_eq!(saved.status, "confirmed");
    }

    #[test]
    fn duplicate_tx_hash_returns_existing() {
        let repo = in_memory_repo();
        let first = repo.save_donation(&sample()).unwrap();
        let second = repo.save_donation(&sample()).unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(first.tx_hash, second.tx_hash);
    }

    #[test]
    fn different_tx_hashes_are_independent() {
        let repo = in_memory_repo();
        let a = repo.save_donation(&sample()).unwrap();
        let b = repo
            .save_donation(&NewDonation {
                tx_hash: "xyz789".to_string(),
                ..sample()
            })
            .unwrap();
        assert_ne!(a.id, b.id);
    }
}
