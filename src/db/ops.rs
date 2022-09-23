use std::path::Path;
use std::io::ErrorKind;

use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, params};

use crate::error::DbError;

use super::{KNOWN_TABLES, DbPool};

/// Attempt to open a connection to an SQLite database at the given path.
pub fn make_connection(path: &Path) -> Result<Connection, DbError> {
    let conn = Connection::open(path)?;
    Ok(conn)
}

/// Attempt to create a connection pool for an SQLite database at the given path.
pub fn make_pool(path: &Path) -> Result<DbPool, DbError> {
    let on_init = |db: &mut Connection| {
        db.pragma_update(None, "journal_mode", &"WAL".to_string())?;
        Ok(())
    };

    let manager = SqliteConnectionManager::file(path).with_init(on_init);
    let pool = r2d2::Pool::new(manager)?;
    Ok(pool)
}

/// Try and create a new SQLite database at the given path. Fails if the database file already exists.
pub fn try_create_db(path: &Path) -> Result<Connection, DbError> {
    if path.exists() {
        let e = std::io::Error::new(
            ErrorKind::AlreadyExists, 
            "Database file already exists."
        );
        return Err(e.into());
    }

    // Calling open() implicitly creates the database if it does not exist.
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", &"WAL".to_string())?;
    try_initialize_tables(&conn)?;
    Ok(conn)
}

/// Try to create all FTL-specific tables in the given database. Does not fail if any of the tables already exist.
pub fn try_initialize_tables(conn: &Connection) -> Result<(), DbError> {
    conn.execute("
        CREATE TABLE IF NOT EXISTS input_files (
            id TEXT PRIMARY KEY,
            path TEXT,
            hash TEXT,
            extension TEXT,
            contents TEXT,
            inline INTEGER,
            UNIQUE(id)
        );
    ", [])?;

    conn.execute("
        CREATE TABLE IF NOT EXISTS revision_files (
            revision TEXT,
            id TEXT
            UNIQUE(revision, id)
        );
    ", [])?;

    conn.execute("
        CREATE TABLE IF NOT EXISTS pages (
            id TEXT PRIMARY KEY,
            route TEXT,
            offset INTEGER,
            title TEXT,
            date TEXT,
            publish_date TEXT,
            expire_date TEXT,
            description TEXT,
            summary TEXT,
            template TEXT,
            draft INTEGER,
            tags TEXT,
            collections TEXT,
            aliases TEXT,
            UNIQUE(id)
        );
    ", [])?;

    conn.execute("
        CREATE TABLE IF NOT EXISTS routes (
            revision TEXT,
            id TEXT,
            path TEXT,
            parent_path TEXT,
            kind INTEGER,
            UNIQUE(
                revision,
                id,
                path,
                parent_path,
                kind
            )
        );
    ", [])?;

    conn.execute("
        CREATE TABLE IF NOT EXISTS page_aliases (
            route TEXT,
            id TEXT,
            UNIQUE(route, id)
        );
    ", [])?;

    conn.execute("
        CREATE TABLE IF NOT EXISTS page_tags (
            tag TEXT UNIQUE,
        );
    ", [])?;

    Ok(())
}

/// Try to clear all rows from all FTL tables (via `DELETE FROM table`). Leaves table schemas unchanged.
pub fn try_clear_tables(conn: &Connection) -> Result<(), DbError> {
    let mut stmt = conn.prepare("
        DELETE FROM ?1;
    ")?;

    for table in KNOWN_TABLES {
        stmt.execute(params![table])?;
    }

    Ok(())
}

/// Try to drop and recreate all FTL tables (using [`try_initialize_tables`]).
pub fn try_reset_tables(conn: &Connection) -> Result<(), DbError> {
    let mut stmt = conn.prepare("
        DROP TABLE ?1;
    ")?;

    for table in KNOWN_TABLES {
        stmt.execute(params![table])?;
    }

    try_initialize_tables(conn)?;

    Ok(())
}