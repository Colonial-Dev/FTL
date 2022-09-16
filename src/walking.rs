use crate::{error::*, *};
use flume::{Sender};
use rayon::prelude::*;
use std::{fs::Metadata, path::PathBuf};
use walkdir::{DirEntry, WalkDir};

#[derive(Debug)]
pub struct WalkItem {
    pub path: PathBuf,
    pub hash: String,
    pub extension: String,
    pub hapa: String,
    pub size: u64,
    pub contents: Vec<u8>,
    pub inline: bool,
}

/// Walks the site's `/src` directory for all valid content files.
pub fn walk_src(build_sinks: &BuildSinks) -> Vec<WalkItem>  {
    let (itx, irx) = flume::unbounded();

    log::info!("Starting walk...");

    WalkDir::new("src")
        .into_iter()
        .par_bridge()
        .filter_map(|x| drain_entries(build_sinks, x))
        .filter_map(|x| extract_metadata(build_sinks, x))
        .map_with((build_sinks, &itx), process_entry)
        .for_each(drop); // Force rayon iterator evalutation for its parallel side effects

    let items: Vec<WalkItem> = irx.try_iter().collect();
    log::info!("Walking done, found {} items.", items.len());

    items
}

/// Drains all non-`Ok(...)` values from the walk output.
fn drain_entries(sinks: &BuildSinks, entry: Result<DirEntry, walkdir::Error>) -> Option<DirEntry> {
    match entry {
        Ok(entry) => Some(entry),
        Err(error) => {
            sinks.sink_error(WalkError::WalkDir(error));
            None
        }
    }
}

/// Extracts the metadata from each item in the walk output, and filters out any directories.
/// The files that remain are returned bundled with their extracted metadata.
fn extract_metadata(sinks: &BuildSinks, entry: DirEntry) -> Option<(DirEntry, Metadata)> {
    match entry.metadata() {
        Ok(md) => match md.is_dir() {
            true => None,
            false => Some((entry, md)),
        },
        Err(error) => {
            sinks.sink_error(WalkError::WalkDir(error));
            None
        }
    }
}

/// Reads and hashes the entries that remain after `drain_entries` and `filter_metadata`.
/// Results are exfiltrated through the given channels.
fn process_entry(
    sink_bundle: &mut (&BuildSinks, &Sender<WalkItem>),
    entry_bundle: (DirEntry, Metadata),
) {
    let (build_sinks, item_sink) = sink_bundle;
    let (entry, md) = entry_bundle;

    log::trace!("Walk found item {:#?}", entry.path());

    let contents = std::fs::read(entry.path());
    match contents {
        Ok(mut contents) => {
            let hash = hash(&contents);
            let inline = entry_is_inline(&entry);
            let extension = entry_extension(&entry);
            let hapa = format!("{},{}", &hash, &entry.path().to_string_lossy());
            
            // Optimization: drain data read from non-inline files.
            if !inline { contents.drain(..);}

            let item = WalkItem {
                path: entry.into_path(),
                hash,
                extension,
                hapa,
                size: md.len(),
                contents,
                inline,
            };

            // Expect justification: these sinks should not close until after `walk_src` returns *at minimum.*
            item_sink
                .send(item)
                .expect("Walking item sink has been closed!");
        }
        Err(error) => {
            build_sinks.sink_error(WalkError::Io(error));
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
