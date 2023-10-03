mod frontmatter;
mod hook;
mod route;
mod walking;

pub use walking::walk_src;

use crate::prelude::*;

pub fn prepare(ctx: &Context, rev_id: Option<&RevisionID>) -> Result<RevisionID> {
    let rev_id = match rev_id {
        Some(id) => {
            println!(
                "{} {:30} {}",
                console::style("[1/4]").bold().dim(),
                "🔍 Walking source directory...",
                console::style("[SKIPPED]").yellow().bold().bright()
            );

            id.clone()
        },
        None => {
            let progress = ctx.progressor(
                "🔍 Walking source directory...",
                1..4
            );

            let rev_id = walk_src(ctx)?;

            progress.finish();
            rev_id
        }
    };

    {
        let progress = ctx.progressor(
            "📑 Parsing frontmattters and hooks...",
            2..4
        );
    
        frontmatter::parse_frontmatters(ctx, &rev_id)?;
        hook::create_hooks(ctx, &rev_id)?;

        progress.finish();
    }

    {
        let progress = ctx.progressor(
            "🧭 Computing routes...",
            3..4
        );

        route::create_routes(ctx, &rev_id)?;

        progress.finish();
    }


    Ok(rev_id)
}