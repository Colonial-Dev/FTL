mod resource;

use std::net::SocketAddr;
use std::time::Duration;
use std::sync::Arc;

use arc_swap::ArcSwap as Swap;
use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use moka::future::Cache;
use resource::*;

use crate::prelude::*;
use crate::render::{prepare_with_id, Renderer};
use crate::watch::init_watcher;

type Server = Arc<InnerServer>;

pub struct InnerServer {
    pub renderer: Swap<Renderer>,
    pub rev_id: Swap<String>,
    pub cache: Cache<Uri, Resource>,
    pub ctx: Context,
}

impl InnerServer {
    pub fn new(ctx: &Context, renderer: Renderer) -> Server {
        let renderer = Arc::new(renderer);
        let rev_id = renderer.rev_id.clone();

        let cache = Cache::builder()
            .max_capacity(ctx.serve.cache_max_size * 1024 * 1024)
            .time_to_idle(Duration::from_secs(ctx.serve.cache_tti))
            .time_to_live(Duration::from_secs(ctx.serve.cache_ttl))
            .weigher(|_, value: &Resource| {
                value.size() as u32
            })
            .eviction_listener_with_queued_delivery_mode(|uri, _, reason| {
                debug!("Entry for URI {uri:?} evicted from cache (reason: {reason:?}).")
            })
            .build();
            
        Arc::new(Self {
            renderer: Swap::new(renderer),
            rev_id: Swap::new(rev_id.into_inner()),
            cache,
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
        let server = self.clone();

        tokio::task::spawn(async move { loop {
            let (_debouncer, mut rx) = init_watcher(&server.ctx)
                .expect("Failed to create watcher");

            match rx.recv().await {
                Ok(id) => {
                    server.migrate_revision(id)
                        .unwrap_or_else(|err| error!("Failed to migrate revision - {err:?}"))
                },
                Err(_) => {
                    error!("Watch receiver closed - this shouldn't happen!");
                    break;
                }
            }
        }});

        let app = Router::new()
            .route("/", get(fetch_resource))
            .route("/*path", get(fetch_resource))
            .with_state(self.clone());

        info!("Starting webserver.");

        let ip = self.ctx.serve.address.parse()?;
        let port = self.ctx.serve.port;

        info!("Binding to address {ip}:{port}...");

        let addr = SocketAddr::new(
            ip,
            port
        );

        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await?;

        info!("Webserver terminated successfully.");

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

    fn migrate_revision(&self, rev_id: RevisionID) -> Result<()> {
        info!("Migrating to revision {rev_id}...");

        prepare_with_id(&self.ctx, &rev_id)?;

        let renderer = Renderer::new(&self.ctx, &rev_id)?;
        renderer.render_revision()?;

        self.renderer.swap(renderer.into());
        self.rev_id.swap(rev_id.into_inner());
        self.cache.invalidate_all();

        info!("Successfully migrated to revision {rev_id}.");
        Ok(())
    }
}

async fn fetch_resource(State(server): State<Server>, uri: Uri) -> Response {
    debug!("GET request for URI {uri:?}");

    if let Some(cached) = server.cache.get(&uri) {
        debug!("Serving URI {uri:?} from cache.");
        return cached.into_response();
    }

    let resource = Resource::from_uri(
        &server,
        uri.clone()
    ).await;

    if resource.should_cache() {
        debug!("Caching {uri:?}");
        server.cache.insert(
            uri, 
            resource.clone()
        ).await;
    }

    resource.into_response()
}