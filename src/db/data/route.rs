use num_enum::TryFromPrimitive;
use serde_repr::{Deserialize_repr, Serialize_repr};

use super::dependencies::*;

#[derive(Serialize_repr, Deserialize_repr, TryFromPrimitive, Debug, Clone, Copy)]
#[repr(u8)]
pub enum RouteKind {
    Unknown = 0,
    StaticAsset = 1,
    Page = 3,
    Stylesheet = 4,
    Redirect = 5,
}

/// Represents a URL route to a file.
/// Maps directly to and from rows in the `routes` table.
#[derive(Serialize, Deserialize, Debug)]
pub struct Route {
    /// The ID of the file this route points to.
    pub id: Option<String>,
    /// The ID of the revision this route is associated with.
    pub revision: String,
    /// The URL this route qualifies.
    /// Example: `/img/banner.png`, which points to `src/assets/img/banner.png`.
    pub route: String,
    /// The "parent" path of the route.
    /// Corresponds to the first subdirectory in the URL.
    /// Example: the parent path of `/posts/hello_there` is `posts`.
    pub parent_route: Option<String>,
    /// What type of asset this route points to.
    pub kind: RouteKind,
}

// Database write methods
impl Route {
    /// Prepares an SQL statement to insert a new row into the `routes` table and returns a closure that wraps it.
    pub fn prepare_insert(conn: &Connection) -> Result<impl FnMut(&RouteIn) -> Result<()> + '_> {
        let mut stmt = conn.prepare(
            "
            INSERT OR IGNORE INTO routes
            VALUES(:id, :revision, :route, :parent_route, :kind)
        ",
        )?;

        let closure = move |input: &RouteIn| {
            let _ = stmt.execute(input.to_params()?.to_slice().as_slice())?;
            Ok(())
        };

        Ok(closure)
    }
}

/// Reference-and-[Copy]-only version of [Route], intended for wrapping non-owned data for database insertion.
#[derive(Serialize, Debug)]
pub struct RouteIn<'a> {
    pub id: Option<&'a str>,
    pub revision: &'a str,
    pub route: &'a str,
    pub parent_route: Option<&'a str>,
    pub kind: RouteKind,
}

impl<'a> RouteIn<'a> {
    /// Serializes a [`RouteIn`] instance to a [`ParameterSlice`] suitable for consumption by [`rusqlite`] queries.
    /// Returns a [`DbError::Serde`] if serialization fails.
    pub fn to_params(&self) -> Result<ParameterSlice> {
        let params = to_params_named(&self)?;
        Ok(params)
    }
}
