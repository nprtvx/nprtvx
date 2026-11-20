//! SQLite-backed client state.
//!
//! Store only device-local application state here. MLS private state must use
//! secure platform storage and an audited OpenMLS storage provider.

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

pub struct ClientStore {
    pool: SqlitePool,
}

impl ClientStore {
    pub async fn open(path: &str) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(path)
            .await?;
        sqlx::migrate!("../../migrations").run(&pool).await
            .map_err(|error| sqlx::Error::Migrate(Box::new(error)))?;
        Ok(Self { pool })
    }

    pub async fn put_cached_message(
        &self,
        message_id: &str,
        group_id: &str,
        ciphertext: &[u8],
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT OR REPLACE INTO messages(message_id, group_id, ciphertext) VALUES (?, ?, ?)",
        )
        .bind(message_id)
        .bind(group_id)
        .bind(ciphertext)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
