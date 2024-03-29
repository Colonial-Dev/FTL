use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;

use crossbeam::channel::{Receiver, Sender};
use crossbeam::queue::ArrayQueue;
use rusqlite::{Connection, OpenFlags};

use crate::prelude::*;

/// A shareable, threadsafe SQLite connection pool.
#[derive(Debug)]
pub struct Pool {
    queue: ArrayQueue<Connection>,
    loopback: Weak<Self>,
    path: PathBuf,
    flags: OpenFlags,
}

impl Pool {
    pub fn open(path: impl Into<PathBuf>, size: usize, flags: OpenFlags) -> Arc<Self> {
        // New pools do not actually hold any connections; they are lazily created
        // later as needed.
        Arc::new_cyclic(|loopback| Self {
            queue: ArrayQueue::new(size),
            loopback: loopback.clone(),
            path: path.into(),
            flags,
        })
    }

    pub fn get(&self) -> Result<PoolConnection> {
        // Pools cannot be "exhausted" - they will always try to open a new connection
        // even if the pool is empty or if doing so would cause there to be more "live"
        // connections than it can actually hold.
        let connection = match self.queue.pop() {
            Some(conn) => conn,
            None => self.make_new()?,
        };

        Ok(PoolConnection {
            parent: self.loopback.clone(),
            connection: Some(connection),
        })
    }

    fn make_new(&self) -> Result<Connection> {
        let new = Connection::open_with_flags(&self.path, self.flags)?;
        new.execute_batch(super::PRAGMAS)?;
        Ok(new)
    }

    fn put_back(&self, conn: Connection) {
        if self.queue.is_full() {
            warn!("Tried to put back a pooled connection, but the pool is full - dropping excess.");
        } else {
            let _ = self.queue.push(conn);
        }
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        debug!("Dropping connection pool at path {:?}...", self.path);
        while let Some(conn) = self.queue.pop() {
            // SQLite recommends calling the optimize PRAGMA immediately before
            // closing database connections.
            let _ = conn.execute("PRAGMA optimize;", []);

            // If this is the last connection, make a best-effort attempt to flush the WAL logs.
            if self.queue.is_empty() {
               let _ = conn.execute("PRAGMA wal_checkpoint(FULL);", []);
            }
        }
    }
}

/// Smart wrapper for an [`rusqlite::Connection`]. Typically (although not always) handed out by a [`Pool`].
pub struct PoolConnection {
    /// The [`Pool`] the connection was obtained from, if any.
    parent: Weak<Pool>,
    /// The inner [`rusqlite::Connection`].]
    /// 
    /// Wrapped in an [`Option`] so it can be taken during drop and returned to its parent pool.
    ///
    /// This field should *never* observably contain [`None`], as the implementation of [`Deref`]
    /// on this type will unwrap it and cause a panic.
    connection: Option<Connection>,
}

impl PoolConnection {
    /// Opens a new read/write connection to the database at the provided path.
    ///
    /// Connections opened in this manner have no parent pool, and as such will be closed
    /// when dropped.
    #[allow(dead_code)]
    pub fn open<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let connection = Connection::open(path)?;
        connection.execute_batch(super::PRAGMAS)?;

        Ok(Self {
            parent: Weak::new(),
            connection: Some(connection),
        })
    }

    /// Opens a new connection to the database at the provided path and with the provided flags.
    ///
    /// Connections opened in this manner have no parent pool, and as such will be closed
    /// when dropped.
    #[allow(dead_code)]
    pub fn open_with_flags<P>(path: P, flags: OpenFlags) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let connection = Connection::open_with_flags(path, flags)?;
        connection.execute_batch(super::PRAGMAS)?;

        Ok(Self {
            parent: Weak::new(),
            connection: Some(connection),
        })
    }

    /// Prepares a "consumer" - a thread and MPSC pair for safely handling concurrent writes to the database.
    ///
    /// The caller is responsible for providing a handler closure that implements
    /// the desired behavior from scratch.
    pub fn prepare_consumer<M, R, F>(mut self, handler: F) -> (JoinHandle<R>, Sender<M>)
    where
        M: Send + 'static,
        R: Send + 'static,
        F: FnOnce(&mut Self, Receiver<M>) -> R + Send + 'static,
    {
        let (tx, rx) = crossbeam::channel::unbounded();

        let handle = std::thread::spawn(move || handler(&mut self, rx));

        (handle, tx)
    }
}

impl Deref for PoolConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.connection.as_ref().unwrap()
    }
}

impl DerefMut for PoolConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.connection.as_mut().unwrap()
    }
}

impl Drop for PoolConnection {
    fn drop(&mut self) {
        if let Some(pool) = self.parent.upgrade() {
            let conn = self.connection.take().unwrap();
            pool.put_back(conn);
        } else {
            warn!(
                "Attempted to put back a pooled connection, but its parent pool no longer exists."
            );
            let _ = self.execute("PRAGMA optimize;", []);
        }
    }
}