use r2d2::{Pool};
use r2d2_sqlite::SqliteConnectionManager;

pub type DbPool = Pool<SqliteConnectionManager>;

pub const KNOWN_TABLES: &[&str] = &["input_files", "revision_files", "pages", "routes", "page_aliases", "page_tags"];

pub mod data;
mod ops;
mod error;

pub use ops::*;
pub use error::*;
pub use rusqlite::Connection;



