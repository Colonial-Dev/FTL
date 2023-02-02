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
mod stylesheet;

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
}

impl Renderer {
    pub fn new(state: &State) -> Result<Arc<Self>> {
        prepare::prepare(state)?;
        
        let env = template::setup_environment(state)?;
        let hili = Highlighter::new(state)?;

        let arc = Arc::new_cyclic(move |weak| Self {
            env,
            hili,
            weak: Weak::clone(weak),
            state: Arc::clone(state)
        });

        let test = indoc!{"
            {% set query = DB.query('SELECT * FROM input_files') %}

            {% for row in query %}
            - File with id {{ row.id }} is at path {{ row.path }}.
            {% endfor %}
        "};

        println!("{}", arc.env.render_str(test, ())?);
        stylesheet::compile(state).unwrap();

        Ok(arc)
    }
}