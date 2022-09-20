use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, params, Transaction};
use std::hash::{Hash, Hasher};
use std::path::Path;
use crate::dbdata::*;
use crate::error::*;

pub type DbPool = Pool<SqliteConnectionManager>;
pub type DbConn = PooledConnection<SqliteConnectionManager>;

pub fn update_input_files(pool: &DbPool, items: &[InputFile]) -> Result<(), DbError> {
    let mut conn = pool.get()?;
    let txn = conn.transaction()?;

    into_input_files(&txn, &items[..])?;

    txn.commit()?;
    Ok(())
}

pub fn update_revision_files(pool: &DbPool, items: &[InputFile]) -> Result<String, DbError> {
    let mut conn = pool.get()?;
    let txn = conn.transaction()?;
    let rev_id = compute_revision_id(&items);

    into_revision_files(&txn, &items, &rev_id)?;

    txn.commit()?;
    Ok(rev_id)
}

fn into_input_files(txn: &Transaction, items: &[InputFile]) -> Result<(), DbError> {
    log::info!("Updating input_files table...");

    let conn = &*txn;
    
    for item in items {
        conn.execute("
            INSERT OR IGNORE INTO input_files
            VALUES(:id, :path, :hash, :extension, :contents, :inline);
        ", item.to_params()?.to_slice().as_slice())?;

        if !item.inline {
            log::trace!("Caching non-inline file {:#?}", &item.path);
            let destination = format!(".ftl/cache/{}", &item.hash);
            std::fs::copy(&item.path, Path::new(&destination))?;
        }
    }

    log::info!("Done updating input_files table.");
    Ok(())
}

fn compute_revision_id(items: &[InputFile]) -> String {
    let mut ids: Vec<&str> = Vec::new();

    for item in items {
        ids.push(&item.id)
    }
    
    let mut hasher = seahash::SeaHasher::default();
    ids.hash(&mut hasher);
    let rev_id = format!("{:016x}", hasher.finish());
    
    log::info!("Computed revision ID {}", rev_id);

    rev_id
}

fn into_revision_files(txn: &Transaction, items: &[InputFile], rev_id: &str) -> Result<(), DbError> {
    log::info!("Updating revision_files table...");
    
    let conn = &*txn;

    for item in items {
        conn.execute("
            INSERT INTO revision_files
            VALUES(?1, ?2);
        ", params![&rev_id, &item.id])?;
    }

    log::info!("Done updating revision_files table.");
    Ok(())
}

pub fn update_pages(pool: &DbPool, items: &[Page]) -> Result<(), DbError> {
    let mut conn = pool.get()?;
    let txn = conn.transaction()?;
    
    into_pages(&txn, &items)?;

    txn.commit()?;
    Ok(())
}

fn into_pages(txn: &Transaction, items: &[Page]) -> Result<(), DbError> {
    log::info!("Updating pages table...");

    let conn = &*txn;

    for item in items {
        conn.execute("
            INSERT OR IGNORE INTO pages
            VALUES(:id, :route, :offset, :title, :date, :description, :summary, :tags, 
            :series, :aliases, :template, :draft, :publish_date, :expire_date);
        ", item.to_params()?.to_slice().as_slice())?;
    }

    /*let mut stmt = conn.prepare("
        SELECT * FROM routes
        WHERE kind = 3
    ")?;
    let mut result = serde_rusqlite::from_rows::<Route>(stmt.query([])?);
    let row = result.next();

    println!("{:#?}", row.unwrap());*/

    log::info!("Done updating pages table.");
    Ok(())
}

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
                description TEXT,
                summary TEXT,
                tags TEXT,
                series TEXT,
                aliases TEXT,
                template TEXT,
                draft INTEGER,
                publish_date TEXT,
                expire_date TEXT,
                UNIQUE(
                    id,
                    route,
                    offset,
                    title,
                    date,
                    description,
                    summary,
                    tags,
                    series,
                    aliases,
                    template,
                    draft,
                    publish_date,
                    expire_date
                )
            );

            CREATE TABLE IF NOT EXISTS routes (
                revision TEXT,
                id TEXT,
                path TEXT,
                parent_path TEXT,
                kind INTEGER,
                template TEXT
                UNIQUE(
                    revision,
                    id,
                    path,
                    parent_path,
                    kind,
                    template
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
