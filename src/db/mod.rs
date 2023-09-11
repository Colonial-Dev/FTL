//! Types and traits for interacting with the database underlying FTL.
//!
//! This module includes:
//! - The [`Database`] type, a shareable top-level portal for acquiring connections and managing write contention.
//! - The [`Insertable`] and [`Queryable`] traits, as well as their associated "model types" (such as [`InputFile`]) that map to and from tables in the database.

mod model;
mod pool;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

pub use model::*;
pub use pool::{Pool, PoolConnection as Connection};
pub use sqlite::{OpenFlags, Statement};

use crate::prelude::*;

pub const AUX_UP: &str = include_str!("sql/aux_up.sql");
pub const AUX_DOWN: &str = "DETACH DATABASE map;";
pub const PRAGMAS: &str = include_str!("sql/pragmas.sql");
pub const PRIME_DOWN: &str = include_str!("sql/prime_down.sql");
pub const PRIME_UP: &str = include_str!("sql/prime_up.sql");

#[cfg(test)]
pub const IN_MEMORY: &str = ":memory:";

pub const NO_PARAMS: Option<&[()]> = None;
pub const DEFAULT_QUERY: Option<&str> = None;

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
                conn.execute(PRIME_UP)?;
                Ok(conn)
            }
        }
    }

    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();

        let rw_pool = Pool::open(
            &path, 
            *THREADS as usize, 
            OpenFlags::new().set_read_write()
        );

        let ro_pool = Pool::open(
            &path,
            BLOCKING_THREADS as usize,
            OpenFlags::new().set_read_only(),
        );

        Self {
            path,
            rw_pool,
            ro_pool,
            write_lock: Mutex::new(()),
        }
    }

    pub fn compress(&self) -> Result<()> {
        todo!()
    }

    pub fn clear(&self) -> Result<()> {
        let _guard = self.write_lock();
        let conn = self.get_rw()?;

        conn.execute(PRIME_DOWN)?;
        conn.execute(PRIME_UP)?;

        Ok(())
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
            .execute(PRIME_DOWN)
            .unwrap();
    }
}
