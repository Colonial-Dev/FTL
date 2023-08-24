use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::prelude::*;

/// Represents the contents of FTL's global configuration.
#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub root_url: String,
    pub build: Build,
    pub render: Render,
    pub serve: Serve,
    #[serde(default)]
    pub extra: HashMap<String, toml::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Build {
    pub compile_sass: bool,
    pub external_links_new_tab: bool,
    pub external_links_no_follow: bool,
    pub external_links_no_referrer: bool,
}

impl Default for Build {
    fn default() -> Self {
        Build {
            compile_sass: true,
            external_links_new_tab: false,
            external_links_no_follow: false,
            external_links_no_referrer: false,
        }
    }
}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct Render {
    pub anchor_template: Option<String>,
    pub code_template: Option<String>,
    pub smart_punctuation: bool,
    pub highlight_code: bool,
    pub render_emoji: bool,
    pub minify_html: bool,
    pub minify_css: bool,
}

impl Config {
    pub fn from_path(path: &Path) -> Result<Self> {
        let toml_raw = match path.exists() {
            true => {
                std::fs::read_to_string(path)
                    .wrap_err("Could not read in configuration file.")
                    .suggestion("The configuration file was found, but couldn't be read - try checking your file permissions.")?
            },
            false => bail!("Configuration file not found.")
        };

        Ok(toml::from_str(&toml_raw)?)
    }
}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct Serve {
    pub address: String,
    pub port: u16,
    pub error_template: Option<String>,
    pub cache_max_size: u64,
    pub cache_ttl: u64,
    pub cache_tti: u64,
}