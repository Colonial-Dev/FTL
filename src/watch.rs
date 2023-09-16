use std::time::Duration;
use std::path::Path;

use notify_debouncer_full::{
    notify::{Watcher, EventKind, RecursiveMode},
    new_debouncer,
    DebounceEventResult, FileIdCache, Debouncer,
};

use tokio::sync::broadcast::*;

use crate::prelude::*;
use crate::render::prepare;

pub fn init_watcher(ctx: &Context) -> Result<(Debouncer<impl Watcher, impl FileIdCache>, Receiver<RevisionID>)> {
    let ctx = ctx.clone();
    let conn = ctx.db.get_ro()?;

    let (tx, rx) = channel(16);

    let mut debouncer = new_debouncer(
        Duration::from_secs(1),
        None,
        move |result: DebounceEventResult| {
            match result {
                Ok(events) => {
                    let changed = events
                        .iter()
                        .any(|event| {
                            use EventKind::*;
                            matches!(event.kind, Any | Create(_) | Modify(_) | Remove(_))
                        });

                    debug!("Watcher received events - {events:?}");
                    debug!("Any files changed? {changed}");

                    if !changed { return; }

                    match prepare::walk(&ctx) {
                        Ok(id) => {
                            let _ = tx.send(id);
                        }
                        Err(e) => {
                            error!("Failed to walk site root from watcher: {e:?}");
                        }
                    }
                },
                Err(errors) => {
                    for error in errors {
                        error!("Debouncer error: {error:?}")
                    }
                }
            }
        }
    )?;

    debouncer.watcher().watch(
        Path::new(SITE_ASSET_PATH),
        RecursiveMode::Recursive
    )?;

    debouncer.watcher().watch(
        Path::new(SITE_HOOKS_PATH),
        RecursiveMode::Recursive
    )?;

    debouncer.watcher().watch(
        Path::new(SITE_CONTENT_PATH),
        RecursiveMode::Recursive
    )?;

    debouncer.watcher().watch(
        Path::new(SITE_TEMPLATE_PATH),
        RecursiveMode::Recursive
    )?;

    debouncer.cache().add_root(
        Path::new("."),
        RecursiveMode::Recursive
    );

    Ok((debouncer, rx))
}