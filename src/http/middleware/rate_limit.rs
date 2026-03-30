use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::http::state::AppState;

pub async fn rate_limit_by_ip(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let limits = state.pdf_rate_limit;
    let ip = addr.ip();

    if limits.contains_key(&ip) {
        let mut response = (
            StatusCode::TOO_MANY_REQUESTS,
            "too many pdf requests, try again later",
        )
            .into_response();

        response
            .headers_mut()
            .insert("retry-after", HeaderValue::from_static("10"));

        return response;
    }

    let response = next.run(request).await;

    if should_store_cooldown(response.status()) {
        limits.insert(ip, ()).await;
    }

    response
}

fn should_store_cooldown(status: StatusCode) -> bool {
    !status.is_client_error() || status == StatusCode::REQUEST_TIMEOUT
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        net::{IpAddr, Ipv4Addr, SocketAddr},
        sync::Arc,
        time::Duration,
    };

    use axum::{
        Router,
        body::Body,
        extract::ConnectInfo,
        http::{Request, StatusCode},
        middleware::from_fn_with_state,
        routing::post,
    };
    use moka::future::Cache;
    use tower::ServiceExt;

    use crate::{
        http::state::AppState, models::Manifest, repo::cards::ManifestRepo,
        service::pdf::PdfService,
    };

    use super::rate_limit_by_ip;

    #[tokio::test]
    async fn rate_limit_by_ip_rejects_the_second_request_from_the_same_ip() {
        let state = test_state();
        let app = Router::new()
            .route(
                "/pdf",
                post(|| async { StatusCode::OK })
                    .route_layer(from_fn_with_state(state.clone(), rate_limit_by_ip)),
            )
            .with_state(state);

        let client_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 3000));

        let first_response = app
            .clone()
            .oneshot(request_with_connect_info(client_addr))
            .await
            .expect("first request should succeed");
        assert_eq!(first_response.status(), StatusCode::OK);

        let second_response = app
            .oneshot(request_with_connect_info(client_addr))
            .await
            .expect("second request should return a rate limit response");
        assert_eq!(second_response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(second_response.headers().get("retry-after").unwrap(), "10");
    }

    #[tokio::test]
    async fn rate_limit_by_ip_does_not_store_cooldown_for_bad_requests() {
        let state = test_state();
        let app = Router::new()
            .route(
                "/pdf",
                post(|| async { StatusCode::BAD_REQUEST })
                    .route_layer(from_fn_with_state(state.clone(), rate_limit_by_ip)),
            )
            .with_state(state);

        let client_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 3001));

        let first_response = app
            .clone()
            .oneshot(request_with_connect_info(client_addr))
            .await
            .expect("first request should return bad request");
        assert_eq!(first_response.status(), StatusCode::BAD_REQUEST);

        let second_response = app
            .oneshot(request_with_connect_info(client_addr))
            .await
            .expect("second request should also reach the handler");
        assert_eq!(second_response.status(), StatusCode::BAD_REQUEST);
    }

    fn request_with_connect_info(client_addr: SocketAddr) -> Request<Body> {
        let mut request = Request::builder()
            .method(axum::http::Method::POST)
            .uri("/pdf")
            .body(Body::empty())
            .expect("request should be built");
        request.extensions_mut().insert(ConnectInfo(client_addr));
        request
    }

    fn test_state() -> AppState {
        let repo = Arc::new(
            ManifestRepo::new(Manifest {
                asset_root: "assets/images/eoj/main_sets".to_owned(),
                preview_root: "assets/previews/eoj/main_sets".to_owned(),
                cards_by_id: BTreeMap::new(),
            })
            .expect("repo should be created"),
        );

        AppState {
            pdf_service: Arc::new(
                PdfService::new(Arc::clone(&repo)).expect("service should be created"),
            ),
            card_repository: repo,
            pdf_rate_limit: test_rate_limit_cache(),
        }
    }

    fn test_rate_limit_cache() -> Cache<IpAddr, ()> {
        Cache::builder()
            .time_to_live(Duration::from_secs(10))
            .max_capacity(128)
            .build()
    }
}
