// We need:
// A high-level Renderer struct that wraps all the assorted crap needed to render a revision
// A module for objects used in the templating engine
//   - "Resource" enum?
// A module for functions/filters/etc used in the templating engine
// A module for setting up the templating engine environment (possibly super of the previous two?)
// A module for post-render rewriting
// A module for stylesheet compilation

mod highlight;
mod prepare;
mod template;

use std::sync::{Arc, Weak};

use minijinja::Environment;

use crate::prelude::*;

use highlight::Highlighter;

#[derive(Debug)]
pub struct Renderer {
    pub env: Environment<'static>,
    pub hili: Highlighter,
    pub weak: Weak<Self>,
    pub state: State,
    // highlighter
    // rewriter??
}

pub fn prepare(state: &State) -> Result<()> {
    prepare::prepare(state)
}