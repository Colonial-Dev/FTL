mod frontmatter;
mod hook;
mod route;
mod walking;

pub use frontmatter::parse_frontmatters;
pub use route::create_routes;
pub use walking::walk_src;

use crate::prelude::*;

pub fn prepare(ctx: &Context, rev_id: Option<&RevisionID>) -> Result<RevisionID> {
    let rev_id = match rev_id {
        Some(id) => id.clone(),
        None => walk_src(ctx)?
    };

    frontmatter::parse_frontmatters(ctx, &rev_id)?;
    hook::create_hooks(ctx, &rev_id)?;
    route::create_routes(ctx, &rev_id)?;

    Ok(rev_id)
}