use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>
}

#[derive(Subcommand)]
enum Commands {
    Init(Init),
    Build(Build),
    Serve(Serve),
    Db(Db)
}

#[derive(Args)]
struct Init {

}

#[derive(Args)]
struct Build {

}

#[derive(Args)]
struct Serve {

}

#[derive(Args)]
struct Db {

}