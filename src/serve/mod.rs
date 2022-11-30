use axum::{extract::Path, response::{IntoResponse, Response}, routing::get, Router};

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
    let app = Router::new().route("/*path", get(resource));

    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

async fn resource(Path(path): Path<String>) -> Response {
    debug!("{path:?}");
    let path = path.trim_start_matches('/');
    let conn = crate::db::make_connection().unwrap();

    if path == "java.png" {
        let bytes = std::fs::read(".ftl/cache/f7b41052ef7e03fb").unwrap();
        return bytes.into_response();
    } else if path == "image.png" {
        let bytes = std::fs::read(".ftl/cache/8445f4dbc24b4507").unwrap();
        return bytes.into_response();
    } else if path == "style.9813baff8c2660ad.css" {
        return "body {
            background-color: #222222;
            color: lightgray;
            font-size: 18px;
          }".into_response()
    }

    let mut stmt = conn
        .prepare(
            "
        SELECT output.content FROM output, routes
        WHERE routes.route = ?1
        AND output.id = routes.id
        LIMIT 1
    ",
        )
        .unwrap();

    let contents: Result<String> =
        serde_rusqlite::from_rows::<String>(stmt.query(rusqlite::params![path]).unwrap())
            .map(|x| x.wrap_err("SQLite deserialization error!"))
            .collect();
    debug!("{contents:?}");
    let contents: axum::response::Html<String> = contents.unwrap().into();
    contents.into_response()
}

fn fetch_resource() {

}
