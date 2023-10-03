use std::time::Duration;
use std::ops::Range;

use console::style;
use indicatif::{ProgressBar, ProgressStyle};

// static MAGNIFYING_GLASS: Emoji<'_, '_> = Emoji("🔍 ", "");
// static CABINET: Emoji<'_, '_> = Emoji("🗃️ ", "");
// static COMPASS: Emoji<'_, '_> = Emoji("🧭 ", "");
// static PRINTER: Emoji<'_, '_> = Emoji("🖨️ ", "");

const TICK_DURATION: Duration = Duration::from_millis(100);

pub enum Progressor {
    Real {
        bar: ProgressBar,
        msg: String,
        ok: bool,
    },
    Dummy
}

impl Progressor {
    pub fn new(msg: impl AsRef<str>, step: Range<usize>, dummy: bool) -> Self {
        if dummy {
            return Self::Dummy;
        }
        
        let bar = Self::new_spinner();

        let step = format!(
            "[{}/{}]",
            step.start,
            step.end
        );

        let msg = format!(
            "{} {:40}",
            style(step).bold().dim(),
            msg.as_ref()
        );

        bar.set_message(msg.clone());
        bar.enable_steady_tick(TICK_DURATION);

        Self::Real {
            bar,
            msg,
            ok: false
        }
    }

    pub fn finish(mut self) {
        let Self::Real { bar, msg, ok } = &mut self else {
            return;
        };

        bar.finish_and_clear();

        println!(
            "{} {}",
            msg,
            style("[OK]").green().bright().bold(),
        );

        *ok = true;
    }

    fn new_spinner() -> ProgressBar {
        ProgressBar::new_spinner().with_style(
            ProgressStyle::with_template("{msg} {spinner:.green} [{elapsed_precise}]").unwrap()
        )
    }
}

impl Drop for Progressor {
    fn drop(&mut self) {
        let Self::Real { bar, msg, ok } = self else {
            return;
        };
        
        if *ok {
            return;
        }

        bar.finish_and_clear();

        println!(
            "{} {}",
            msg,
            style("[FAIL]").red().bright().bold()
        )
    }
}