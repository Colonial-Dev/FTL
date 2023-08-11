mod error;
mod resource;

use std::sync::Arc;

use arc_swap::ArcSwapAny as Swap;
use axum::extract::State;
use axum::http::Uri;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use error::BoxError;
use resource::*;

use crate::db::*;
use crate::prelude::*;

type Server = Arc<InnerServer>;

pub struct InnerServer {
    pub rev_id: Swap<RevisionID>,
    pub ctx: Context,
}

impl InnerServer {
    pub fn new(ctx: &Context, rev_id: &RevisionID) -> Server {
        Arc::new(Self {
            rev_id: Swap::new(rev_id.clone()),
            ctx: ctx.clone(),
        })
    }
}

/// Bootstraps the Tokio runtime and starts the internal `async` site serving code.
pub fn serve(ctx: &Context, rev_id: &RevisionID) -> Result<()> {
    info!("Starting Tokio runtime.");

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(_serve(ctx, rev_id))
}

async fn _serve(ctx: &Context, rev_id: &RevisionID) -> Result<()> {
    let app = Router::new()
        .route("/", get(fetch_wrapper))
        .route("/*path", get(fetch_wrapper))
        .with_state(InnerServer::new(ctx, rev_id));

    info!("Starting webserver.");

    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

async fn fetch_wrapper(State(server): State<Server>, uri: Uri) -> Result<Response, BoxError> {
    Ok(fetch_resource(State(server), uri).await?)
}

async fn fetch_resource(State(server): State<Server>, uri: Uri) -> Result<Response> {
    info!("GET request for path {uri:?}");

    tokio::task::spawn_blocking(move || {
        let route = lookup_route(&server, uri.path())?;

        let resource = Resource::from_route(&server.ctx, route)?;

        Ok(resource)
    })
    .await?
    .map(IntoResponse::into_response)
}

fn lookup_route(server: &Server, path: &str) -> Result<Route> {
    let conn = server.ctx.db.get_ro()?;
    let rev_id = server.rev_id.load();

    let query = "
        SELECT * FROM routes
        WHERE route = ?1
        AND revision = ?2
    ";

    let parameters = [(1, path), (2, rev_id.as_ref())];

    let mut get_route = conn.prepare_reader(query, parameters.as_slice().into())?;

    match get_route.next() {
        Some(route) => route,
        None => bail!("404 not found"),
    }
}
