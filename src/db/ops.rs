use std::path::Path;
use std::io::ErrorKind;

use anyhow::{Result, Context};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, params};

use super::{KNOWN_TABLES, DbPool};

static DB_INIT_QUERY: &str = include_str!("db_init.in");
static MAP_INIT_QUERY: &str = include_str!("map_init.in");

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

pub fn attach_mapping_database(conn: &Connection) -> Result<()> {
    enumerate_static_queries(conn, MAP_INIT_QUERY)
}

pub fn detach_mapping_database(conn: &Connection) -> Result<()> {
    conn.execute("DETACH DATABASE map;", [])?;
    Ok(())
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
    enumerate_static_queries(conn, DB_INIT_QUERY)
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
    for table in KNOWN_TABLES {
        let query = format!("DROP TABLE IF EXISTS {table};");
        conn.execute(&query, [])?;
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

fn enumerate_static_queries(conn: &Connection, queries: &'static str) -> Result<()> {
    let mut queries = queries.split(";\n");

    while let Some(query) = queries.next() {
        conn.execute(query, [])?;
    }

    Ok(())
}