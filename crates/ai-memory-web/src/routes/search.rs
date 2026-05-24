//! `GET /search?q=…` — FTS5 search results.

use std::sync::Arc;

use askama::Template;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use serde::Deserialize;

use crate::markdown;
use crate::state::WebState;
use crate::templates::{SearchHit, SearchView, page_href};

/// Query-string parameters for the search endpoint.
#[derive(Debug, Deserialize)]
pub(crate) struct SearchParams {
    /// The free-text search query.
    #[serde(default)]
    pub q: String,
}

/// Handler for `GET /search?q=…`.
pub(crate) async fn handler(
    State(state): State<Arc<WebState>>,
    Query(params): Query<SearchParams>,
) -> Result<Html<String>, StatusCode> {
    let query = params.q.trim().to_owned();

    let hits = if query.is_empty() {
        Vec::new()
    } else {
        let raw = state
            .reader
            .search_pages(query.clone(), 50)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Search hits carry only the page id/path/title/snippet. Resolve
        // workspace + project by page id so duplicate paths in sibling
        // projects still link to the right page.
        let mut results = Vec::with_capacity(raw.len());
        for h in raw {
            if let Ok(Some(m)) = state.reader.page_meta_by_id(h.id).await {
                let path = h.path.as_str().to_owned();
                let href = page_href(&m.workspace_name, &m.project_name, &path);
                results.push(SearchHit {
                    workspace: m.workspace_name,
                    project: m.project_name,
                    path,
                    href,
                    title: h.title,
                    snippet: markdown::escape_snippet(&h.snippet),
                });
            } else {
                // Fallback: no workspace/project known; skip for now.
            }
        }
        results
    };

    let hit_count = hits.len();
    let html = SearchView {
        query,
        hits,
        hit_count,
    }
    .render()
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Html(html))
}
