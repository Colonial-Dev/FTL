//! Types and traits for interacting with the database underlying FTL.
//!
//! This module includes:
//! - The [`Database`] type, a shareable top-level portal for acquiring connections and managing write contention.
//! - The [`Model`] trait, as well as their associated "model types" (such as [`InputFile`]) that map to and from tables in the database.
//! - Macros ([`model`](crate::model) and [`record`](crate::record)) for creating model and record types.

mod model;
mod pool;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

pub use model::*;
pub use pool::{Pool, PoolConnection as Connection};

pub use rusqlite::{
    OpenFlags,
    Statement,
    params,
    named_params
};

use crate::record;
use crate::prelude::*;

pub const SCHEMA_VERSION: i64 = 1;

pub const AUX_UP: &str = include_str!("sql/aux_up.sql");
pub const AUX_DOWN: &str = "DETACH DATABASE map;";
pub const PRAGMAS: &str = include_str!("sql/pragmas.sql");
pub const PRIME_DOWN: &str = include_str!("sql/prime_down.sql");
pub const PRIME_UP: &str = include_str!("sql/prime_up.sql");

#[cfg(test)]
pub const IN_MEMORY: &str = ":memory:";

#[derive(Debug)]
pub struct Database {
    pub path: PathBuf,
    pub rw_pool: Arc<Pool>,
    pub ro_pool: Arc<Pool>,
    write_lock: Mutex<()>,
}

impl Database {
    pub fn create(path: impl AsRef<Path>) -> Result<Connection> {
        match path.as_ref().exists() {
            true => bail!("Cannot initialize database - path already exists."),
            false => {
                let conn = Connection::open(path)?;
                conn.execute_batch(PRIME_UP)?;
                conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
                Ok(conn)
            }
        }
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();

        let rw_pool = Pool::open(
            &path, 
            *THREADS as usize, 
            OpenFlags::SQLITE_OPEN_READ_WRITE
        );

        let ro_pool = Pool::open(
            &path,
            BLOCKING_THREADS as usize,
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        );

        let version = ro_pool.get()?.pragma_query_value(
            None,
            "user_version",
            |row| row.get::<_, i64>(0)
        )?;

        if version != SCHEMA_VERSION {
            let err = eyre!("Database schema is not compatible with this version of FTL.")
                .note("Expected version {SCHEMA_VERSION}, got {version}.")
                .suggestion("You may need to run `db clear` to update the schema.");

            bail!(err);
        }

        Ok(Self {
            path,
            rw_pool,
            ro_pool,
            write_lock: Mutex::new(()),
        })
    }

    pub fn compress(&self) -> Result<()> {
        let _guard = self.write_lock();
        let conn = self.get_rw()?;

        conn.execute(
            "DELETE FROM revisions
            WHERE pinned = FALSE
            AND time NOT IN (
                SELECT MAX(time) FROM revisions
            )",
            []
        )?;

        conn.execute(
            "DELETE FROM input_files
            WHERE id NOT IN (
                SELECT id FROM revision_files
            )",
            []
        )?;

        record! {
            id => String
        }
        
        let mut all_ids = conn.prepare("
            SELECT id FROM input_files;
        ")?;

        let set = all_ids
            .query_and_then([], Record::from_row)?
            .try_fold(HashSet::new(), |mut acc, row| -> Result<_> {
                acc.insert(
                    PathBuf::from(row?.id)
                );
                Ok(acc)
            })?;
        
        for entry in std::fs::read_dir(SITE_CACHE_PATH)? {
            let path = entry?.path();

            if !set.contains(&path) {
                std::fs::remove_file(&path)?;
            }
        }

        conn.execute("VACUUM;", [])?;
        conn.execute("PRAGMA wal_checkpoint(FULL);", [])?;

        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        let _guard = self.write_lock();
        let conn = self.get_rw()?;

        conn.execute_batch(PRIME_DOWN)?;
        conn.execute_batch(PRIME_UP)?;
        conn.execute("VACUUM;", [])?;

        std::fs::remove_dir_all(SITE_CACHE_PATH)?;
        std::fs::create_dir_all(SITE_CACHE_PATH)?;

        Ok(())
    }

    pub fn stat(&self) -> Result<()> {
        todo!()
    }

    /// Acquire a read-write connection from the underlying pool, creating a new one
    /// if it does not exist.
    pub fn get_rw(&self) -> Result<Connection> {
        self.rw_pool.get()
    }

    /// Acquire a read-only connection from the underlying pool, creating a new one
    /// if it does not exist.
    pub fn get_ro(&self) -> Result<Connection> {
        self.ro_pool.get()
    }

    /// Block until the database write lock is free, then yield its guard.
    pub fn write_lock(&self) -> MutexGuard<()> {
        self.write_lock
            .lock()
            .expect("Database write lock should not be poisoned.")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn migrations() {
        Database::create(IN_MEMORY)
            .unwrap()
            .execute_batch(PRIME_DOWN)
            .unwrap();
    }
}
