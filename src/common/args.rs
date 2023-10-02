use clap::{ArgAction, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Arguments {
    #[command(subcommand)]
    pub command: Command,
    /// If enabled, surpress all output.
    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,
    /// Enable debug logging. 
    /// 
    /// - Level 1 enables ERROR, WARN and INFO.
    /// - Level 2 enables DEBUG.
    /// - Level 3 and up enables TRACE.
    #[clap(short, long, action = ArgAction::Count)]
    pub verbose: u8,
}

impl Arguments {
    pub fn drafts_enabled(&self) -> bool {
        match self.command {
            Command::Build { drafts, ..} => drafts,
            _ => false
        }
    }

    pub fn pretty_output(&self) -> bool {
        !self.quiet && self.verbose == 0
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Interactively create a new FTL site.
    Init {
        /// The root URL for the new site (e.g. `https://example.com`)
        root_url: String,
    },
    /// Invoke the FTL build pipeline.
    Build {
        /// Stay resident and trigger a new build whenever changes to the site source are detected.
        #[arg(short, long)]
        watch: bool,
        /// Build and serve the site locally, in debug mode. Implicitly enables `--watch`.
        #[arg(short, long)]
        serve: bool,
        /// Rebuild the entire site from scratch. Implicitly invokes `ftl db clear`.
        #[arg(short, long)]
        full: bool,
        /// Build the site with drafts included.
        #[arg(short, long)]
        drafts: bool,
    },
    /// Start the FTL webserver in production mode. Configured in `ftl.toml`.
    Serve,
    /// Inspect and manipulate site revisions.
    #[command(subcommand)]
    Revision(RevisionSubcommand),
    /// Inspect and manipulate the site's database and cache.
    #[command(subcommand)]
    Db(DatabaseSubcommand)
}

#[derive(Debug, Subcommand)]
pub enum RevisionSubcommand {
    /// List all revisions.
    List,
    /// View details for the specified revision.
    Inspect {
        /// The ID or user-provided name of the revision to inspect.
        id: String
    },
    /// Assigns a custom name to the specified revision.
    Name {
        /// The ID hash of the revision to name.
        id: String,
        /// The name to assign to the revision.
        name: String
    },
    /// Pin the specified revision, exempting it from `ftl db compress`.
    Pin {
        /// The ID or user-provided name of the revision to pin.
        id: String
    },
    /// Unpin the specified revision, allowing it to be swept by `ftl db compress`.
    Unpin {
        /// The ID or user-provided name of the revision to unpin.
        id: String
    },
    // TODO dumping?
}

#[derive(Debug, Subcommand)]
pub enum DatabaseSubcommand {
    /// Displays database and cache usage statistics (primarily disk space consumed).
    Stat,
    /// Sweeps the database and asset cache, deleting all rows and files not relevant
    /// to the most recent revision.
    Compress,
    /// Wipes the database and asset cache clean.
    Clear
}
