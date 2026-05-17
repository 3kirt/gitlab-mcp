use anyhow::Context as _;
use axum::{
    Router,
    extract::Request,
    http::{StatusCode, header},
    middleware::{self, Next},
    response::IntoResponse,
    routing::get,
};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde_json::json;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::tools::GitlabMcpServer;

pub async fn run(listen: &str, base_url: String) -> anyhow::Result<()> {
    let ct = CancellationToken::new();

    let base_url_for_factory = base_url.clone();
    let config = StreamableHttpServerConfig::default()
        .disable_allowed_hosts()
        .with_cancellation_token(ct.child_token());

    let mcp_service: StreamableHttpService<GitlabMcpServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(GitlabMcpServer::new_http(base_url_for_factory.clone())),
            Default::default(),
            config,
        );

    let base_url_for_readyz = base_url.clone();

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route(
            "/readyz",
            get({
                let url = base_url_for_readyz.clone();
                move || readyz(url.clone())
            }),
        )
        .nest_service("/mcp", mcp_service)
        .layer(middleware::from_fn(require_bearer));

    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("binding to {listen}"))?;

    let ct_clone = ct.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            info!("shutdown signal received — draining in-flight requests");
            ct_clone.cancel();
        })
        .await?;

    Ok(())
}

async fn require_bearer(req: Request, next: Next) -> impl IntoResponse {
    let path = req.uri().path().to_owned();
    if path == "/healthz" || path == "/readyz" {
        return next.run(req).await;
    }

    // Per-session token model: require a non-empty Bearer header but do not
    // compare it server-side. The token is extracted in GitlabMcpServer::initialize
    // and forwarded to GitLab, which is the source of truth for validity.
    let has_bearer = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| !t.trim().is_empty())
        .unwrap_or(false);

    if has_bearer {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
        )
            .into_response()
    }
}

async fn healthz() -> impl IntoResponse {
    axum::Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn readyz(base_url: String) -> impl IntoResponse {
    let Ok(url) = url::Url::parse(&base_url) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let host = match url.host_str() {
        Some(h) => h.to_owned(),
        None => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let port = url.port_or_known_default().unwrap_or(443);
    let addr = format!("{host}:{port}");

    match tokio::net::lookup_host(addr).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
