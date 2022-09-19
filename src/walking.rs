use crate::{error::*, dbdata::InputFile};
use rayon::prelude::*;
use walkdir::{DirEntry, WalkDir};

/// Walks the site's `/src` directory for all valid content files.
pub fn walk_src() -> Vec<InputFile>  {

    log::info!("Starting walk...");

    let mut items: Vec<InputFile> = 
    WalkDir::new("src")
        .into_iter()
        .par_bridge()
        .filter_map(drain_entries)
        .filter_map(extract_metadata)
        .filter_map(process_entry)
        .collect();

    log::info!("Walking done, found {} items.", items.len());

    // Stupid hack to ensure consistent ordering after parallel computation.
    // This means we can generate consistent revision IDs down the line.
    // (Sorting is done by comparing on the item's id value.)
    items.sort_unstable();
    items
}

/// Drains all non-`Ok(...)` values from the walk output.
fn drain_entries(entry: Result<DirEntry, walkdir::Error>) -> Option<DirEntry> {
    match entry {
        Ok(entry) => Some(entry),
        Err(error) => {
            ERROR_CHANNEL.sink_error(WalkError::WalkDir(error));
            None
        }
    }
}

/// Extracts the metadata from each item in the walk output, and filters out any directories.
/// The files that remain are returned.
fn extract_metadata(entry: DirEntry) -> Option<DirEntry> {
    match entry.metadata() {
        Ok(md) => match md.is_dir() {
            true => None,
            false => Some(entry),
        },
        Err(error) => {
            ERROR_CHANNEL.sink_error(WalkError::WalkDir(error));
            None
        }
    }
}

/// Reads and hashes the entries that remain after `drain_entries` and `filter_metadata`.
/// Results are exfiltrated through the given channels.
fn process_entry(entry: DirEntry) -> Option<InputFile> {
    log::trace!("Walk found item {:#?}", entry.path());

    let contents = std::fs::read(entry.path());
    match contents {
        Ok(mut contents) => {
            let hash = hash(&contents);
            let inline = entry_is_inline(&entry);
            let extension = entry_extension(&entry);
            let id = {
                let joined = format!("{}{}", &hash, &entry.path().to_string_lossy());
                self::hash(&joined.as_bytes())
            };
            let path = entry.into_path();
            
            // Optimization: drain data read from non-inline files.
            // This isn't necessary per se, but we don't want to potentially
            // shuffle an entire MP4 around in memory for no reason.
            if !inline { contents.drain(..); }

            let contents = String::from_utf8_lossy(&contents).to_string();

            let item = InputFile {
                id,
                path,
                hash,
                extension,
                contents,
                inline
            };

            Some(item)
        }
        Err(error) => {
            ERROR_CHANNEL.sink_error(WalkError::Io(error));
            None
        }
    }
}

/// Hash the provided bytestream using `seahash` and `format!` it as a hexadecimal string.
fn hash(bytes: &[u8]) -> String {
    format!("{:016x}", seahash::hash(&bytes))
}

/// Determines whether or not the given entry is considered "inline."
///
/// Inline entries will have their content inserted directly into the content database.
/// Non-inline entries will be copied to the on-disk cache and renamed to their hash.
fn entry_is_inline(entry: &DirEntry) -> bool {
    match entry.path().extension() {
        Some(ext) => match ext.to_string_lossy().as_ref() {
            "md" | "scss" | "html" | "json" | "liquid" => true,
            _ => false,
        },
        _ => false,
    }
}

fn entry_extension(entry: &DirEntry) -> String {
    match entry.path().extension() {
        Some(ext) => ext.to_str().unwrap_or("").to_string(),
        None => String::from("")
    }
}
