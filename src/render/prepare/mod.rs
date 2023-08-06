mod frontmatter;
mod route;
mod walking;

pub use frontmatter::parse_frontmatters;
pub use route::create_routes;
pub use walking::walk;

use crate::prelude::*;

pub fn prepare(ctx: &Context) -> Result<RevisionID> {
    let rev_id = walking::walk(ctx)?;

    frontmatter::parse_frontmatters(ctx, &rev_id)?;
    route::create_routes(ctx, &rev_id)?;

    Ok(rev_id)
}
