mod database;
mod highlighter;
mod ticket;
mod resource;

use minijinja::State as MJState;

use super::error::{
    MJResult,
    MJError,
    MJErrorKind,
    WrappedReport as Wrap
};

pub use database::*;
pub use highlighter::*;
pub use ticket::*;
pub use resource::*;

/// Attempts to fetch the "page" variable from the engine state and downcast it into
/// a [`Ticket`].
/// 
/// - If successful, it then executes the provided closure against the downcasted [`Ticket`]
/// and returns its output.
/// - If unsuccessful, it immediately returns [`None`].
fn try_with_page<F, R>(state: &MJState, op: F) -> Option<R> where
    F: FnOnce(&Ticket) -> R
{
    use std::sync::Arc;

    if let Some(value) = state.lookup("page") {
        if let Some(ticket) = value.downcast_object_ref::<Arc<Ticket>>() {
            return op(ticket).into()
        }
    }

    None
}