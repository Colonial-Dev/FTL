use std::fmt::Display;
use std::time::Duration;
use std::ops::Range;

use std::sync::atomic::{
    AtomicBool,
    Ordering
};

use console::style;
use indicatif::{ProgressBar, ProgressStyle};

// static MAGNIFYING_GLASS: Emoji<'_, '_> = Emoji("🔍 ", "");
// static CABINET: Emoji<'_, '_> = Emoji("🗃️ ", "");
// static COMPASS: Emoji<'_, '_> = Emoji("🧭 ", "");
// static PRINTER: Emoji<'_, '_> = Emoji("🖨️ ", "");

#[derive(Debug, Clone, Copy)]
pub enum Message {
    Walk,
    WalkSkipped,
    Parsing,
    Routing,
    Rendering,
    BuildOK,
    BuildFail
}

impl Message {
    pub fn print(self) {
        if ENABLE_DUMMY.load(Ordering::Relaxed) {
            return;
        }

        eprintln!("{self}")
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Message::*;

        let mut write_step = |msg: &str, step: Range<usize>| {
            let step = format!(
                "[{}/{}]",
                step.start,
                step.end
            );
    
            write!(
                f,
                "{} {:40}",
                style(step).bold().dim(),
                msg
            )
        };

        match self {
            Walk => write_step(
                "🔍 Walking source directory...",
                1..4
            ),
            WalkSkipped => write!(
                f,
                "{} {:40} {}",
                console::style("[1/4]").bold().dim(),
                "🔍 Walking source directory...",
                console::style("[SKIPPED]").yellow().bold().bright()
            ),
            Parsing => write_step(
                "📑 Parsing frontmattters and hooks...",
                2..4
            ),
            Routing => write_step(
                "🧭 Computing routes...",
                3..4
            ),
            Rendering => write_step(
                "📥 Rendering...",
                4..4
            ),
            BuildOK => writeln!(
                f,
                "\n Build {}",
                console::style("complete.").bold().bright().green()
            ),
            BuildFail => todo!()
        }
    }
}

static ENABLE_DUMMY: AtomicBool = AtomicBool::new(false);

const TICK_DURATION: Duration = Duration::from_millis(100);

#[must_use]
pub enum Progressor {
    Real {
        bar: ProgressBar,
        msg: String,
        ok: bool,
    },
    Dummy
}

impl Progressor {
    pub fn set_dummy(value: bool) {
        ENABLE_DUMMY.store(
            value,
            Ordering::SeqCst
        )
    }

    pub fn new(msg: Message) -> Self {
        if ENABLE_DUMMY.load(Ordering::Relaxed) {
            return Self::Dummy;
        }
        
        let bar = Self::new_spinner();
        let msg = format!("{msg}");

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

        eprintln!(
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

        eprintln!(
            "{} {}",
            msg,
            style("[FAIL]").red().bright().bold()
        )
    }
}