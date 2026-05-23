//! Integration tests for `POST /admin/backup`.
//!
//! Builds a real [`AdminState`] over a tmpdir-backed store + wiki,
//! seeds a page, POSTs to `/admin/backup`, and asserts:
//! - the response Content-Type is `application/gzip`,
//! - the body is a valid gzip/tar stream,
//! - the seeded wiki file appears in the tarball.

use ai_memory_core::{NewPage, PagePath, Tier};
use ai_memory_mcp::{AdminState, admin_router};
use ai_memory_store::{DecayParams, Store};
use ai_memory_wiki::Wiki;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use flate2::read::GzDecoder;
use tar::Archive;
use tempfile::TempDir;
use tower::ServiceExt;

async fn make_state(tmp: &TempDir) -> (AdminState, Store) {
    let store = Store::open(tmp.path()).unwrap();
    let wiki = Wiki::new(tmp.path(), store.writer.clone()).unwrap();
    let db_path = store.db_path().to_path_buf();
    let state = AdminState {
        writer: store.writer.clone(),
        reader: store.reader.clone(),
        wiki,
        llm: None,
        embedder: None,
        decay_params: DecayParams::default(),
        data_dir: tmp.path().to_path_buf(),
        db_path,
        bind: "127.0.0.1:0".to_string(),
    };
    (state, store)
}

/// Seed a page directly into the store (index only — we also need the
/// on-disk wiki file for the tarball). Use `AdminState.wiki.write_page`
/// via the state, but here we seed through the store + write the file
/// directly to exercise the backup path.
async fn seed_page(store: &Store, tmp: &TempDir, path: &str, body: &str) {
    let ws = store
        .writer
        .get_or_create_workspace("default".to_string())
        .await
        .unwrap();
    let proj = store
        .writer
        .get_or_create_project(ws, "scratch".to_string(), None)
        .await
        .unwrap();
    store
        .writer
        .upsert_page(NewPage {
            workspace_id: ws,
            project_id: proj,
            path: PagePath::new(path).unwrap(),
            title: "Test".to_string(),
            body: body.to_string(),
            tier: Tier::Semantic,
            frontmatter_json: serde_json::json!({}),
            pinned: false,
        })
        .await
        .unwrap();

    // Also write the file on disk so the tarball picks it up.
    let full = tmp.path().join("wiki").join(path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&full, body).unwrap();
}

#[tokio::test]
async fn backup_returns_application_gzip_content_type() {
    let tmp = TempDir::new().unwrap();
    let (state, store) = make_state(&tmp).await;
    seed_page(&store, &tmp, "concepts/karpathy.md", "compile not retrieve").await;

    let router = admin_router(state);
    let req = Request::builder()
        .method("POST")
        .uri("/admin/backup")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("application/gzip"),
        "expected application/gzip content-type, got: {ct}"
    );
}

#[tokio::test]
async fn backup_body_is_valid_gzip_and_contains_seeded_page() {
    let tmp = TempDir::new().unwrap();
    let (state, store) = make_state(&tmp).await;
    seed_page(
        &store,
        &tmp,
        "concepts/karpathy.md",
        "compile-not-retrieve from Karpathy LLM wiki",
    )
    .await;

    let router = admin_router(state);
    let req = Request::builder()
        .method("POST")
        .uri("/admin/backup")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(!bytes.is_empty(), "backup body must not be empty");

    // Decompress and list entries.
    let decoder = GzDecoder::new(bytes.as_ref());
    let mut archive = Archive::new(decoder);
    let entries: Vec<String> = archive
        .entries()
        .expect("tarball must be readable")
        .map(|e| {
            e.expect("entry must be readable")
                .path()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    assert!(
        entries.iter().any(|p| p.contains("concepts/karpathy.md")),
        "tarball must contain the seeded page; entries: {entries:?}"
    );
    assert!(
        entries.iter().any(|p| p.contains("memory.sqlite")),
        "tarball must contain the db snapshot; entries: {entries:?}"
    );
}

#[tokio::test]
async fn backup_empty_store_still_succeeds() {
    // Even with no wiki pages, the backup must not return an error.
    let tmp = TempDir::new().unwrap();
    let (state, _store) = make_state(&tmp).await;

    let router = admin_router(state);
    let req = Request::builder()
        .method("POST")
        .uri("/admin/backup")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(!bytes.is_empty());
}
