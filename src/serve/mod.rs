use crate::prelude::*;

/// Bootstraps the Tokio runtime and starts the internal `async` site serving code.
pub fn serve() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(_serve())
}

async fn _serve() -> Result<()> {
    Ok(())
}