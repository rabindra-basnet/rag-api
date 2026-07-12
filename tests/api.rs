//! Integration tests: real router + real SQLite (temp file per test) + a
//! mock OpenAI-compatible upstream for embeddings and chat completions.

use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use tower::ServiceExt;

use rag_backend::routes;
use rag_backend::state::{http_client, AppState, Config};

/// Deterministic fake embedding: same text -> same vector.
fn fake_embedding(text: &str) -> Vec<f32> {
    let mut h: u64 = 1469598103934665603;
    for b in text.bytes() {
        h = (h ^ b as u64).wrapping_mul(1099511628211);
    }
    (0..8)
        .map(|i| ((h.rotate_left(i * 8) & 0xffff) as f32 / 65535.0) - 0.5)
        .collect()
}

async fn spawn_mock_llm() -> String {
    let app = Router::new()
        .route(
            "/embeddings",
            post(|Json(body): Json<Value>| async move {
                let inputs: Vec<String> = body["input"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let data: Vec<Value> = inputs
                    .iter()
                    .enumerate()
                    .map(|(i, t)| json!({ "index": i, "embedding": fake_embedding(t) }))
                    .collect();
                Json(json!({ "data": data }))
            }),
        )
        .route(
            "/chat/completions",
            post(|Json(body): Json<Value>| async move {
                // Echo part of the prompt so tests can assert context made it in.
                let prompt = body["messages"][1]["content"].as_str().unwrap_or("").to_string();
                let answer = format!("MOCK ANSWER (prompt {} chars)", prompt.len());
                Json(json!({ "choices": [ { "message": { "content": answer } } ] }))
            }),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

async fn test_app() -> Router {
    let mock_url = spawn_mock_llm().await;
    let db_path = std::env::temp_dir().join(format!("ragtest-{}.db", uuid::Uuid::new_v4()));
    let pool = rag_backend::db::init(
        &format!("sqlite://{}?mode=rwc", db_path.display()),
        std::path::Path::new("./migrations"),
    )
    .await
    .expect("test db");

    let cfg = Config {
        jwt_secret: "integration-test-secret".into(),
        access_ttl_minutes: 15,
        refresh_ttl_days: 30,
        cookie_secure: false,
        trust_proxy: false,
        database_url: String::new(),
        migrations_path: "./migrations".into(),
        bind_addr: String::new(),
        llm_base_url: mock_url.clone(),
        llm_api_key: Some("test-key".into()),
        llm_model: "mock-model".into(),
        llm_max_tokens: 256,
        embeddings_base_url: mock_url,
        embeddings_api_key: Some("test-key".into()),
        embeddings_model: "mock-embed".into(),
    };

    routes::router(AppState { db: pool, http: http_client(), cfg })
}

fn req(method: &str, uri: &str, token: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        // Rate limiter's peer-IP key extractor reads ConnectInfo.
        .extension(ConnectInfo("127.0.0.1:9999".parse::<SocketAddr>().unwrap()));
    if let Some(t) = token {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    match body {
        Some(v) => b
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    }
}

async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

async fn register(app: &Router, email: &str) -> (String, String) {
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            "/auth/register",
            None,
            Some(json!({ "email": email, "password": "password123" })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    (
        v["access_token"].as_str().unwrap().to_string(),
        v["refresh_token"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn health_works() {
    let app = test_app().await;
    let resp = app.oneshot(req("GET", "/health", None, None)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn register_login_me_flow() {
    let app = test_app().await;
    let (access, _) = register(&app, "a@test.com").await;

    let resp = app
        .clone()
        .oneshot(req("GET", "/auth/me", Some(&access), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["email"], "a@test.com");

    // wrong password
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            "/auth/login",
            None,
            Some(json!({ "email": "a@test.com", "password": "wrongpass1" })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // duplicate registration
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            "/auth/register",
            None,
            Some(json!({ "email": "a@test.com", "password": "password123" })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn validation_rejects_bad_input() {
    let app = test_app().await;
    for (uri, body) in [
        ("/auth/register", json!({ "email": "not-an-email", "password": "password123" })),
        ("/auth/register", json!({ "email": "b@test.com", "password": "short" })),
    ] {
        let resp = app.clone().oneshot(req("POST", uri, None, Some(body))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn refresh_rotation_and_reuse_detection() {
    let app = test_app().await;
    let (_, refresh1) = register(&app, "rot@test.com").await;

    // Rotate once — succeeds.
    let resp = app
        .clone()
        .oneshot(req("POST", "/auth/refresh", None, Some(json!({ "refresh_token": refresh1 }))))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let refresh2 = body_json(resp).await["refresh_token"].as_str().unwrap().to_string();

    // Replaying the rotated token fails AND revokes the family.
    let resp = app
        .clone()
        .oneshot(req("POST", "/auth/refresh", None, Some(json!({ "refresh_token": refresh1 }))))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // The newer token from the same family is dead too.
    let resp = app
        .clone()
        .oneshot(req("POST", "/auth/refresh", None, Some(json!({ "refresh_token": refresh2 }))))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_routes_require_auth() {
    let app = test_app().await;
    for (method, uri) in [("GET", "/documents"), ("POST", "/chat"), ("GET", "/auth/me")] {
        let resp = app.clone().oneshot(req(method, uri, None, None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
    // Garbage token
    let resp = app
        .clone()
        .oneshot(req("GET", "/auth/me", Some("garbage"), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn document_crud_and_rag_chat() {
    let app = test_app().await;
    let (access, _) = register(&app, "docs@test.com").await;

    // Ingest single
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            "/documents",
            Some(&access),
            Some(json!({ "title": "Capitals", "content": "The capital of Japan is Tokyo." })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let doc = body_json(resp).await;
    let doc_id = doc["id"].as_str().unwrap().to_string();
    assert_eq!(doc["chunks"], 1);

    // Ingest batch
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            "/documents",
            Some(&access),
            Some(json!([
                { "title": "Doc A", "content": "Water boils at 100C." },
                { "title": "Doc B", "content": "Rust ships every six weeks." }
            ])),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["count"], 2);

    // List
    let resp = app
        .clone()
        .oneshot(req("GET", "/documents", Some(&access), None))
        .await
        .unwrap();
    assert_eq!(body_json(resp).await["documents"].as_array().unwrap().len(), 3);

    // Update re-embeds
    let resp = app
        .clone()
        .oneshot(req(
            "PUT",
            &format!("/documents/{doc_id}"),
            Some(&access),
            Some(json!({ "title": "Capitals v2", "content": "The capital of France is Paris." })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Chat hits the mock LLM and returns sources
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            "/chat",
            Some(&access),
            Some(json!({ "question": "What is the capital of France?" })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let chat = body_json(resp).await;
    assert!(chat["answer"].as_str().unwrap().starts_with("MOCK ANSWER"));
    assert!(!chat["sources"].as_array().unwrap().is_empty());

    // Delete
    let resp = app
        .clone()
        .oneshot(req("DELETE", &format!("/documents/{doc_id}"), Some(&access), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = app
        .clone()
        .oneshot(req("DELETE", &format!("/documents/{doc_id}"), Some(&access), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn users_cannot_see_each_others_documents() {
    let app = test_app().await;
    let (alice, _) = register(&app, "alice@test.com").await;
    let (bob, _) = register(&app, "bob@test.com").await;

    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            "/documents",
            Some(&alice),
            Some(json!({ "title": "Secret", "content": "Alice's private notes." })),
        ))
        .await
        .unwrap();
    let doc_id = body_json(resp).await["id"].as_str().unwrap().to_string();

    // Bob sees an empty list and can't delete or update Alice's doc.
    let resp = app
        .clone()
        .oneshot(req("GET", "/documents", Some(&bob), None))
        .await
        .unwrap();
    assert!(body_json(resp).await["documents"].as_array().unwrap().is_empty());

    let resp = app
        .clone()
        .oneshot(req("DELETE", &format!("/documents/{doc_id}"), Some(&bob), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let resp = app
        .clone()
        .oneshot(req(
            "PUT",
            &format!("/documents/{doc_id}"),
            Some(&bob),
            Some(json!({ "title": "Hijack", "content": "gotcha" })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn plain_text_ingestion() {
    let app = test_app().await;
    let (access, _) = register(&app, "plain@test.com").await;

    let request = Request::builder()
        .method("POST")
        .uri("/documents?title=Notes")
        .extension(ConnectInfo("127.0.0.1:9999".parse::<SocketAddr>().unwrap()))
        .header(header::AUTHORIZATION, format!("Bearer {access}"))
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("Plain text document body."))
        .unwrap();
    let resp = app.clone().oneshot(request).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["title"], "Notes");
}
