use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use crossbeam::channel::Receiver;
use itertools::Itertools;
use rayon::prelude::*;
use walkdir::{DirEntry, WalkDir};

use crate::db::{Connection, InputFile, Revision, RevisionFile, Model};
use crate::prelude::*;

/// Walks the site directory for all valid content files.
pub fn walk_src(ctx: &Context) -> Result<RevisionID> {
    info!("Starting source directory walk...");

    let _spinner = Cli::source_walk();
    let (handle, tx) = ctx.db.get_rw()?.prepare_consumer(consumer_handler);

    WalkDir::new(".")
        .into_iter()
        .filter_ok(|entry| {
            (entry.file_type().is_file() || entry.file_type().is_symlink())
                && !entry
                    .path()
                    .to_str()
                    .map(|s| s.starts_with("./.ftl") || s.starts_with("./ftl.toml"))
                    .unwrap_or(false)
        })
        .par_bridge()
        .try_for_each(|entry| -> Result<_> {
            let entry = entry.map_err(Report::from).map(process_entry)?;
            let _ = tx.send(entry?);
            Ok(())
        })?;
    
    // Deadlocking is generally regarded as undesirable.
    drop(tx);

    handle
        .join()
        .expect("Database consumer thread should not panic.")
}

fn process_entry(entry: DirEntry) -> Result<(InputFile, u64)> {
    let Some(path) = entry.path().to_str() else {
        let err = eyre!("Encountered a non-UTF-8 path ({:?}).", entry.path())
            .suggestion("FTL only supports UTF-8 paths; make sure your directories and filenames are valid UTF-8.");
        bail!(err)
    };

    debug!("Walk found item at {path}");

    // Note: we filter out directories before calling this function.
    let mut contents = std::fs::read(path)?;

    let extension = match entry.path().extension() {
        Some(ext) => ext.to_str().map(str::to_owned),
        None => None,
    };
    let inline = is_inline(&extension);

    let hash = format!("{:016x}", seahash::hash(&contents));
    let (hex_id, int_id) = {
        let mut hasher = seahash::SeaHasher::new();
        hash.hash(&mut hasher);
        path.hash(&mut hasher);

        let int_hash = hasher.finish();
        let hex_hash = format!("{int_hash:016x}");

        (hex_hash, int_hash)
    };

    // Drain data read from non-inline files.
    //
    // This isn't super necessary, but a few experiments
    // showed it can help keep memory usage down, esp. if the
    // consumer thread can't keep up.
    if !inline {
        contents.clear();
        contents.shrink_to_fit();
    }

    let contents = String::from_utf8(contents)
        .with_context(|| {
            format!("Encountered an inline file (path: {path}) that is not valid UTF-8.")
        })
        .suggestion("FTL only supports UTF-8 text; check to make sure your file isn't corrupt.")
        .map(|str| match str.len() {
            0 => None,
            _ => Some(str),
        })?;

    let file = InputFile {
        id: hex_id,
        hash,
        path: path.trim_start_matches("./").into(),
        extension,
        contents,
        inline,
    };

    Ok((file, int_id))
}

fn consumer_handler(conn: &mut Connection, rx: Receiver<(InputFile, u64)>) -> Result<RevisionID> {
    let txn = conn.transaction()?;

    let mut ids = Vec::new();
    let mut hash = 0_u64;

    for message in rx.into_iter() {
        let (file, id) = message;
        file.insert_or_ignore(&txn)?;

        if !file.inline {
            let destination = PathBuf::from(format!("{SITE_CACHE_PATH}{}", &file.id));

            if !destination.exists() {
                debug!("Caching non-inline file {:#?}", &file.path);
                std::fs::copy(&file.path, destination)?;
            }
        }

        ids.push(file.id);
        // XORing hashes to combine them is cryptographically questionable, but:
        // - We're already using a non-cryptographic hash function.
        // - We know there are no duplicates (so the hash won't accidentally be XORed to zero.)
        // - This is infinitely faster than the original approach of sorting a Vec of all id's
        // and hashing that.
        hash ^= id;
    }

    let rev_id = format!("{hash:016x}");
    let rev_id = RevisionID::from(rev_id);

    info!("Computed revision ID {rev_id}.");

    Revision {
        id: rev_id.to_string(),
        name: None,
        time: None,
        pinned: false,
        stable: false,
    }.insert_or_ignore(&txn)?;

    for id in ids {
        RevisionFile {
            id,
            revision: rev_id.to_string(),
        }.insert_or_ignore(&txn)?;
    }

    txn.commit()?;
    info!("Done walking source directory.");

    Ok(rev_id)
}

/// Determines whether or not a file with the given extension is considered "inline."
///
/// Inline files will have their content inserted directly into the content database as UTF-8 text.
/// Non-inline files will be copied to the on-disk cache and renamed to their hash.
fn is_inline(ext: &Option<String>) -> bool {
    match ext {
        Some(ext) => matches!(
            ext.as_str(),
            "md" | "in" | "html" | "scss" | "json" | "toml"
        ),
        _ => false,
    }
}
