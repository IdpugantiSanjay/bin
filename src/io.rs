use rand::{distributions::Alphanumeric, thread_rng, Rng};
use std::cell::RefCell;
use sqlx::{prelude::FromRow, sqlite::SqlitePool};

#[derive(Debug, FromRow)]
pub struct Paste {
    pub title: String,
    pub content: String,
    pub id: u8,
}


#[derive(Clone)]
pub struct Store(SqlitePool);

impl Store {
    pub fn new(pool: SqlitePool) -> Self {
        Store(pool)
    }

    pub async fn insert(&self, title: &str, content: &str) -> Result<Paste, sqlx::Error> {
        let paste = sqlx::query_as::<_, Paste>("INSERT INTO pastes (title, content) VALUES (?, ?) RETURNING *")
            .bind(title)
            .bind(content)
            .fetch_one(&self.0)
            .await?;
        Ok(paste)
    }

    pub async fn get_paste_by_id(&self, id: &u8) -> Result<Paste, sqlx::Error> {
        let paste = sqlx::query_as::<_, Paste>("SELECT id, title, content FROM pastes WHERE id = ?")
        .bind(id)
        .fetch_one(&self.0)
        .await?;
        Ok(paste)
    }

    pub async fn get_paste_by_title(&self, title: &str) -> Result<Paste, sqlx::Error> {
        let paste = sqlx::query_as::<_, Paste>("SELECT id, title, content FROM pastes WHERE title = ?")
        .bind(title)
        .fetch_one(&self.0)
        .await?;
        Ok(paste)
    }

    pub async fn delete_paste_by_id(&self, id: &u8) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM pastes WHERE id = ?")
            .bind(id)
            .execute(&self.0)
            .await
            .map(|_| ())
    }

    pub async fn delete_paste_by_title(&self, title: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM pastes WHERE title = ?")
            .bind(title)
            .execute(&self.0)
            .await
            .map(|_| ())
    }

    pub async fn get_all_pastes(&self) -> Result<Vec<Paste>, sqlx::Error> {
        let pastes = sqlx::query_as::<_, Paste>("SELECT id, title, content FROM pastes")
            .fetch_all(&self.0)
            .await?;
        Ok(pastes)
    }

    pub async fn update_paste_content(&self, title: &str, content: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE pastes SET content = ? WHERE title = ?")
            .bind(content)
            .bind(title)
            .execute(&self.0)
            .await
            .map(|_| ())
    }
}

/// Generates a 'pronounceable' random ID using gpw
pub fn generate_id() -> String {
    thread_local!(static KEYGEN: RefCell<gpw::PasswordGenerator> = RefCell::new(gpw::PasswordGenerator::default()));

    KEYGEN.with(|k| k.borrow_mut().next()).unwrap_or_else(|| {
        thread_rng()
            .sample_iter(&Alphanumeric)
            .take(6)
            .map(char::from)
            .collect()
    })
}
