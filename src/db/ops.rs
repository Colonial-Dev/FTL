use std::path::Path;

use once_cell::sync::Lazy;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection};
use rusqlite_migration::{Migrations, M};

use super::DbPool;
use crate::prelude::*;

const DB_PATH: &str = ".ftl/content.db";
const MAP_INIT_QUERY: &str = include_str!("sql/aux_up.sql");

static PRIME_MIGRATIONS: Lazy<Migrations> = Lazy::new(|| {
    Migrations::new(vec![
        M::up(include_str!("sql/prime_up.sql"))
            .down(include_str!("sql/prime_down.sql"))
    ])
});

/// Try and create a new SQLite database at the given path. Fails if the database file already exists.
pub fn try_create_db(path: &Path) -> Result<Connection> {
    if path.exists() {
        bail!("Database file already exists.");
    }

    // Calling open() implicitly creates the database if it does not exist.
    let mut conn = Connection::open(path)?;
    
    // WAL grants faster performance and allows reads that are concurrent to writes.
    // NORMAL synchronization is safe with WAL enabled, and gives an extra speed boost
    // by minimizing filesystem IO.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    PRIME_MIGRATIONS.to_latest(&mut conn)?;

    Ok(conn)
}

/// Attempt to open a connection to the SQLite database.
pub fn make_connection() -> Result<Connection> {
    let conn = Connection::open(DB_PATH)?;
    Ok(conn)
}

/// Attempt to create a connection pool for an SQLite database at the given path.
pub fn make_pool() -> Result<DbPool> {
    let manager = SqliteConnectionManager::file(DB_PATH)
        .with_init(|c| {
            c.pragma_update(None, "journal_mode", "WAL")?;
            c.pragma_update(None, "synchronous", "NORMAL")?;
            c.pragma_update(None, "foreign_keys", "ON")
        });

    let pool = r2d2::Pool::builder()
        .max_size(*THREADS)
        .build(manager)?;

    Ok(pool)
}

pub fn attach_mapping_database(conn: &Connection) -> Result<()> {
    enumerate_static_queries(conn, MAP_INIT_QUERY)
}

pub fn detach_mapping_database(conn: &Connection) -> Result<()> {
    conn.execute("DETACH DATABASE map;", [])?;
    Ok(())
}

pub fn dump_input_for_revision(conn: &Connection, rev_id: &str) -> Result<()> {
    let mut stmt = conn.prepare("
        SELECT input_files.* FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
    ")?;

    // Basically:
    // 1. Get all input files for the revision
    // 2. Iterate over them, matching on their "inline" value to determine
    //    if we need to fetch from the flat-file cache.
    // 3. Append the file's original path onto that of the dump directory,
    //    and write/copy its data to that location.

    // For dumping *output*, we instead get all the *routes* for the revision.
    // From there, we match on its kind value to determine where to fetch the data from
    // (either the `output` table or the flat-file cache), then write the file to the dump
    // target at its route.

    //from_rows::<InputFile>(stmt.query(params![&rev_id])?)

    todo!()
}

/// Try to drop and recreate all FTL tables.
pub fn try_reset_tables(conn: &mut Connection) -> Result<()> {
    PRIME_MIGRATIONS.to_version(conn, 0)?;
    PRIME_MIGRATIONS.to_latest(conn)?;
    Ok(())
}

/// Tries to drop all information from the database that is not relevant for the current active revision.
/// Under the hood, this consists of some `SELECT` and `DELETE FROM` operations followed by a `VACUUM` call.
pub fn try_compress_db(conn: &Connection) -> Result<()> {
    // The goal here is to delete as much from the database as we possibly can, within
    // the constraints set by the user.
    // Specifically, this means we have to keep both the input and output of any pinned revisions.
    //
    // Clearing unwanted revisions is simple; simply delete all non-pinned rows, and FOREIGN KEY cascades
    // will handle the rest.
    // For input files, we need to query only for IDs not associated with any pinned revision, and delete 
    // just those (once again, cascades will do the rest.)
    todo!()
}

/// Tries to delete all files from the cache that are not relevant for the current active revision.
pub fn try_compress_cache(conn: &Connection) -> Result<()> {
    todo!()
}

fn enumerate_static_queries(conn: &Connection, queries: &'static str) -> Result<()> {
    let queries = queries.split(";\n");

    for query in queries {
        conn.execute(query, [])?;
    }

    Ok(())
}

#[cfg(test)]
mod migrations {
    use super::*;

    #[test]
    fn prime() {
        assert_eq!(PRIME_MIGRATIONS.validate(), Ok(()));
    }
}