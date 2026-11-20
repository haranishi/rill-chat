use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::model::{ApiError, Member, Result, User, now_ms};
use rusqlite::{Connection, OptionalExtension};

#[derive(Clone)]
pub struct Database(Arc<Mutex<Connection>>);

impl Database {
    pub fn open(path: &Path) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        #[cfg(unix)]
        if path != Path::new(":memory:") {
            use std::{fs::OpenOptions, io::ErrorKind, os::unix::fs::OpenOptionsExt};
            // SQLite creates WAL files with the same permissions as the database.
            let created = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path);
            match created {
                Ok(_) => (),
                Err(e) if e.kind() == ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.into()),
            }
        }
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        match version {
            0 => {
                let tx = conn.transaction()?;
                tx.execute_batch(include_str!("schema.sql"))?;
                tx.commit()?;
            }
            1 => (),
            _ => return Err(format!("Unsupported database schema version {version}").into()),
        }
        Ok(Self(Arc::new(Mutex::new(conn))))
    }

    pub async fn run<T, F>(&self, work: F) -> Result<T>
    where
        F: FnOnce(&mut Connection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().map_err(|_| ApiError::internal())?;
            work(&mut guard)
        })
        .await
        .map_err(|_| ApiError::internal())?
    }

    pub async fn for_user<T, F>(&self, key: String, work: F) -> Result<T>
    where
        F: FnOnce(&mut Connection, Member) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        self.run(move |c| {
            let member = c
                .query_row(
                    "SELECT u.id,u.name,u.handle,u.workspace_id,s.expires_at FROM sessions s
                 JOIN users u ON u.id=s.user_id WHERE s.token_hash=? AND s.expires_at>?",
                    (key, now_ms()),
                    |r| {
                        Ok(Member {
                            user: User::from_row(r)?,
                            workspace: r.get(3)?,
                            expires_at: r.get(4)?,
                        })
                    },
                )
                .optional()?
                .ok_or_else(ApiError::unauthorized)?;
            work(c, member)
        })
        .await
    }
}
