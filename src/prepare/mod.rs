mod frontmatter;
mod hook;
mod route;
mod walking;

pub use walking::walk_src;

use crate::prelude::*;

pub fn prepare(ctx: &Context, rev_id: Option<&RevisionID>) -> Result<RevisionID> {
    let rev_id = match rev_id {
        Some(id) => {
            Message::WalkSkipped.print();

            id.clone()
        },
        None => {
            let progress = Progressor::new(Message::Walk);

            let rev_id = walk_src(ctx)?;

            progress.finish();
            rev_id
        }
    };

    {
        let progress = Progressor::new(Message::Parsing);
    
        frontmatter::parse_frontmatters(ctx, &rev_id)?;
        hook::create_hooks(ctx, &rev_id)?;

        progress.finish();
    }

    {
        let progress = Progressor::new(Message::Routing);

        route::create_routes(ctx, &rev_id)?;

        progress.finish();
    }

    Ok(rev_id)
}