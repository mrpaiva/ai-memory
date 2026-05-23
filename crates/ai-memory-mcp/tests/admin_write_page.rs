//! Integration tests for `POST /admin/write-page`.
//!
//! Exercises the route through the axum router: post a synthetic page,
//! verify it appears in `/admin/search` results. Also tests that an
//! unknown tier returns 422.

use ai_memory_mcp::{AdminState, admin_router};
use ai_memory_store::{DecayParams, Store};
use ai_memory_wiki::Wiki;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tempfile::TempDir;
use tower::ServiceExt;

async fn make_state(tmp: &TempDir) -> AdminState {
    let store = Store::open(tmp.path()).unwrap();
    let wiki = Wiki::new(tmp.path(), store.writer.clone()).unwrap();
    let db_path = store.db_path().to_path_buf();
    AdminState {
        writer: store.writer.clone(),
        reader: store.reader.clone(),
        wiki,
        llm: None,
        embedder: None,
        decay_params: DecayParams::default(),
        data_dir: tmp.path().to_path_buf(),
        db_path,
        bind: "127.0.0.1:0".to_string(),
    }
}

async fn post_json(
    state: AdminState,
    uri: &str,
    body: serde_json::Value,
) -> axum::response::Response {
    let router = admin_router(state);
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    router.oneshot(req).await.unwrap()
}

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

#[tokio::test]
async fn write_page_returns_page_id_and_path() {
    let tmp = TempDir::new().unwrap();
    let state = make_state(&tmp).await;

    let resp = post_json(
        state,
        "/admin/write-page",
        json!({
            "workspace": "default",
            "project": "scratch",
            "path": "notes/test-write.md",
            "body": "This is a test page written via the admin route.",
            "tier": "semantic",
        }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK, "write-page must succeed");

    let body = body_json(resp).await;
    assert!(
        body["page_id"].is_string(),
        "response must have page_id: {body}"
    );
    assert_eq!(
        body["path"].as_str().unwrap(),
        "notes/test-write.md",
        "response path must match request: {body}"
    );
}

#[tokio::test]
async fn write_page_appears_in_search() {
    let tmp = TempDir::new().unwrap();
    let state = make_state(&tmp).await;

    // Write a page with a distinctive term.
    let write_resp = post_json(
        state.clone(),
        "/admin/write-page",
        json!({
            "workspace": "default",
            "project": "scratch",
            "path": "notes/unique-term.md",
            "body": "The xyloquartz pattern enables distributed widget fusion.",
            "tier": "semantic",
        }),
    )
    .await;
    assert_eq!(write_resp.status(), StatusCode::OK);

    // Now search for the distinctive term.
    let router = admin_router(state);
    let search_req = Request::builder()
        .method("GET")
        .uri("/admin/search?q=xyloquartz&limit=10")
        .body(Body::empty())
        .unwrap();
    let search_resp = router.oneshot(search_req).await.unwrap();
    assert_eq!(search_resp.status(), StatusCode::OK);

    let hits: Vec<serde_json::Value> = serde_json::from_slice(
        &axum::body::to_bytes(search_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        hits.len(),
        1,
        "written page must appear in search results: {hits:?}"
    );
    assert_eq!(hits[0]["path"].as_str().unwrap(), "notes/unique-term.md");
}

#[tokio::test]
async fn write_page_invalid_tier_returns_422() {
    let tmp = TempDir::new().unwrap();
    let state = make_state(&tmp).await;

    let resp = post_json(
        state,
        "/admin/write-page",
        json!({
            "workspace": "default",
            "project": "scratch",
            "path": "notes/bad-tier.md",
            "body": "Some content.",
            "tier": "legendary",
        }),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "unknown tier must return 422"
    );

    let body = body_json(resp).await;
    assert!(
        body["error"].as_str().unwrap_or("").contains("legendary"),
        "error must mention the unknown tier name: {body}"
    );
}

#[tokio::test]
async fn write_page_with_tags_and_pinned() {
    let tmp = TempDir::new().unwrap();
    let state = make_state(&tmp).await;

    let resp = post_json(
        state,
        "/admin/write-page",
        json!({
            "workspace": "default",
            "project": "scratch",
            "path": "notes/tagged.md",
            "body": "Tagged and pinned content.",
            "tier": "procedural",
            "tags": ["rust", "memory"],
            "pinned": true,
        }),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "tagged+pinned page must succeed"
    );

    let body = body_json(resp).await;
    assert!(body["page_id"].is_string(), "must have page_id: {body}");
}
