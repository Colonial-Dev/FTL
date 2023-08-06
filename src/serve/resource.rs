use axum::response::Response;
use axum::{response::IntoResponse, http::StatusCode, body::Bytes};

use crate::prelude::*;
use crate::db::*;

#[derive(Debug, Clone)]
pub enum Resource {
    Hypertext(String),
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
        
                let mut get_output = conn.prepare_reader(
                    query, 
                    (1, id).into()
                )?;
            
                match get_output.next() {
                    Some(output) => {
                        let output: Output = output?;
                        let content = output.content;

                        if matches!(route.kind, RouteKind::Page | RouteKind::RedirectPage) {
                            Ok(Self::Hypertext(content))
                        } else {
                            Ok(Self::Plaintext(content))
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
            Hypertext(html) => {
                (
                    StatusCode::OK,
                    [
                        ("Content-Type", "text/html"),
                        ("Cache-Control", "max-age=500, must-revalidate")
                    ],
                    html
                ).into_response()
            },
            Plaintext(str) => {
                (
                    StatusCode::OK,
                    [
                        ("Content-Type", "text/plain"),
                        ("Cache-Control", "max-age=500, must-revalidate")
                    ],
                    str
                ).into_response()
            },
            Octets(bytes) => {
                (
                    StatusCode::OK,
                    [
                        ("Content-Type", "application/octet-stream"),
                        ("Cache-Control", "max-age=31536000, immutable")
                    ],
                    bytes
                ).into_response()
            },
            Code(status) => status.into_response(),
        }
    }
}