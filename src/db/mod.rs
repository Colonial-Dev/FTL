pub type DbPool = r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>;

pub mod data;
mod ops;

pub use ops::*;
pub use rusqlite::Connection;