# FTL - A sorta-static site generator
**🚧🚧 Warning - Things are still very much Under Construction™! 🚧🚧**

FTL is a static site generator (and server) with a twist: instead of being a largely stateless Markdown-in, HTML-out pipeline, it leans on an *embedded [SQLite](https://www.sqlite.org/index.html) database* to track your site's past and efficiently reason about the future, enabling incremental and atomic builds.

## Roadmap
While FTL is still very much under development, lots is already implemented!

- The [core rendering subsystem](https://github.com/SomewhereOutInSpace/FTL/tree/master/src/render) is nearly complete. This includes:
    - All "prepatory operations", from file walking through frontmatter parsing and route construction.
    - A bespoke page parser/preprocessor for features like shortcodes and codeblock syntax highlighting.
    - A templating system that revolves around mitsuhiko's *excellent* [MiniJinja](https://github.com/mitsuhiko/minijinja) engine.
        - Custom functions enable operations like querying the content database from within templates.
        - FTL also resolves the dependency graph of a site's templates using a bespoke parser and some SQL magic, tagging pages so they can be rebuilt if their template changes. This is done recursively, so even changes to transitive dependencies will be handled.
    - A baked-in SASS compiler and HTML post-processor, based on the `grass` and `lol_html` crates respectively.
    - Since the database tracks *every* file ever input into FTL (identified by its hash), everything past the file walking step gets change detection for free and avoids
    doing any unnecessary processing.
- A [bespoke wrapper on top of the `sqlite` crate](https://github.com/SomewhereOutInSpace/FTL/tree/master/src/db), providing:
    - An efficient (if slightly boilerplate-y) to serialize and deserialize types from database tables using ergonomic closures and iterators.
    - A concurrent connection pool implementation. (Doing this allowed me to add useful functionality to the pooled connection.)

What still needs to be done:

- A smattering of various features in the rendering subsystem, such as a "resource" API. (Additionally, some of the aformentioned features have old, poor implementations with replacements incubating in local branches.)
- The server subsystem, which encompasses stuff like caching and the ability to add dynamic rendering "hooks" on top of simple `path -> hypertext`. (*Technically*, there's already an implementation of this in the source tree, but it was created purely for testing purposes and is currently excluded from compilation altogether.)
- A proper command-line interface - one existed earlier in development (`clap` makes it pretty easy), but it quickly lagged behind the actual desired feature set and I eventually stripped it out.