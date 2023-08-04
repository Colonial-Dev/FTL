use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

use ahash::AHashMap;
use grass::{Fs, Options};
use itertools::Itertools;

use crate::db::{
    Output, OutputKind, Queryable, Route, RouteKind, StatementExt, DEFAULT_QUERY, NO_PARAMS,
};
use crate::prelude::*;

/// Filesystem override for [`grass`] that preloads all known stylesheets and their paths into a hashmap.
#[derive(Debug)]
struct MapFs {
    map: AHashMap<PathBuf, Vec<u8>>,
}

#[derive(Debug)]
struct Row {
    path: PathBuf,
    contents: String,
}

impl Queryable for Row {
    fn read_query(stmt: &sqlite::Statement<'_>) -> Result<Self> {
        Ok(Self {
            path: stmt.read_string("path").map(PathBuf::from)?,
            contents: stmt.read_string("contents")?,
        })
    }
}

impl MapFs {
    pub fn load(ctx: &Context, rev_id: &RevisionID) -> Result<Self> {
        let conn = ctx.db.get_ro()?;

        let query = "
            SELECT path, contents FROM input_files
            JOIN revision_files ON revision_files.id = input_files.id
            WHERE revision_files.revision = ?1
            AND path LIKE 'assets/sass/%'
            AND extension IN ('sass', 'scss');
        ";
        let params = (1, rev_id.as_ref()).into();

        let map: AHashMap<_, _> = conn
            .prepare_reader(query, params)?
            .map_ok(|row: Row| {
                // Shave off the 'assets/sass/' component of the path.
                let path: PathBuf = row.path.iter().skip(2).collect();
                let bytes = row.contents.into_bytes();
                (path, bytes)
            })
            .try_collect()?;

        Ok(Self { map })
    }
}

impl Fs for MapFs {
    fn is_dir(&self, _: &Path) -> bool {
        false
    }

    fn is_file(&self, path: &Path) -> bool {
        self.map.contains_key(path)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        use io::Error;
        use io::ErrorKind::NotFound;

        match self.map.get(path) {
            Some(vector) => Ok(vector.to_owned()),
            None => Err(Error::new(NotFound, format!("file not found ({path:?})"))),
        }
    }
}

pub fn compile(ctx: &Context, rev_id: &RevisionID) -> Result<()> {
    info!("Starting SASS compilation...");

    let conn = ctx.db.get_rw()?;

    let hash = load_hash(ctx, rev_id)?;
    let route = format!("static/style.css?v={hash}");

    conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?(&Route {
        id: hash.clone().into(),
        revision: rev_id.to_string(),
        route,
        kind: RouteKind::Stylesheet,
    })?;

    let query = "
        SELECT NULL FROM output
        WHERE id = ?1
    ";
    let params = (1, hash.as_str()).into();

    if conn.exists(query, params)? {
        info!("Stylesheet output already exists, skipping rebuild.");
        return Ok(());
    }

    let fs = MapFs::load(ctx, rev_id)?;
    let options = Options::default().fs(&fs);
    let path = Path::new("style.scss");

    if !fs.is_file(path) {
        let err = eyre!("Tried to compile SASS, but 'style.scss' could not be found.");
        let err = err.note(
            "SASS compilation expects the root file to be at \"assets/sass/style.scss\".",
        );
        bail!(err)
    }

    let output = grass::from_path(path, &options)?;

    conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?(&Output {
        id: hash.into(),
        kind: OutputKind::Stylesheet,
        content: output,
    })?;

    Ok(())
}

pub fn load_hash(ctx: &Context, rev_id: &RevisionID) -> Result<String> {
    let conn = ctx.db.get_ro()?;

    let query = "
        SELECT input_files.id FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND path LIKE 'assets/sass/%'
        AND extension IN ('sass', 'scss');
        ORDER BY input_files.id
    ";
    let params = (1, rev_id.as_ref()).into();

    let hash = conn
        .prepare_reader(query, params)?
        .fold_ok(seahash::SeaHasher::new(), |mut hasher, id: String| {
            id.hash(&mut hasher);
            hasher
        })?
        .finish();

    info!("Stylesheet compilation complete.");
    Ok(format!("{hash:016x}"))
}
