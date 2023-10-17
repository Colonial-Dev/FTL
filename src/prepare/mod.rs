mod frontmatter;
mod hook;
mod route;
mod walking;

pub use walking::walk_src;

use crate::prelude::*;

pub fn prepare(ctx: &Context, rev_id: Option<&RevisionID>) -> Result<RevisionID> {
    let rev_id = match rev_id {
        Some(id) => {
            println!("{}", Message::WalkSkipped);

            id.clone()
        },
        None => {
            let progress = ctx.progressor(Message::Walk);

            let rev_id = walk_src(ctx)?;

            progress.finish();
            rev_id
        }
    };

    {
        let progress = ctx.progressor(Message::Parsing);
    
        frontmatter::parse_frontmatters(ctx, &rev_id)?;
        hook::create_hooks(ctx, &rev_id)?;

        progress.finish();
    }

    {
        let progress = ctx.progressor(Message::Routing);

        route::create_routes(ctx, &rev_id)?;

        progress.finish();
    }

    Ok(rev_id)
}