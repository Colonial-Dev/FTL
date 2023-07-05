use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use crossbeam::channel::Receiver;
use itertools::Itertools;
use rayon::prelude::*;
use walkdir::{DirEntry, WalkDir};

use crate::db::{Connection, InputFile, Revision, RevisionFile, DEFAULT_QUERY, NO_PARAMS};
use crate::prelude::*;

/// Walks the site's `/src` directory for all valid content files.
pub fn walk(state: &State) -> Result<String> {
    info!("Starting source directory walk...");

    let (handle, tx) = state.db.get_rw()?.prepare_consumer(consumer_handler);

    WalkDir::new(SITE_SRC_PATH)
        .into_iter()
        .filter_ok(|entry| entry.file_type().is_file() || entry.file_type().is_symlink())
        .par_bridge()
        .map(|entry| entry.map_err(Report::from).map(process_entry)?)
        .try_for_each(|entry| -> Result<_> {
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
        path: entry.into_path(),
        extension,
        contents,
        inline,
    };

    Ok((file, int_id))
}

fn consumer_handler(conn: &Connection, rx: Receiver<(InputFile, u64)>) -> Result<String> {
    let txn = conn.open_transaction()?;

    let mut insert_file = conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?;
    let mut ids = Vec::new();
    let mut hash = 0_u64;

    for message in rx.into_iter() {
        let (file, id) = message;
        insert_file(&file)?;

        if !file.inline {
            let destination = PathBuf::from(format!(".ftl/cache/{}", &file.hash));

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
    info!("Computed revision ID {rev_id}.");

    conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?(&Revision {
        id: rev_id.to_owned(),
        name: None,
        time: None,
        pinned: false,
        stable: false,
    })?;

    let mut insert_file = conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?;

    for id in ids {
        insert_file(&RevisionFile {
            id,
            revision: rev_id.clone(),
        })?;
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
            "md" | "in" | "html" | "scss" | "json" | "sublime-syntax" | "tmTheme"
        ),
        _ => false,
    }
}
