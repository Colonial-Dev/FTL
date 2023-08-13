mod frontmatter;
mod hook;
mod route;
mod walking;

pub use frontmatter::parse_frontmatters;
pub use route::create_routes;
pub use walking::walk;

use crate::prelude::*;

pub fn prepare(ctx: &Context) -> Result<RevisionID> {
    let rev_id = walking::walk(ctx)?;

    frontmatter::parse_frontmatters(ctx, &rev_id)?;
    hook::create_hooks(ctx, &rev_id)?;
    route::create_routes(ctx, &rev_id)?;

    Ok(rev_id)
}
