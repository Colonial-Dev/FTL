use r2d2::{Pool};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, params, Transaction};
use std::path::Path;
use crate::parse::FmItem;
use crate::walking::WalkItem;
use crate::error::*;

pub type DbPool = Pool<SqliteConnectionManager>;

pub fn update_input_files(pool: &DbPool, items: &[WalkItem]) -> Result<(), DbError> {
    let mut conn = pool.get()?;
    let txn = conn.transaction()?;

    into_input_files(&txn, &items[..])?;

    txn.commit()?;
    Ok(())
}

pub fn update_revision_files(pool: &DbPool, items: &[WalkItem]) -> Result<String, DbError> {
    let mut conn = pool.get()?;
    let txn = conn.transaction()?;
    let rev_id = compute_revision_id(&items);

    into_revision_files(&txn, &items, &rev_id)?;

    txn.commit()?;
    Ok(rev_id)
}

fn into_input_files(txn: &Transaction, items: &[WalkItem]) -> Result<(), DbError> {
    log::info!("Updating input_files table...");

    let conn = &*txn;
    
    for item in items {
        conn.execute("
            INSERT OR IGNORE INTO input_files
            VALUES(?1, ?2, ?3);
        ", params![&item.hapa, &item.extension, &String::from_utf8_lossy(&item.contents)])?;

        if !item.inline {
            log::trace!("Caching non-inline file {:#?}", &item.path);
            let destination = format!(".ftl/cache/{}", &item.hash);
            std::fs::copy(&item.path, Path::new(&destination))?;
        }
    }

    log::info!("Done updating input_files table.");
    Ok(())
}

fn compute_revision_id(items: &[WalkItem]) -> String {
    let mut hapas: Vec<u8> = Vec::new();

    for item in items {
        hapas = [&hapas, item.hapa.as_bytes()].concat();
    }

    // Supremely braindead hack to ensure reproducible revision IDs,
    // even when hapas are shuffled due to parallel execution quirks.
    hapas.sort();

    let rev_id = format!("{:016x}", seahash::hash(&hapas));
    log::info!("Computed revision ID {}", rev_id);

    rev_id
}

fn into_revision_files(txn: &Transaction, items: &[WalkItem], rev_id: &str) -> Result<(), DbError> {
    log::info!("Updating revision_files table...");
    
    let conn = &*txn;

    for item in items {
        conn.execute("
            INSERT INTO revision_files
            VALUES(?1, ?2);
        ", params![&rev_id, &item.hapa])?;
    }

    log::info!("Done updating revision_files table.");
    Ok(())
}

pub fn update_pages(pool: &DbPool, items: &[FmItem]) -> Result<(), DbError> {
    let mut conn = pool.get()?;
    let txn = conn.transaction()?;
    
    into_pages(&txn, &items)?;

    txn.commit()?;
    Ok(())
}

fn into_pages(txn: &Transaction, items: &[FmItem]) -> Result<(), DbError> {
    log::info!("Updating pages table...");

    let conn = &*txn;

    // TODO clean this cringe up
    for item in items {
        conn.execute("
            INSERT OR IGNORE INTO pages
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13);
        ", params! [
            &item.hapa,
            &item.offset,
            &item.fm.title,
            &item.fm.date,
            &item.fm.description,
            &item.fm.summary,
            &serde_json::to_string(&item.fm.tags).unwrap_or_else(|_| "".to_string()),
            &serde_json::to_string(&item.fm.series).unwrap_or_else(|_| "".to_string()),
            &serde_json::to_string(&item.fm.aliases).unwrap_or_else(|_| "".to_string()),
            &item.fm.build_cfg.template,
            &item.fm.build_cfg.draft,
            &item.fm.build_cfg.publish_date,
            &item.fm.build_cfg.expire_date,
        ])?;
    }

    log::info!("Done updating pages table.");
    Ok(())
}

pub fn make_db_pool(path: &Path) -> Result<DbPool, DbError> {
    let on_init = |db: &mut Connection| {
        db.pragma_update(None, "journal_mode", &"WAL".to_string())?;
        let mut tables = db.prepare(
            "
            CREATE TABLE IF NOT EXISTS input_files (
                hapa TEXT PRIMARY KEY,
                extension TEXT,
                contents TEXT,
                UNIQUE(hapa)
            );

            CREATE TABLE IF NOT EXISTS revision_files (
                revision TEXT,
                hapa TEXT,
            );

            CREATE TABLE IF NOT EXISTS pages (
                hapa TEXT PRIMARY KEY,
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
                UNIQUE(hapa)
            );

            CREATE TABLE IF NOT EXISTS page_aliases (
                hapa TEXT PRIMARY KEY,
                route TEXT,
                UNIQUE(hapa)
            );

            CREATE TABLE IF NOT EXISTS tags (
                tag TEXT,
                UNIQUE(tag)
            );

            CREATE TABLE IF NOT EXISTS routes (
                route TEXT PRIMARY KEY,
                hapa TEXT,
                UNIQUE(route)
            );
            "
        )?;
        tables.execute([])?;
        Ok(())
    };

    let manager = SqliteConnectionManager::file(path).with_init(on_init);
    let pool = Pool::new(manager)?;
    Ok(pool)
}
