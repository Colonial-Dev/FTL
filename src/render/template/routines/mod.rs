mod querying;

use minijinja::{
    context,
    State as MJState
};

use super::{*, error::MJResult};

pub fn register(state: &State, env: &mut Environment<'_>) -> Result<()> {
    let query_fn = querying::prepare_query(state);
    let query_filter = querying::prepare_query(state);

    env.add_function("eval", eval);
    env.add_function("query", query_fn);

    env.add_filter("eval", eval);
    env.add_filter("query", query_filter);
    env.add_filter("slug", slug::slugify::<String>);

    Ok(())
}

fn eval(state: &MJState, template: String) -> MJResult {
    state.env().render_named_str(
        "eval.html",
        &template,
        context!(page => state.lookup("page"))
    ).map(Value::from_safe_string)
}

fn eval_shortcode(state: &MJState, name: &str, args: Value) -> Result<Value> {
    let name = format!("{name}.html");

    let Ok(template) = state.env().get_template(&name) else {
        /*let err = eyre!(
            "Page \"{}\" contains a shortcode invoking template \"{}\", which does not exist.",
            page.title,
            code.name
        )
        .note("This error occurred because a shortcode referenced a template that FTL couldn't find at build time.")
        .suggestion("Double check the shortcode invocation for spelling and path mistakes, and make sure the template is where you think it is.");

        bail!(err)*/

        bail!("Shortcode {name} does not exist.")
    };

    Ok(
        template
            .render(context!(args => args))
            .map(Value::from_safe_string)?
    )
}