use std::net::SocketAddr;

use anyhow::{anyhow, Error};
use axum::extract::Query;
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use errors::ApiError;
use lazy_static::lazy_static;
use reqwest::Client;
use response::Bson;
use serde::{Deserialize, Serialize};
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info};

use common::response;
use common::{errors, init_tracing};

lazy_static! {
    static ref PORT: u16 = std::env::var("PORT")
        .ok()
        .and_then(|it| it.parse().ok())
        .unwrap_or(3000);
    static ref COMPILER_URL: String =
        std::env::var("COMPILER_URL").expect("COMPILER_URL must be set");
    static ref CLINET: Client = Client::new();
}

#[derive(Deserialize)]
struct RunPayload {
    code: String,
}

#[derive(Serialize)]
struct RunResponse {
    index_html: String,
    js: String,
    wasm: Vec<u8>,
}

const INDEX_HTML: &str = r#"
<!doctype html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, user-scalable=no, initial-scale=1.0, maximum-scale=1.0, minimum-scale=1.0">
    <meta http-equiv="X-UA-Compatible" content="ie=edge">
    <title>Document</title>
</head>
<body>
    <script type="module">
    /*JS_GOES_HERE*/
    /*INIT_GOES_HERE*/
    </script>
</body>
</html>
"#;

async fn run(Query(body): Query<RunPayload>) -> Result<Html<String>, ApiError> {
    let client = &*CLINET;

    let res = client
        .post(format!("{}/run", *COMPILER_URL))
        .body(body.code)
        .send()
        .await
        .map_err(Error::from)?;

    let status = res.status();
    debug!(status = ?status, "got response from compiler");

    if !status.is_success() {
        return Err(ApiError::Unknown(
            anyhow!("Compiler service returned an error: {}", res.text().await.unwrap())
        ))
    }

    let run_response: common::Response = {
        let bytes = res.bytes().await.map_err(|e| {
            error!(?e, "failed to get bytes from compiler response");
            ApiError::Unknown(e.into())
        })?;
        bson::from_slice(&bytes).map_err(|e| {
            error!(?e, "failed to deserialize compiler response");
            ApiError::BsonDeserializeError(e)
        })?
    };


    match run_response {
        common::Response::Output {
            index_html: _,
            js,
            wasm,
        } => {
            debug!(wasm_bytes = wasm.len(), "compilation successful");
            let init_fn = js.split("export default").nth(1).and_then(|it| it.trim().strip_suffix(";"));
            match init_fn {
                Some(init_fn) => {
                    let index_html = INDEX_HTML.replace("/*JS_GOES_HERE*/", &js);
                    let init = format!("{}((new Int8Array({:?})).buffer)", init_fn, wasm);
                    let index_html = index_html.replace("/*INIT_GOES_HERE*/", &init);

                    Ok(Html(index_html))
                }
                None => {
                    return Err(ApiError::Unknown(anyhow!("failed to find init function as default export in js")))
                }
            }
        }
        common::Response::CompileError(e) => Ok(Html(e)),
    }
}

async fn hello() -> Bson<RunResponse> {
    Bson(RunResponse {
        index_html: "index_html".to_string(),
        js: "js".to_string(),
        wasm: "wasm".as_bytes().to_vec(),
    })
}

#[tokio::main]
async fn main() {
    init_tracing();

    let api = Router::new()
        .route("/hello", get(hello))
        .route("/run", get(run))
        .layer(TraceLayer::new_for_http());

    let app = Router::new().nest("/api", api);

    let addr = SocketAddr::new("0.0.0.0".parse().unwrap(), *PORT);
    info!("Server running on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
