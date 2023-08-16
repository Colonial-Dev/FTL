mod error;
mod resource;

use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::extract::State;
use axum::http::Uri;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use error::BoxError;
use resource::*;

use crate::db::*;
use crate::prelude::*;
use crate::render::Renderer;

type Server = Arc<InnerServer>;

pub struct InnerServer {
    pub renderer: ArcSwap<Renderer>,
    pub rev_id: ArcSwap<String>,
    pub ctx: Context,
}

impl InnerServer {
    pub fn new(ctx: &Context, renderer: Renderer) -> Server {
        let renderer = Arc::new(renderer);
        let rev_id = renderer.rev_id.clone();
        
        Arc::new(Self {
            renderer: ArcSwap::new(renderer),
            rev_id: ArcSwap::new(rev_id.into_inner()),
            ctx: ctx.clone(),
        })
    }

    /// Bootstraps the Tokio runtime and starts the internal `async` site serving code.
    pub fn serve(self: &Server) -> Result<()> {
        info!("Starting Tokio runtime.");

        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to start Tokio runtime.")
            .block_on(self._serve())
    }

    async fn _serve(self: &Server) -> Result<()> {
        let app = Router::new()
            .route("/", get(fetch_wrapper))
            .route("/*path", get(fetch_wrapper))
            .with_state(self.clone());

        info!("Starting webserver.");

        axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
            .serve(app.into_make_service())
            .await?;

        Ok(())
    }
}

async fn fetch_wrapper(State(server): State<Server>, uri: Uri) -> Result<Response, BoxError> {
    Ok(fetch_resource(State(server), uri).await?)
}

async fn fetch_resource(State(server): State<Server>, uri: Uri) -> Result<Response> {
    info!("GET request for path {uri:?}");

    tokio::task::spawn_blocking(move || {
        let route = lookup_route(&server, uri.path())?;

        let resource = Resource::from_route(&server, route)?;

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
