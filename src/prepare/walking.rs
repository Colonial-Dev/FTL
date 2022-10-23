use std::{
    hash::{Hash, Hasher},
    path::Path,
};

use rayon::prelude::*;
use walkdir::{DirEntry, WalkDir};

use crate::{
    db::{
        data::{InputFile, RevisionFile, RevisionFileIn},
        Connection,
    },
    prelude::*,
};

pub const SITE_SRC_DIRECTORY: &str = "src/";
pub const SITE_ASSET_DIRECTORY: &str = "assets/";
pub const SITE_CONTENT_DIRECTORY: &str = "content/";
pub const SITE_TEMPLATE_DIRECTORY: &str = "templates/";

/// Walks the site's `/src` directory for all valid content files.
pub fn walk_src(conn: &mut Connection) -> Result<String> {
    info!("Starting walk...");

    let files: Result<Vec<InputFile>> = WalkDir::new(SITE_SRC_DIRECTORY)
        .into_iter()
        .par_bridge()
        .map(process_entry)
        .filter_map(|x| x.transpose() )
        .collect();

    let mut files = files?;
    
    info!("Walking done, found {} items.", files.len());

    // Stupid hack to ensure consistent ordering after parallel co`mputation.
    // This means we can generate consistent revision IDs down the line.
    // (Sorting is done by comparing on the item's id value.)
    files.sort_unstable_by(|a, b| a.id.cmp(&b.id) );

    // We use a transaction to accelerate database write performance.
    let txn = conn.transaction()?;

    update_input_files(&*txn, &files).context("Failed to update input_files table.")?;
    let rev_id = compute_revision_id(&files);
    update_revision_files(&*txn, &files, &rev_id)
        .context("Failed to update revision_files table.")?;

    txn.commit()?;
    Ok(rev_id)
}

/// Reads and hashes the entries that remain after `drain_entries` and `filter_metadata`.
fn process_entry(entry: Result<DirEntry, walkdir::Error>) -> Result<Option<InputFile>> {
    let entry = entry?;
    let metadata = entry.metadata()?;

    if metadata.is_dir() {
        return Ok(None)
    }

    let mut contents = std::fs::read(entry.path())?;

    debug!("Walk found item {:#?}", entry.path());

    let hash = hash(&contents);
    let inline = entry_is_inline(&entry);
    let extension = entry_extension(&entry);
    let id = {
        let joined = format!("{}{}", &hash, &entry.path().to_string_lossy());
        self::hash(joined.as_bytes())
    };
    let path = entry.into_path();

    // Optimization: drain data read from non-inline files.
    // This isn't necessary per se, but we don't want to potentially
    // shuffle an entire MP4 around in memory for no reason.
    if !inline {
        contents.drain(..);
    }

    let str_repr = String::from_utf8_lossy(&contents).to_string();

    let contents: Option<String> = match str_repr.len() {
        0 => None,
        _ => Some(str_repr),
    };

    let item = InputFile {
        id,
        hash,
        path,
        extension,
        contents,
        inline,
    };

    Ok(Some(item))
}

/// Hash the provided bytestream using `seahash` and `format!` it as a hexadecimal string.
fn hash(bytes: &[u8]) -> String {
    format!("{:016x}", seahash::hash(bytes))
}

/// Determines whether or not the given entry is considered "inline."
///
/// Inline entries will have their content inserted directly into the content database.
/// Non-inline entries will be copied to the on-disk cache and renamed to their hash.
fn entry_is_inline(entry: &DirEntry) -> bool {
    match entry.path().extension() {
        Some(ext) => match ext.to_string_lossy().as_ref() {
            "md" | "scss" | "html" | "json" | "tera" => true,
            _ => false,
        },
        _ => false,
    }
}

/// Gets the extension of the entry, if any.
fn entry_extension(entry: &DirEntry) -> Option<String> {
    match entry.path().extension() {
        Some(ext) => {
            let ext = ext.to_str();
            ext.map(|ext| ext.to_string())
        }
        None => None,
    }
}

fn update_input_files(conn: &Connection, files: &[InputFile]) -> Result<()> {
    let mut insert_file = InputFile::prepare_insert(conn)?;

    for file in files {
        insert_file(file)?;

        if !file.inline {
            debug!("Caching non-inline file {:#?}", &file.path);
            let destination = format!(".ftl/cache/{}", &file.hash);
            let destination = Path::new(&destination);
            if !&destination.exists() {
                std::fs::copy(&file.path, &destination)?;
            }
        }
    }

    info!("Updated input_files table.");
    Ok(())
}

fn update_revision_files(conn: &Connection, files: &[InputFile], rev_id: &str) -> Result<()> {
    let mut insert_file = RevisionFile::prepare_insert(conn)?;

    for file in files {
        insert_file(&RevisionFileIn {
            revision: rev_id,
            id: &file.id,
        })?;
    }

    info!("Updated revision_files table.");
    Ok(())
}

fn compute_revision_id(files: &[InputFile]) -> String {
    let mut ids: Vec<&str> = Vec::new();

    for file in files {
        ids.push(&file.id);
    }

    let mut hasher = seahash::SeaHasher::default();
    ids.hash(&mut hasher);
    let rev_id = format!("{:016x}", hasher.finish());

    info!("Computed revision ID {}", rev_id);

    rev_id
}
