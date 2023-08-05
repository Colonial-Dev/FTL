use axum::{
    body::Bytes,
    response::{Response, IntoResponse, Html},
    Router, routing::get,
    extract::State, 
    http::{Uri, StatusCode}
};

use tokio::task::spawn_blocking;

use crate::{
    prelude::*,
    db::*
};

#[derive(Clone)]
pub struct Server {
    pub rev_id: RevisionID,
    pub ctx: Context,
}

#[derive(Debug, Clone)]
pub enum Resource {
    Hypertext(Html<String>),
    Plaintext(String),
    Octets(Bytes),
    Code(StatusCode),
}

impl Resource {
    pub fn from_route(ctx: &Context, route: Route) -> Result<Self> {
        match route.kind {
            RouteKind::Asset | RouteKind::RedirectAsset => {
                let path = format!(
                    "{SITE_CACHE_PATH}{}", 
                    route.id.expect("Asset routes should have an ID")
                );

                let bytes = std::fs::read(path)?;
                let bytes = Bytes::from(bytes);

                Ok(Self::Octets(bytes))
            }
            RouteKind::Page | RouteKind::RedirectPage | RouteKind::Stylesheet => {
                let conn = ctx.db.get_ro()?;
                let id = route.id
                    .as_ref()
                    .expect("Page and stylesheet routes should have an ID")
                    .as_str();

                let query = "
                    SELECT * FROM output
                    WHERE id = ?1
                ";
        
                let parameters = (
                    1, 
                    id
                ).into();

                let mut get_output = conn.prepare_reader(
                    query, 
                    parameters
                )?;
            
                match get_output.next() {
                    Some(output) => {
                        let output: Output = output?;
                        let content = output.content;

                        if matches!(route.kind, RouteKind::Page | RouteKind::RedirectPage) {
                            Ok(
                                Self::Hypertext(Html::from(content))
                            )
                        } else {
                            Ok(
                                Self::Plaintext(content)
                            )
                        }
                    },
                    None => Ok(Self::Code(StatusCode::NOT_FOUND))
                }
            }
            _ => unimplemented!()
        }
    }
}

impl IntoResponse for Resource {
    fn into_response(self) -> Response {
        use Resource::*;

        match self {
            Hypertext(html) => html.into_response(),
            Plaintext(str) => str.into_response(),
            Octets(bytes) => bytes.into_response(),
            Code(status) => status.into_response(),
        }
    }
}

/// Bootstraps the Tokio runtime and starts the internal `async` site serving code.
pub fn serve(ctx: &Context, rev_id: &RevisionID) -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(_serve(ctx, rev_id))
}

async fn _serve(ctx: &Context, rev_id: &RevisionID) -> Result<()> {
    let app = Router::new()
        .route("/", get(fetch))
        .route("/*path", get(fetch))
        .with_state(Server {
            rev_id: rev_id.clone(),
            ctx: ctx.clone()
        });

    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

async fn fetch(State(server): State<Server>, uri: Uri) -> Result<Response, AppError> {
    Ok(fetch_resource(State(server), uri).await?)
}

async fn fetch_resource(State(server): State<Server>, uri: Uri) -> Result<Response> {
    info!("GET request for path {uri:?}");

    let path = uri.to_string();

    spawn_blocking(move || -> Result<_> {
        let route = lookup_route(&server, path)?;
        let resource = Resource::from_route(
            &server.ctx,
            route
        )?;

        Ok(resource)
    })
    .await?
    .map(IntoResponse::into_response)
}

fn lookup_route(server: &Server, path: String) -> Result<Route> {
    let path = path.trim_start_matches('/');
    let conn = server.ctx.db.get_ro()?;
    let rev_id = server.rev_id.as_ref();

    let query = "
        SELECT * FROM routes
        WHERE route = ?1
        AND revision = ?2
    ";

    let parameters = [
        (1, path),
        (2, rev_id)
    ];

    let mut get_route = conn.prepare_reader(
        query, 
        parameters.as_slice().into()
    )?;

    match get_route.next() {
        Some(route) => route,
        None => bail!("404 not found")
    }
}


// Make our own error that wraps `anyhow::Error`.
struct AppError(color_eyre::Report);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, AppError>`. That way you don't need to do that manually.
impl<E> From<E> for AppError
where
    E: Into<color_eyre::Report>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}