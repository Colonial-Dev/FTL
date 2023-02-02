use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::io;

use ahash::AHashMap;
use itertools::Itertools;
use grass::{Fs, Options};

use crate::db::{
    Queryable, StatementExt, DEFAULT_QUERY, NO_PARAMS,
    Route, RouteKind, Output, OutputKind
};
use crate::prelude::*;

/// Filesystem override for [`grass`] that preloads all known stylesheets and their paths into a hashmap.
#[derive(Debug)]
struct MapFs {
    map: AHashMap<PathBuf, Vec<u8>>
}

#[derive(Debug)]
struct Row {
    path: PathBuf,
    contents: String
}

impl Queryable for Row {
    fn read_query(stmt: &sqlite::Statement<'_>) -> Result<Self> {
        Ok(Self {
            path: stmt.read_string("path").map(PathBuf::from)?,
            contents: stmt.read_string("contents")?
        })
    }
}

impl MapFs {
    pub fn load(state: &State) -> Result<Self> {
        let conn = state.db.get_ro()?;
        let rev_id = state.get_working_rev();

        let query = "
            SELECT path, contents FROM input_files
            JOIN revision_files ON revision_files.id = input_files.id
            WHERE revision_files.revision = ?1
            AND path LIKE 'src/assets/sass/%'
            AND extension IN ('sass', 'scss');
        ";
        let params = (1, rev_id.as_str()).into();

        let map: AHashMap<_, _> = conn.prepare_reader(query, params)?
            .map(|row| -> Result<_> {
                let row: Row = row?;
                
                // Shave off the 'src/assets/sass/' component of the path.
                let path = row.path.iter().skip(3).collect();
                let bytes = row.contents.into_bytes();

                Ok((path, bytes))
            })
            .try_collect()?;
        
        Ok(Self { map })
    }
}

impl Fs for MapFs {
    fn is_dir(&self, path: &Path) -> bool {
        false
    }

    fn is_file(&self, path: &Path) -> bool {
        self.map.contains_key(path)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        use io::{Error, ErrorKind::NotFound};

        match self.map.get(path) {
            Some(vector) => Ok(vector.to_owned()),
            None => Err(Error::new(
                NotFound,
                format!("file not found ({path:?})")
            ))
        }
    }
}

pub fn compile(state: &State) -> Result<String> {
    let fs = MapFs::load(state)?;
    let options = Options::default().fs(&fs);
    let path = Path::new("style.scss");

    if !fs.is_file(path) {
        let err = eyre!("Tried to compile SASS, but 'style.scss' could not be found.");
        let err = err.note("SASS compilation expects the root file to be at 'src/assets/sass/style.scss'.");
        bail!(err)
    }

    let output = grass::from_path(path, &options)?;

    let conn = state.db.get_rw()?;
    let rev_id = state.get_working_rev();

    let mut hasher = seahash::SeaHasher::new();
    for value in fs.map.values() {
        value.hash(&mut hasher);
    }

    let hash = format!("{:016x}", hasher.finish());
    let route = format!("static/style.{hash}.css");

    conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?(&Output{
        id: hash.clone().into(),
        kind: OutputKind::Stylesheet,
        content: output
    })?;

    conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?(&Route {
        id: hash.into(),
        revision: (*rev_id).to_owned(),
        route: route.to_owned(),
        kind: RouteKind::Stylesheet
    })?;

    //TODO: Stylesheet dependency tracking

    Ok(route)
}