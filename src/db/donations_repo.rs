use rusqlite::{params, Connection};
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
    pub amount: u64,
    pub status: String,
}

#[derive(Debug, PartialEq)]
pub struct Donation {
    pub id: i64,
    pub tx_hash: String,
    pub campaign_id: String,
    pub donor_address: String,
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
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS donations (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                tx_hash         TEXT    NOT NULL UNIQUE,
                campaign_id     TEXT    NOT NULL,
                donor_address   TEXT    NOT NULL,
                amount          INTEGER NOT NULL,
                status          TEXT    NOT NULL,
                created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );",
        )?;
        Ok(())
    }

    /// Save a donation. Returns the existing record if `tx_hash` already exists (idempotent).
    pub fn save_donation(&self, donation: &NewDonation) -> Result<Donation, DbError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO donations
                (tx_hash, campaign_id, donor_address, amount, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
            params![
                donation.tx_hash,
                donation.campaign_id,
                donation.donor_address,
                donation.amount,
                donation.status,
            ],
        )?;

        let record = self.conn.query_row(
            "SELECT id, tx_hash, campaign_id, donor_address, amount, status, created_at
             FROM donations WHERE tx_hash = ?1",
            params![donation.tx_hash],
            |row| {
                Ok(Donation {
                    id: row.get(0)?,
                    tx_hash: row.get(1)?,
                    campaign_id: row.get(2)?,
                    donor_address: row.get(3)?,
                    amount: row.get(4)?,
                    status: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        )?;

        Ok(record)
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
            amount: 1000,
            status: "confirmed".to_string(),
        }
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
