mod frontmatter;
mod route;
mod walking;

use crate::prelude::*;

pub fn prepare(state: &State) -> Result<()> {
    let rev_id = walking::walk(state)?;
    frontmatter::parse_frontmatters(state, &rev_id)?;
    route::create_routes(state, &rev_id)?;

    state.set_working_rev(rev_id);
    Ok(())
}
