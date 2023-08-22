mod error;
mod resource;

use std::net::SocketAddr;
use std::sync::Arc;

use arc_swap::ArcSwap as Swap;
use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use resource::*;

use crate::prelude::*;
use crate::render::Renderer;

type Server = Arc<InnerServer>;

pub struct InnerServer {
    pub renderer: Swap<Renderer>,
    pub rev_id: Swap<String>,
    pub ctx: Context,
}

impl InnerServer {
    pub fn new(ctx: &Context, renderer: Renderer) -> Server {
        let renderer = Arc::new(renderer);
        let rev_id = renderer.rev_id.clone();
        
        Arc::new(Self {
            renderer: Swap::new(renderer),
            rev_id: Swap::new(rev_id.into_inner()),
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
            .route("/", get(fetch_resource))
            .route("/*path", get(fetch_resource))
            .with_state(self.clone());

        info!("Starting webserver.");

        let ip = self.ctx.serve.address.parse()?;
        let port = self.ctx.serve.port;

        let addr = SocketAddr::new(
            ip,
            port
        );

        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await?;

        Ok(())
    }

    fn render_error_page(self: &Server, code: StatusCode, uri: &Uri, report: Option<Report>) -> Result<String> {
        if let Some(template) = &self.ctx.serve.error_template {
            let renderer = self.renderer.load();
            let template = renderer
                .env
                .get_template(template)
                .expect("Error template should be loaded.");

            let backtrace = report.map(|report| {
                ansi_to_html::convert_escaped(
                    &format!("{report:?}")
                ).unwrap()
            });
            
            let error_page = template.render(minijinja::context!{
                code => code.as_u16(),
                reason => code.canonical_reason(),
                path => uri.path(),
                uri => uri.to_string(),
                backtrace
            })?;

            Ok(error_page)
        } else {
            Ok(format!(
                "{} {}",
                code.as_u16(),
                code.canonical_reason().unwrap_or("Unknown")
            ))
        }
    }
}

async fn fetch_resource(State(server): State<Server>, uri: Uri) -> Response {
    info!("GET request for path {uri:?}");

    Resource::from_uri(&server, uri).await.into_response()
}