use regex::Regex;
use tauri::State;

use crate::core::state::AppState;
use crate::infra::db::open_connection;
use crate::models::common::PipelineStage;
use crate::models::metrics::SearchHit;

pub(crate) fn build_fts_match_query(input: &str) -> Option<String> {
    let token_regex = Regex::new(r"[\p{L}\p{N}_]+").ok()?;
    let tokens = token_regex
        .find_iter(input)
        .map(|item| item.as_str().to_lowercase())
        .filter(|item| !item.is_empty())
        .take(8)
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return None;
    }

    Some(
        tokens
            .into_iter()
            .map(|token| format!("\"{token}\"*"))
            .collect::<Vec<_>>()
            .join(" AND "),
    )
}

#[tauri::command]
pub(crate) fn search_candidates(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<SearchHit>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let Some(match_query) = build_fts_match_query(&query) else {
        return Ok(Vec::new());
    };

    let mut stmt = conn
        .prepare(
            r#"
            SELECT c.id, c.name, c.stage, snippet(candidate_search, 3, '<b>', '</b>', '…', 10)
            FROM candidate_search
            JOIN candidates c ON c.id = candidate_search.candidate_id
            WHERE candidate_search MATCH ?1
            ORDER BY rank
            LIMIT 50
            "#,
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([match_query], |row| {
            let stage_text: String = row.get(2)?;
            Ok(SearchHit {
                candidate_id: row.get(0)?,
                name: row.get(1)?,
                stage: PipelineStage::from_db(&stage_text).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?,
                snippet: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}
