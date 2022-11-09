use std::path::Path;

use once_cell::sync::Lazy;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection};
use rusqlite_migration::{Migrations, M};

use super::DbPool;
use crate::prelude::*;

const DB_PATH: &str = ".ftl/content.db";

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
    set_pragmas(&mut conn)?;
    PRIME_MIGRATIONS.to_latest(&mut conn)?; 

    Ok(conn)
}

/// Attempt to open a connection to the SQLite database.
pub fn make_connection() -> Result<Connection> {
    let mut conn = Connection::open(DB_PATH)?;
    set_pragmas(&mut conn)?;
    Ok(conn)
}

/// Attempt to create a connection pool for an SQLite database at the given path.
pub fn make_pool() -> Result<DbPool> {
    let manager = SqliteConnectionManager::file(DB_PATH)
        .with_init(set_pragmas);

    let pool = r2d2::Pool::builder()
        .max_size(*THREADS as u32)
        .build(manager)?;

    Ok(pool)
}

fn set_pragmas(conn: &mut Connection) -> Result<(), rusqlite::Error> {
    // WAL grants faster performance and allows reads that are concurrent to writes.
    // NORMAL synchronization is safe with WAL enabled, and gives an extra speed boost
    // by minimizing filesystem IO.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

pub fn attach_mapping_database(conn: &Connection) -> Result<()> {
    conn.execute_batch(include_str!("sql/aux_up.sql"))
        .context("Could not attach mapping database!")
}

pub fn detach_mapping_database(conn: &Connection) -> Result<()> {
    conn.execute("DETACH DATABASE map;", [])?;
    Ok(())
}

pub fn dump_input_for_revision(conn: &Connection, rev_id: &str) -> Result<()> {
    use serde_rusqlite::from_rows;
    use crate::db::data::InputFile;

    let mut stmt = conn.prepare("
        SELECT input_files.* FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
    ")?;
    
    let target_dir = Path::new("target/").join(format!("input-{}", &rev_id));
    let cache_dir = Path::new(".ftl/cache/");

    for file in from_rows::<InputFile>(stmt.query(params![&rev_id])?) {
        let file = file?;

        let target = target_dir.join(file.path);
        std::fs::create_dir_all(target.parent().unwrap())?;

        if !file.inline {
            let source = cache_dir.join(file.hash);
            std::fs::copy(source, target)?;
        }
        else {
            std::fs::write(&target, &file.contents.unwrap_or_default())?;
        }
    }
    
    Ok(())
}

pub fn dump_output_for_revision(conn: &Connection, rev_id: &str) -> Result<()> {
    // For dumping *output*, we get all the *routes* for the revision.
    // From there, we match on its kind value to determine where to fetch the data from
    // (either the `output` table or the flat-file cache), then write the file to the dump
    // target at its route.
    use std::path::PathBuf;
    use serde_rusqlite::from_rows;
    use crate::db::data::{Route, RouteKind};

    // Stage one: get all "safe" routes (static assets and pages).
    // Safe routes map 1:1 to an entry in the cache or database.
    let mut get_safe_routes = conn.prepare("
        SELECT routes.* FROM routes
        JOIN revision_files ON revision_files.id = routes.id
        WHERE revision_files.revision = ?1
        AND kind NOT IN (0, 4, 5)
    ")?;

    let mut get_output = conn.prepare("
        SELECT content FROM output
        WHERE id = ?1 AND revision = ?2
    ")?;

    let mut get_hash = conn.prepare("
        SELECT hash FROM input_files
        WHERE id = ?1
    ")?;

    let target_dir = Path::new("target/").join(format!("output-{}", &rev_id));
    let cache_dir = Path::new(".ftl/cache/");

    let mut resolve_disk = |id: &str, route: &str| -> Result<(PathBuf, PathBuf)> {
        let hash = from_rows::<String>(get_hash.query([id])?)
            .next()
            .transpose()?
            .with_context(|| format!("Could not resolve hash for on-disk file {id}."))?;

        let source = cache_dir.join(hash);
        let target = target_dir.join(route);
        Ok((source, target))
    };

    let mut resolve_inline = |id: &str, route: &str| -> Result<(PathBuf, String)> {
        let content = from_rows::<String>(get_output.query(params![id, rev_id])?)
            .next()
            .transpose()?
            .with_context(|| format!("Could not resolve output for input file {id}."))?;

        let target = target_dir.join(route);
        Ok((target, content))
    };

    for route in from_rows::<Route>(get_safe_routes.query(params![rev_id])?) {
        let route = route?;

        match route.kind {
            RouteKind::StaticAsset => {
                let (source, target) = resolve_disk(&route.id.unwrap(), &route.route)?;
                debug!("{source:?} // {target:?}");

                std::fs::create_dir_all(target.parent().unwrap())?;
                std::fs::copy(source, target)?;
            },
            RouteKind::Page => {
                let (mut target, content) = resolve_inline(&route.id.unwrap(), &route.route)?;
                debug!("{target:?}");

                if target.is_dir() {
                    target.push("index.html");
                } else {
                    target.set_extension("html");
                }

                std::fs::create_dir_all(target.parent().unwrap())?;
                std::fs::write(target, content)?;
            },
            _ => unreachable!()
        };
    }

    let mut get_redirects = conn.prepare("
        SELECT routes.* FROM routes
        JOIN revision_files ON revision_files.id = routes.id
        WHERE revision_files.revision = ?1
        AND kind = 5
    ")?;

    let mut resolve_redirect = conn.prepare("
        SELECT * FROM routes
        WHERE routes.id = ?1
        AND revision = ?2
        AND kind IN (1, 3)
    ")?;

    for route in from_rows::<Route>(get_redirects.query(params![rev_id])?) {
        let route = route?;
        let link = target_dir.join(&route.route);

        let route = from_rows::<Route>(resolve_redirect.query(params![route.id, rev_id])?)
            .next()
            .transpose()?
            .with_context(|| format!("Could not resolve redirect to file {:?}.", route.id))?;

        match route.kind {
            RouteKind::StaticAsset => {
                let (_, original) = resolve_disk(&route.id.unwrap(), &route.route)?;
                soft_link(original, link)?;
            },
            RouteKind::Page => {
                let (original, _) = resolve_inline(&route.id.unwrap(), &route.route)?;
                soft_link(original, link)?;
            }
            _ => unreachable!()
        };
    }

    todo!()
}

fn soft_link(original: impl AsRef<Path>, link: impl AsRef<Path>) -> Result<()> {
    #[cfg(target_family="unix")]
    {
        std::os::unix::fs::symlink(original, link)?;
    }
    #[cfg(target_family="windows")]
    {
        // CERTIFIED WINDOWS MOMENT
        std::os::windows::fs::symlink_file(original, link)?;
    }
    Ok(())
}


/// Tries to wipe the database and flat-file cache clean.
pub fn try_clear(conn: &mut Connection) -> Result<()> {
    std::fs::remove_dir_all(".ftl/cache")?;
    std::fs::create_dir_all(".ftl/cache")?;
    
    PRIME_MIGRATIONS.to_version(conn, 0)?;
    PRIME_MIGRATIONS.to_latest(conn)?;
    Ok(())
}

/// Tries to clean the database and flat-file cache of any data that is not relevant to any
/// pinned or active revision.
pub fn try_compress(conn: &Connection) -> Result<()> {
    use serde_rusqlite::from_rows;

    conn.execute("
        DELETE FROM revisions
        WHERE pinned != 1
        AND timestamp NOT IN (SELECT MAX(timestamp) FROM revisions)
    ", [])?;

    let cache_dir = Path::new(".ftl/cache/");
    let mut stmt = conn.prepare("
        SELECT hash FROM input_files
        WHERE NOT EXISTS (
            SELECT 1 FROM revision_files
            WHERE revision_files.id = input_files.id
        )
    ")?;

    for hash in from_rows::<String>(stmt.query([])?) {
        let hash = hash?;

        let target = cache_dir.join(hash);
        
        if target.exists() {
            std::fs::remove_file(target)?;
        } else {
            warn!("Tried to remove cached file \"{target:?}\", but it does not exist!")
        }
    }

    conn.execute("
        DELETE FROM input_files
        WHERE NOT EXISTS (
            SELECT 1 FROM revision_files
            WHERE revision_files.id = input_files.id
    )
    ", [])?;

    conn.execute("VACUUM", [])?;
    
    Ok(())
}

// TODO: helper functions to create faux in-memory databases (including dummy data) for unit testing.

#[cfg(test)]
mod migrations {
    use super::*;

    #[test]
    fn prime() {
        let mut conn = Connection::open_in_memory().unwrap();
        set_pragmas(&mut conn).unwrap();

        assert_eq!(PRIME_MIGRATIONS.to_latest(&mut conn), Ok(()));
        assert_eq!(PRIME_MIGRATIONS.to_version(&mut conn, 0), Ok(()));
    }

    #[test]
    fn aux() {
        let mut conn = Connection::open_in_memory().unwrap();
        set_pragmas(&mut conn).unwrap();

        assert!(attach_mapping_database(&conn).is_ok());
        assert!(detach_mapping_database(&conn).is_ok());
    }
}