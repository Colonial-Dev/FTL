use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;

use crossbeam::channel::{Receiver, Sender};
use crossbeam::queue::ArrayQueue;
use sqlite::{Bindable, Connection, OpenFlags, State};

use super::{Insertable, Queryable};
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
        new.execute(super::PRAGMAS)?;
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
            let _ = conn.execute("PRAGMA optimize;");
        }
    }
}

#[macro_export]
/// Polls an SQLite statement to completion.
macro_rules! poll {
    ($stmt:ident) => {
        while let sqlite::State::Row = $stmt.next()? {}
    };
}

/// Smart wrapper for an [`sqlite::Connection`]. Typically (although not always) handed out by a [`Pool`].
pub struct PoolConnection {
    /// The [`Pool`] the connection was obtained from, if any.
    parent: Weak<Pool>,
    /// The inner [`sqlite::Connection`].
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
        connection.execute(super::PRAGMAS)?;

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
        connection.execute(super::PRAGMAS)?;

        Ok(Self {
            parent: Weak::new(),
            connection: Some(connection),
        })
    }

    /// Creates a "reader" - an iterator that lazily deserializes instances of a [`Queryable`] type
    /// from the results of a database query.
    pub fn prepare_reader<T, Q, P>(
        &self,
        query: Q,
        parameters: Option<P>,
    ) -> Result<impl Iterator<Item = Result<T>> + '_>
    where
        T: Queryable,
        Q: AsRef<str>,
        P: Bindable,
    {
        let mut stmt = self.prepare(query)?;

        if let Some(parameters) = parameters {
            stmt.bind(parameters)?;
        }

        let iterator = std::iter::from_fn(move || {
            use sqlite::State::*;
            match stmt.next().map_err(Report::from) {
                Ok(Row) => Some(T::read_query(&stmt)),
                Ok(Done) => None,
                Err(err) => Err(err).into(),
            }
        });

        Ok(iterator)
    }

    /// Creates a "writer" - a closure that inserts instances of an [`Insertable`] type into the database,
    /// using either the type's default insertion query or a caller-provided substitute.
    pub fn prepare_writer<T, Q, P>(
        &self,
        query: Option<Q>,
        parameters: Option<P>,
    ) -> Result<impl FnMut(&T) -> Result<()> + '_>
    where
        T: Insertable,
        Q: AsRef<str>,
        P: Bindable,
    {
        let mut stmt = match query {
            Some(sub) => self.prepare(sub)?,
            None => self.prepare(T::default_query())?,
        };

        if let Some(parameters) = parameters {
            stmt.bind(parameters)?;
        }

        let closure = move |item: &T| {
            stmt.reset()?;
            item.bind_query(&mut stmt)?;
            poll!(stmt);
            Ok(())
        };

        Ok(closure)
    }

    /// Prepares a "consumer" - a thread and MPSC pair for safely handling concurrent writes to the database.
    ///
    /// Unlike writers and readers, the caller is responsible for providing a handler closure that implements
    /// the desired behavior from scratch.
    pub fn prepare_consumer<M, R, F>(self, handler: F) -> (JoinHandle<R>, Sender<M>)
    where
        M: Send + 'static,
        R: Send + 'static,
        F: FnOnce(&Self, Receiver<M>) -> R + Send + 'static,
    {
        let (tx, rx) = crossbeam::channel::unbounded();

        let handle = std::thread::spawn(move || handler(&self, rx));

        (handle, tx)
    }

    /// Given a query and an optional set of parameters, returns `true` if it returns
    /// one or more rows.
    pub fn exists<Q, P>(&self, query: Q, parameters: Option<P>) -> Result<bool>
    where
        Q: AsRef<str>,
        P: Bindable,
    {
        let mut stmt = self.prepare(query)?;

        if let Some(parameters) = parameters {
            stmt.bind(parameters)?;
        }

        if let State::Row = stmt.next()? {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Open a new transaction on the connection, yielding a [`Transaction`] token
    /// that can be used to commit any changes made or auto-rollback on drop.
    pub fn open_transaction(&self) -> Result<Transaction<'_>> {
        Transaction::new(self)
    }
}

impl Deref for PoolConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.connection.as_ref().unwrap()
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
            let _ = self.execute("PRAGMA optimize;");
        }
    }
}

pub struct Transaction<'a> {
    parent: &'a PoolConnection,
    complete: bool,
}

impl<'a> Transaction<'a> {
    fn new(parent: &'a PoolConnection) -> Result<Self> {
        parent.execute("BEGIN TRANSACTION;")?;

        Ok(Self {
            parent,
            complete: false,
        })
    }

    pub fn commit(mut self) -> Result<()> {
        self.parent.execute("COMMIT;")?;
        self.complete = true;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn rollback(mut self) -> Result<()> {
        self.parent.execute("ROLLBACK;")?;
        self.complete = true;
        Ok(())
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if !self.complete {
            // We have no way to report a rollback error during drop, so we just
            // ignore the possibility.
            let _ = self.parent.execute("ROLLBACK;");
        }
    }
}
