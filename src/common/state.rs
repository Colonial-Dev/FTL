use std::{
    sync::Arc,
    path::{PathBuf, Path},
    env, ops::Deref
};

use arc_swap::ArcSwap;
use clap::Parser;

use super::{
    Arguments,
    Config
};

use crate::{
    prelude::*,
    db::Database,
};

/// Type alias for an atomically-refcounted instance of [`InnerState`].
pub type State = Arc<InnerState>;

/// Constant for ID values that have not yet been set.
pub const UNSET_ID: &str = "ID_NOT_SET";

/// Inner representation of global program state.
/// 
/// This is *mostly* immutable - the primary exceptions are the pools in `db` and
/// the revision tracking fields.
#[derive(Debug)]
pub struct InnerState {
    pub config: Config,
    pub args: Arguments,
    pub db: Database,
    pub swap: Swap,
}

impl InnerState {
    pub fn init() -> Result<State> {
        let args = Arguments::parse();
        let dir = validate_env()?;

        let config = dir.join(CONFIG_FILENAME);
        let config = Config::from_path(&config)?;

        let db = dir.join(SITE_DB_PATH);
        let db = Database::open(db);

        let state = InnerState {
            config,
            args,
            db,
            swap: Swap::new()
        };

        Ok(Arc::new(state))
    }

    pub fn clone(self: &State) -> State {
        Arc::clone(self)
    }
}

impl Deref for InnerState {
    type Target = Swap;

    fn deref(&self) -> &Self::Target {
        &self.swap
    }
}

#[derive(Debug)]
pub struct Swap {
    pub rev: ArcSwap<String>,
}

impl Swap {
    pub fn new() -> Self {
        Self {
            rev: Self::new_unset_id().into(),
        }
    }

    pub fn get_rev(&self) -> Arc<String> {
        self.rev.load_full()
    }

    pub fn set_rev(&self, id: impl Into<String>) {
        self.rev.store(Arc::new(id.into()))
    }

    fn new_unset_id() -> Arc<String> {
        Arc::new(UNSET_ID.to_string())
    }
}

/// Performs environment validation and setup - essentially, ensuring that everything is where and how it should be
/// before the program executes any further. If nothing is amiss, it returns the path of the current directory.
/// 
/// In no particular order, this function:
/// - 
fn validate_env() -> Result<PathBuf> {
    let mut current_dir = env::current_dir()?;
    
    if env::var("FTL_TEST_MODE").is_ok() {
        current_dir.push("test_site/");
        env::set_current_dir(&current_dir)?;
    }

    match try_locate_config(&current_dir) {
        Some(mut path) => {
            path.pop();
            env::set_current_dir(&path)?;
            Ok(path)
        }
        None => bail!("Failed to locate FTL configuration.")
    }
}

fn try_locate_config(start: &Path) -> Option<PathBuf> {
    let mut path: PathBuf = start.into();
    let target = Path::new(CONFIG_FILENAME);

    loop {
        path.push(target);

        if path.is_file() {
            break Some(path);
        }

        if !(path.pop() && path.pop()) {
            break None;
        }
    }
}