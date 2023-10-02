use std::string::FromUtf8Error;
use std::sync::Arc;

use axum::body::Bytes;
use axum::http::{StatusCode, Uri, HeaderName, HeaderValue, HeaderMap};
use axum::response::{IntoResponse, Response};
use itertools::Itertools;
use minijinja::{context, Value};

use crate::db::*;
use crate::prelude::*;

use super::Server;

#[derive(Debug, Clone)]
pub enum Resource {
    Text(String, RouteKind),
    Octets(Bytes),
    Hook {
        code: StatusCode,
        headers: Arc<[(String, String)]>,
        output: String,
        cache: bool,
    },
    Error(String, StatusCode)
}

impl Resource {
    pub async fn from_uri(server: &Server, uri: Uri) -> Self {
        let server_copy = server.clone();
        let uri_copy = uri.clone();

        let handle = tokio::task::spawn_blocking(move || {
            Self::from_uri_sync(&server_copy, uri_copy)
        });

        handle
            .await
            // The map_err + and_then flattens the Result, allowing us to simultaneously handle either
            // the task panicking or the resource acquisition failing in some way.
            .map_err(Report::from)
            .and_then(std::convert::identity)
            .unwrap_or_else(|err| {
                let error_page = server.render_error_page(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &uri,
                    Some(err)
                ).unwrap_or_else(|err| {
                    format!("500 Internal Server Error (Double Fault)\n{err:?}")
                });

                Self::Error(
                    error_page,
                    StatusCode::INTERNAL_SERVER_ERROR
                )
            })
    }

    fn from_uri_sync(server: &Server, uri: Uri) -> Result<Self> {
        let Some(route) = Self::lookup_route(server, &uri)? else {
            let error_page = server.render_error_page(
                StatusCode::NOT_FOUND,
                &uri,
                None
            )?;

            return Ok(Self::Error(
                error_page,
                StatusCode::NOT_FOUND
            ))
        };

        match route.kind {
            RouteKind::Asset | RouteKind::RedirectAsset => Self::from_asset(&route),
            RouteKind::Page | RouteKind::RedirectPage | RouteKind::Stylesheet => Self::from_text(server, &route),
            RouteKind::Hook => Self::from_hook(server, &uri, &route),
        }
    }

    #[inline]
    fn lookup_route(server: &Server, uri: &Uri) -> Result<Option<Route>> {
        let conn = server.ctx.db.get_ro()?;
        let rev_id = server.rev_id.load();
    
        let mut query = conn.prepare("
            SELECT * FROM routes
            WHERE route IN (?1, ?2)
            AND revision = ?3
        ")?;
        
        // We trim any leading slashes, just in case the user accidentally adds one.
        let path = match uri.path() {
            "/" => "/", // Special case for the root path.
            path => path.trim_end_matches('/')
        };

        let path_and_query = uri
            .path_and_query()
            .map(|path| match path.as_str() {
                "/" => "/", 
                path => path.trim_end_matches('/')
            })
            .unwrap_or("");
        
        // We need to check the path both with and without the query,
        // in order to work with both versioned assets and hooks.
        let parameters = [
            path,
            path_and_query,
            rev_id.as_ref()
        ];
    
        let route = match query
            .query_and_then(parameters, Route::from_row)?
            .next() 
        {
            Some(route) => Ok(Some(route?)),
            None => Ok(None)
        };

        // Necessary because lifetime bullshit
        #[allow(clippy::let_and_return)]
        route
    }

    #[inline]
    fn from_asset(route: &Route) -> Result<Self> {
        let path = format!(
            "{SITE_CACHE_PATH}{}",
            route.id
        );

        let bytes = std::fs::read(path).map(Bytes::from)?;

        Ok(Self::Octets(bytes))
    }

    #[inline]
    fn from_text(server: &Server, route: &Route) -> Result<Self> {
        let conn = server.ctx.db.get_ro()?;
        let id = &*route.id;

        let mut query = conn.prepare_cached("
            SELECT * FROM output
            WHERE id = ?1
        ")?;

        let resource = match query
            .query_and_then([id], Output::from_row)?
            .next()
        {
            Some(output) => {
                let output: Output = output?;
                let content = output.content;

                Ok(Self::Text(content, route.kind))
            }
            None => panic!("Could not find output for page with ID {id}!")
        };

        // Necessary because lifetime bullshit
        #[allow(clippy::let_and_return)]
        resource    
    }

    #[inline]
    fn from_hook(server: &Server, uri: &Uri, route: &Route) -> Result<Self> {
        let conn = server.ctx.db.get_ro()?;
        let id = &*route.id;

        let mut query = conn.prepare("
            SELECT * FROM hooks
            WHERE id = ?1
        ")?;
        
        let Some(hook) = query
            .query_and_then([id], Hook::from_row)? 
            .next()
        else {
            let error_page = server.render_error_page(
                StatusCode::NOT_FOUND,
                uri,
                None
            )?;

            return Ok(
                Self::Error(error_page, StatusCode::NOT_FOUND)
            )
        };

        let hook: Hook = hook?;

        let renderer = server.renderer.load();
        let template = renderer
            .env
            .get_template(&hook.template)
            .map_err(|_| {
                eyre!(
                    "Tried to build a hook with a nonexistent template (\"{}\").",
                    hook.template,
                )
            })?;

        // Scuffed HTTP query parsing.
        // I don't think there are any edge cases lurking uncovered in here, but
        // we'll see.
        let queries = uri
            .query()
            .unwrap_or("")
            .split('&')
            .filter_map(|kwarg| {
                let mut split = kwarg.split('=').take(2);

                match (split.next(), split.next()) {
                    (Some(k), Some(v)) => Some((k, v)),
                    _ => None
                }
            })
            .map(|(k, v)| {
                use std::borrow::Cow;
                use urlencoding::decode;
                
                let lossy = |err: FromUtf8Error| {
                    String::from_utf8_lossy(
                        err.as_bytes()
                    ).into_owned()
                };

                (
                    decode(k)
                        .map(Cow::into_owned)
                        .unwrap_or_else(lossy),
                    decode(v)
                        .map(Cow::into_owned)
                        .unwrap_or_else(lossy)
                )
            });
        
        let output = template.render(context! {
            path => uri.path(),
            queries => Value::from_iter(queries)
        })
        .context("Hook rendering failed")
        .suggestion("Check your template code for bugs.")?;

        let headers = hook.headers
            .split('\n')
            .map(ToOwned::to_owned)
            .tuples()
            .collect();

        Ok(Self::Hook {
            code: StatusCode::OK,
            headers,
            output,
            cache: hook.cache
        })
    }
}

impl Resource {
    pub fn should_cache(&self) -> bool {
        use Resource::*;

        match self {
            Hook { cache, .. } => *cache,
            _ => true
        }
    }
    
    pub fn size(&self) -> usize {
        use std::mem::size_of_val;
        use Resource::*;

        match self {
            Text(content, kind) => content.len() + size_of_val(kind),
            Octets(bytes) => bytes.len(),
            Hook { code, headers, output, cache } => {
                let headers: usize = headers
                    .iter()
                    .map(|(name, value)| {
                        name.len() + value.len()
                    })
                    .sum();

                size_of_val(code) + headers + output.len() + size_of_val(cache)
            },
            Error(content, code) => content.len() + size_of_val(code)
        }
    }
}

impl IntoResponse for Resource {
    fn into_response(self) -> Response {
        use Resource::*;

        match self {
            Text(content, kind) => {
                let headers = match kind {
                    RouteKind::Page | RouteKind::RedirectPage => [
                        ("Content-Type", "text/html; charset=utf-8"),
                        ("Cache-Control", "max-age=500, must-revalidate"),
                    ],
                    RouteKind::Stylesheet => [
                        ("Content-Type", "text/css; charset=utf-8"),
                        ("Cache-Control", "max-age=31536000, immutable"),
                    ],
                    _ => unreachable!()
                };

                (
                    StatusCode::OK,
                    headers,
                    content,
                ).into_response()
            }
            Octets(bytes) => (
                StatusCode::OK,
                [
                    ("Content-Type", "application/octet-stream"),
                    ("Cache-Control", "max-age=31536000, immutable"),
                ],
                bytes,
            ).into_response(),
            Hook { code, headers, output, .. } => {
                let headers = headers
                    .iter()
                    .map(|(name, value)| {
                        use std::str::FromStr;

                        (
                            HeaderName::from_str(name).expect("Hook header name should be valid."),
                            HeaderValue::from_str(value).expect("Hook header value should be valid.")
                        )
                    })
                    .collect::<HeaderMap>();

                (
                    code,
                    headers,
                    output,
                ).into_response()
            }
            Error(response, code) => (
                code,
                [
                    ("Content-Type", "text/html; charset=utf-8"),
                    ("Cache-Control", "max-age=500, must-revalidate"),
                ],
                response
            ).into_response()
        }
    }
}