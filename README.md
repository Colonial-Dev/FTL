<p align="center">
<img src=".github/logo.png" width="512">
</p>
<h3 align="center">A sorta-static site generator and server.</h3>


<p align="center">
<img src="https://img.shields.io/github/actions/workflow/status/SomewhereOutInSpace/FTL/rust.yml">
<img src="https://img.shields.io/github/license/SomewhereOutInSpace/FTL">
</p>

FTL is a static site generator (and server) with a twist: instead of being a largely stateless Markdown-in, HTML-out pipeline, it leans on an embedded [SQLite database](https://www.sqlite.org/index.html) to track your site's past and efficiently reason about its future, enabling incremental and atomic builds.

## Features

TODO

## Acknowledgements
Credit where it's due: the inspiration for (and basic design of) FTL is shamelessly cribbed from Amos/`fasterthanlime`'s closed-source `futile`, via his blog post on its design [here](https://fasterthanli.me/articles/a-new-website-for-2020). The implementation is all mine, but the ideas were invaluable for getting this project to escape velocity. Check his stuff out!

(The name, on the other hand, is only related by coincidence. I had settled on the name FTL back before I even knew Rust, when all I wanted to make was a Hugo-esque SSG in C#.)