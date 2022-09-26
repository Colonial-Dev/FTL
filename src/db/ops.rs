use std::path::Path;
use std::io::ErrorKind;

use anyhow::{Result, Context};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, params};

use super::{KNOWN_TABLES, DbPool};

/// Attempt to open a connection to an SQLite database at the given path.
pub fn make_connection(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    Ok(conn)
}

/// Attempt to create a connection pool for an SQLite database at the given path.
pub fn make_pool(path: &Path) -> Result<DbPool> {
    let manager = SqliteConnectionManager::file(path);
    let pool = r2d2::Pool::new(manager)?;
    Ok(pool)
}

/// Try and create a new SQLite database at the given path. Fails if the database file already exists.
pub fn try_create_db(path: &Path) -> Result<Connection> {
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

/// Try to create all FTL-specific tables in the given database. Does NOT fail if any of the tables already exist.
pub fn try_initialize_tables(conn: &Connection) -> Result<()> {
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
            id TEXT,
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
            dynamic INTEGER,
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
            route TEXT,
            parent_route TEXT,
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
        CREATE TABLE IF NOT EXISTS hypertext (
            revision TEXT,
            input_id TEXT,
            templating_id TEXT,
            content TEXT,
            UNIQUE(
                revision,
                input_id,
                templating_id,
                content
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
            tag TEXT UNIQUE
        );
    ", [])?;

    Ok(())
}

/// Try to clear all rows from all FTL tables (via `DELETE FROM table`). Leaves table schemas unchanged.
pub fn try_clear_tables(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("
        DELETE FROM ?1;
    ")?;

    for table in KNOWN_TABLES {
        stmt.execute(params![table])?;
    }

    Ok(())
}

/// Try to drop and recreate all FTL tables (using [`try_initialize_tables`]).
pub fn try_reset_tables(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("
        DROP TABLE ?1;
    ")?;

    for table in KNOWN_TABLES {
        stmt.execute(params![table])?;
    }

    try_initialize_tables(conn)?;

    Ok(())
}

/// Tries to drop all information from the database that is not relevant for the current active revision.
/// Under the hood, this consists of some `SELECT` and `DELETE FROM` operations followed by a `VACUUM` call.
pub fn try_compress_db(conn: &Connection) -> Result<()> {
    todo!()
}

/// Tries to delete all files from the cache that are not relevant for the current active revision.
pub fn try_compress_cache(conn: &Connection) -> Result<()> {
    todo!()
}