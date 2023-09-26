use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use console::{Emoji, style};
use indicatif::{ProgressBar, ProgressStyle};

static MAGNIFYING_GLASS: Emoji<'_, '_> = Emoji("🔍 ", "");
static CABINET: Emoji<'_, '_> = Emoji("🗃️ ", "");
static COMPASS: Emoji<'_, '_> = Emoji("🧭 ", "");
static PRINTER: Emoji<'_, '_> = Emoji("🖨️ ", "");

const TICK_DURATION: Duration = Duration::from_millis(100);

pub struct ProgressHandle {
    bar: ProgressBar,
    msg: String,
}

impl Drop for ProgressHandle {
    fn drop(&mut self) {
        self.bar.finish_and_clear();
        println!(
            "{} {}",
            self.msg,
            style("[OK]").green().bright().bold()
        )
    }
}

pub struct Cli {
    start: Mutex<Instant>,
    end: Mutex<Instant>,
}

impl Cli {
    pub fn start_render(&self) {
        
    }

    pub fn source_walk() -> ProgressHandle {
        let style = ProgressStyle::with_template("{msg} {spinner:.green} [{elapsed_precise}]").unwrap();

        let msg = format!(
            "{} {}Walking source directory...",
            console::style("[1/4]").bold().dim(),
            MAGNIFYING_GLASS
        );

        let bar = ProgressBar::new_spinner()
            .with_style(style)
            .with_message(msg.clone());

        bar.enable_steady_tick(TICK_DURATION);

        ProgressHandle {
            bar,
            msg,
        }
    }

    pub fn finish_render(&self) {

    }
}