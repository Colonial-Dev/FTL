use r2d2::{Pool};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection};
use std::path::Path;

pub type DbPool = Pool<SqliteConnectionManager>;
pub type DbConn = rusqlite::Connection;

pub mod data;
pub mod ops;
pub mod error;

use crate::DbError;

pub fn make_db_pool(path: &Path) -> Result<DbPool, DbError> {
    let on_init = |db: &mut Connection| {
        db.pragma_update(None, "journal_mode", &"WAL".to_string())?;
        let mut tables = db.prepare(
            "
            CREATE TABLE IF NOT EXISTS input_files (
                id TEXT PRIMARY KEY,
                path TEXT,
                hash TEXT,
                extension TEXT,
                contents TEXT,
                inline INTEGER,
                UNIQUE(id)
            );

            CREATE TABLE IF NOT EXISTS revision_files (
                revision TEXT,
                id TEXT
                UNIQUE(revision, id)
            );

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

            CREATE TABLE IF NOT EXISTS page_aliases (
                route TEXT,
                id TEXT,
                UNIQUE(route, id)
            );

            CREATE TABLE IF NOT EXISTS page_tags (
                tag TEXT UNIQUE,
            );
            "
            // TODO - we need a "state" table that holds data like the current revision to serve in a single row
        )?;
        tables.execute([])?;
        Ok(())
    };

    let manager = SqliteConnectionManager::file(path).with_init(on_init);
    let pool = Pool::new(manager)?;
    Ok(pool)
}
