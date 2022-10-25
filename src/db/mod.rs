use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

pub type DbPool = Pool<SqliteConnectionManager>;

pub mod data;
mod ops;

pub use ops::*;
pub use rusqlite::Connection;
