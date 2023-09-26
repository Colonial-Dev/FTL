<p align="center">
<img src=".github/logo.png" width="512">
</p>
<h3 align="center">A sorta-static site generator and server.</h3>


<p align="center">
<img src="https://img.shields.io/github/actions/workflow/status/SomewhereOutInSpace/FTL/rust.yml">
<img src="https://img.shields.io/github/license/SomewhereOutInSpace/FTL">
<img src="https://img.shields.io/github/stars/Colonial-Dev/FTL">
</p>

FTL is a static site generator (and server) with a twist: instead of being a largely stateless Markdown-in, HTML-out pipeline, it leans on an embedded [SQLite database](https://www.sqlite.org/index.html) to track your site's past and efficiently reason about its future, enabling incremental and atomic builds.

## Features

- Fast and atomic builds.
  - Builds are incremental and dependency-aware. FTL tracks all assets used by a page, and will only rebuild pages when necessary.
  - Thanks to SQLite's ACID guarantees, FTL will never leave your site in a half-built state, even if it crashes or is interrupted.
  - Every build is tracked as an independent "revision," allowing you to easily roll back if need be.
- SASS compilation using [`grass`](https://crates.io/crates/grass).
- Syntax highlighting for code blocks, using [`inkjet`](https://crates.io/crates/inkjet).
- Automatic cache-busting for static assets.
- Flexible frontmatter format. You decide what attributes are available, and what they mean.
- A powerful templating system based on the [MiniJinja](https://github.com/mitsuhiko/minijinja) engine.
  - Use shortcodes with parameters directly in your Markdown source.
  - Includes a number of useful built-in filters and functions, ranging from the mundane (time formatting, Base64 manipulation) to Very Cursed and Problematic™ (executing arbitrary shell code.)
  - Query the site database from within your templates - the sky's the limit when it comes to custom behavior!
- A built-in webserver for both development and production, using [`axum`](https://crates.io/crates/axum).
  - Supports automatic live-reloading. Simply alter the source files and watch your changes go live!
  - Define arbitrary "hook" templates to enable dynamic behavior, such as site search.
  - Configurable caching system - set TTI/TTL and maximum size to best fit your needs.

## Installation
Some notes:
- FTL is developed on and primarily targets Linux. As such, I can't promise that it will work on other platforms.
- I've strived to avoid making FTL a "bespoke" website solution. Anyone should be able to use it, and I do encourage you give it a try if you find it attractive. 
  - *However,* please keep in mind that this is a hobby project, not a professional-grade tool like Hugo or Zola. I can't promise the same level of features or support as those projects.
  - (That said, I'm still happy to entertain issues or PRs.)

Dependencies:
- The most recent stable [Rust toolchain](https://rustup.rs/).
- A C/C++ toolchain (such as `gcc` - used when compiling SQLite, among other things.)

Installation is easy - just use `cargo install`, and FTL will be automatically compiled and added to your `PATH`.
```sh
cargo install --locked --git https://github.com/Colonial-Dev/FTL --branch master
```

The same command can be used to update FTL in the future.

## Getting Started/Documentation

Creating a new site is easy - just run `ftl init` in an empty directory. You'll be prompted for some basic information, and then a site skeleton will be created as a starting point.

To learn how to actually work with your site, please see the [wiki](https://github.com/Colonial-Dev/FTL/wiki).

## Acknowledgements
Credit where it's due: the inspiration for (and basic design of) FTL is shamelessly cribbed from Amos/`fasterthanlime`'s closed-source `futile`, via his blog post on its design [here](https://fasterthanli.me/articles/a-new-website-for-2020). The implementation is all mine, but the ideas were invaluable for getting this project to escape velocity. Check his stuff out!

(The name, on the other hand, is only related by coincidence. I had settled on the name FTL back before I even knew Rust, when all I wanted to make was a Hugo-esque SSG in C#.)