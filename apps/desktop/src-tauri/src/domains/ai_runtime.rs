use chrono::Utc;
use reqwest::blocking::{
    multipart::{Form, Part},
    Client,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::time::{Duration, Instant};
#[cfg(test)]
use std::collections::BTreeMap;

use crate::core::cipher::FieldCipher;
use crate::core::error::{AppError, AppResult};
use crate::core::time::now_iso;
use crate::models::ai::{
    AiProviderProfileView, AiProviderSettingsView, ResolvedAiProviderSettings,
    StoredAiProviderProfile, StoredAiProviderProfiles, StoredAiProviderSettings,
    TaskRuntimeSettings, UpsertAiProviderSettingsInput,
};
#[cfg(test)]
use crate::models::ai::{AiAnalysisPayload, AiPromptContext, DimensionScore, EvidenceItem};
use crate::models::common::AiProvider;

#[cfg(test)]
pub(crate) fn clamp_score(value: i32) -> i32 {
    value.clamp(0, 100)
}

pub(crate) const AI_SETTINGS_KEY: &str = "ai_provider_settings_v1";
pub(crate) const AI_SETTINGS_PROFILES_KEY: &str = "ai_provider_profiles_v1";
pub(crate) const TASK_RUNTIME_SETTINGS_KEY: &str = "task_runtime_settings_v1";

pub(crate) fn default_task_runtime_settings() -> TaskRuntimeSettings {
    TaskRuntimeSettings {
        auto_batch_concurrency: 2,
        auto_retry_count: 1,
        auto_retry_backoff_ms: 450,
    }
}

pub(crate) fn normalize_task_runtime_settings(input: TaskRuntimeSettings) -> TaskRuntimeSettings {
    TaskRuntimeSettings {
        auto_batch_concurrency: input.auto_batch_concurrency.clamp(1, 8),
        auto_retry_count: input.auto_retry_count.clamp(0, 6),
        auto_retry_backoff_ms: input.auto_retry_backoff_ms.clamp(100, 8_000),
    }
}

pub(crate) fn read_task_runtime_settings(conn: &Connection) -> AppResult<TaskRuntimeSettings> {
    let maybe_text = conn
        .query_row(
            "SELECT value_json FROM app_settings WHERE key = ?1",
            [TASK_RUNTIME_SETTINGS_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    if let Some(text) = maybe_text {
        if let Ok(settings) = serde_json::from_str::<TaskRuntimeSettings>(&text) {
            return Ok(normalize_task_runtime_settings(settings));
        }
    }

    Ok(default_task_runtime_settings())
}

pub(crate) fn write_task_runtime_settings(
    conn: &Connection,
    settings: &TaskRuntimeSettings,
) -> AppResult<()> {
    let normalized = normalize_task_runtime_settings(settings.clone());
    let value = serde_json::to_string(&normalized)?;
    let now = now_iso();
    conn.execute(
        r#"
        INSERT INTO app_settings(key, value_json, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET
            value_json = excluded.value_json,
            updated_at = excluded.updated_at
        "#,
        params![TASK_RUNTIME_SETTINGS_KEY, value, now],
    )?;
    Ok(())
}

pub(crate) fn extract_json_object_block(text: &str) -> Option<String> {
    let mut start: Option<usize> = None;
    let mut depth: i32 = 0;

    for (index, ch) in text.char_indices() {
        if ch == '{' {
            if start.is_none() {
                start = Some(index);
            }
            depth += 1;
            continue;
        }

        if ch == '}' && depth > 0 {
            depth -= 1;
            if depth == 0 {
                if let Some(start_index) = start {
                    return Some(text[start_index..=index].to_string());
                }
            }
        }
    }

    None
}

pub(crate) fn parse_json_from_text(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Ok(value);
    }

    let extracted = extract_json_object_block(trimmed)
        .ok_or_else(|| "provider_response_not_json".to_string())?;
    serde_json::from_str::<Value>(&extracted).map_err(|error| error.to_string())
}

#[cfg(test)]
pub(crate) fn get_array_strings(value: &Value, snake: &str, camel: &str) -> Vec<String> {
    value
        .get(snake)
        .or_else(|| value.get(camel))
        .and_then(|item| item.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(|text| text.trim().to_string()))
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn get_i32(value: &Value, snake: &str, camel: &str) -> Option<i32> {
    value
        .get(snake)
        .or_else(|| value.get(camel))
        .and_then(|item| item.as_i64())
        .map(|number| number as i32)
}

#[cfg(test)]
pub(crate) fn get_f64(value: &Value, snake: &str, camel: &str) -> Option<f64> {
    value
        .get(snake)
        .or_else(|| value.get(camel))
        .and_then(|item| item.as_f64())
}

#[cfg(test)]
pub(crate) fn parse_dimension_scores(value: &Value) -> Vec<DimensionScore> {
    let rows = value
        .get("dimension_scores")
        .or_else(|| value.get("dimensionScores"))
        .and_then(|item| item.as_array())
        .cloned()
        .unwrap_or_default();

    rows.iter()
        .filter_map(|item| {
            let key = item
                .get("key")
                .and_then(|field| field.as_str())?
                .trim()
                .to_string();
            if key.is_empty() {
                return None;
            }

            let score = item
                .get("score")
                .and_then(|field| field.as_i64())
                .map(|field| field as i32)
                .unwrap_or(70);
            let reason = item
                .get("reason")
                .and_then(|field| field.as_str())
                .map(|field| field.trim().to_string())
                .filter(|field| !field.is_empty())
                .unwrap_or_else(|| "模型未给出充分理由，采用默认说明。".to_string());

            Some(DimensionScore {
                key,
                score: clamp_score(score),
                reason,
            })
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn parse_evidence(value: &Value) -> Vec<EvidenceItem> {
    let rows = value
        .get("evidence")
        .and_then(|item| item.as_array())
        .cloned()
        .unwrap_or_default();

    rows.iter()
        .filter_map(|item| {
            let dimension = item
                .get("dimension")
                .and_then(|field| field.as_str())
                .map(|field| field.trim().to_string())
                .filter(|field| !field.is_empty())?;
            let statement = item
                .get("statement")
                .and_then(|field| field.as_str())
                .map(|field| field.trim().to_string())
                .filter(|field| !field.is_empty())?;
            let source_snippet = item
                .get("source_snippet")
                .or_else(|| item.get("sourceSnippet"))
                .and_then(|field| field.as_str())
                .map(|field| field.trim().to_string())
                .filter(|field| !field.is_empty())
                .unwrap_or_else(|| "模型未返回证据片段。".to_string());

            Some(EvidenceItem {
                dimension,
                statement,
                source_snippet,
            })
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn parse_ai_provider_response(text: &str) -> Result<AiAnalysisPayload, String> {
    let value = parse_json_from_text(text)?;
    let overall_score = get_i32(&value, "overall_score", "overallScore")
        .map(clamp_score)
        .ok_or_else(|| "provider_overall_score_missing".to_string())?;

    let dimension_scores = parse_dimension_scores(&value);
    if dimension_scores.is_empty() {
        return Err("provider_dimension_scores_empty".to_string());
    }

    let risks = get_array_strings(&value, "risks", "risks");
    let highlights = get_array_strings(&value, "highlights", "highlights");
    let suggestions = get_array_strings(&value, "suggestions", "suggestions");
    let evidence = parse_evidence(&value);
    let confidence = get_f64(&value, "confidence", "confidence");

    Ok(AiAnalysisPayload {
        overall_score,
        dimension_scores,
        risks,
        highlights,
        suggestions,
        evidence,
        confidence,
    })
}

#[cfg(test)]
pub(crate) fn ensure_analysis_payload(
    payload: AiAnalysisPayload,
    fallback: &AiAnalysisPayload,
) -> AiAnalysisPayload {
    let mut dimensions = BTreeMap::<String, DimensionScore>::new();
    for score in payload.dimension_scores {
        dimensions.insert(
            score.key.clone(),
            DimensionScore {
                key: score.key,
                score: clamp_score(score.score),
                reason: score.reason,
            },
        );
    }

    for score in &fallback.dimension_scores {
        dimensions
            .entry(score.key.clone())
            .or_insert_with(|| score.clone());
    }

    let ordered_keys = ["skill_match", "experience", "compensation", "stability"];
    let dimension_scores = ordered_keys
        .iter()
        .filter_map(|key| dimensions.remove(*key))
        .collect::<Vec<_>>();

    let weighted_score = clamp_score(
        (dimension_scores
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let weight = match index {
                    0 => 0.4,
                    1 => 0.25,
                    2 => 0.15,
                    _ => 0.2,
                };
                item.score as f64 * weight
            })
            .sum::<f64>())
        .round() as i32,
    );

    let overall_score = if payload.overall_score > 0 {
        clamp_score(payload.overall_score)
    } else {
        weighted_score
    };

    AiAnalysisPayload {
        overall_score,
        dimension_scores,
        risks: if payload.risks.is_empty() {
            fallback.risks.clone()
        } else {
            payload.risks
        },
        highlights: if payload.highlights.is_empty() {
            fallback.highlights.clone()
        } else {
            payload.highlights
        },
        suggestions: if payload.suggestions.is_empty() {
            fallback.suggestions.clone()
        } else {
            payload.suggestions
        },
        evidence: if payload.evidence.is_empty() {
            fallback.evidence.clone()
        } else {
            payload.evidence
        },
        confidence: payload.confidence,
    }
}

pub(crate) fn make_ai_profile_id() -> String {
    format!(
        "profile-{}-{:08x}",
        Utc::now().timestamp_millis(),
        rand::random::<u32>()
    )
}

pub(crate) fn profile_default_name(provider: AiProvider, ordinal: usize) -> String {
    if ordinal <= 1 {
        format!("{} 默认", provider.label())
    } else {
        format!("{} 配置{}", provider.label(), ordinal)
    }
}

pub(crate) fn profile_to_stored_settings(
    profile: &StoredAiProviderProfile,
) -> StoredAiProviderSettings {
    StoredAiProviderSettings {
        provider: profile.provider.clone(),
        model: profile.model.clone(),
        base_url: profile.base_url.clone(),
        api_key_enc: profile.api_key_enc.clone(),
        temperature: profile.temperature,
        max_tokens: profile.max_tokens,
        timeout_secs: profile.timeout_secs,
        retry_count: profile.retry_count,
    }
}

pub(crate) fn build_profile_from_settings(
    settings: &StoredAiProviderSettings,
    name: String,
    created_at: String,
) -> StoredAiProviderProfile {
    let provider = AiProvider::from_db(&settings.provider);
    let provider_raw = settings.provider.trim().to_lowercase();
    let legacy_mock_provider = provider_raw == "mock";

    let model = {
        let text = settings.model.trim();
        if legacy_mock_provider || text.is_empty() {
            provider.default_model().to_string()
        } else {
            text.to_string()
        }
    };

    let base_url = {
        let text = settings.base_url.trim().trim_end_matches('/');
        if legacy_mock_provider || text.is_empty() {
            provider.default_base_url().to_string()
        } else {
            text.to_string()
        }
    };

    StoredAiProviderProfile {
        id: make_ai_profile_id(),
        name,
        provider: provider.as_db().to_string(),
        model,
        base_url,
        api_key_enc: settings.api_key_enc.clone(),
        temperature: settings.temperature.clamp(0.0, 1.2),
        max_tokens: settings.max_tokens.clamp(200, 8192),
        timeout_secs: settings.timeout_secs.clamp(8, 180),
        retry_count: settings.retry_count.clamp(1, 5),
        created_at: created_at.clone(),
        updated_at: created_at,
    }
}

pub(crate) fn normalize_profile_in_place(profile: &mut StoredAiProviderProfile, ordinal: usize) {
    let provider = AiProvider::from_db(&profile.provider);
    profile.provider = provider.as_db().to_string();

    let model = profile.model.trim();
    profile.model = if model.is_empty() {
        provider.default_model().to_string()
    } else {
        model.to_string()
    };

    let base_url = profile.base_url.trim().trim_end_matches('/');
    profile.base_url = if base_url.is_empty() {
        provider.default_base_url().to_string()
    } else {
        base_url.to_string()
    };

    let name = profile.name.trim();
    profile.name = if name.is_empty() {
        profile_default_name(provider, ordinal)
    } else {
        name.to_string()
    };

    profile.temperature = profile.temperature.clamp(0.0, 1.2);
    profile.max_tokens = profile.max_tokens.clamp(200, 8192);
    profile.timeout_secs = profile.timeout_secs.clamp(8, 180);
    profile.retry_count = profile.retry_count.clamp(1, 5);

    if profile.id.trim().is_empty() {
        profile.id = make_ai_profile_id();
    }
    if profile.created_at.trim().is_empty() {
        profile.created_at = now_iso();
    }
    if profile.updated_at.trim().is_empty() {
        profile.updated_at = profile.created_at.clone();
    }
}

pub(crate) fn active_profile<'a>(
    profiles: &'a StoredAiProviderProfiles,
) -> Option<&'a StoredAiProviderProfile> {
    profiles
        .profiles
        .iter()
        .find(|item| item.id == profiles.active_profile_id)
}

pub(crate) fn read_legacy_ai_settings(conn: &Connection) -> AppResult<StoredAiProviderSettings> {
    let maybe_text = conn
        .query_row(
            "SELECT value_json FROM app_settings WHERE key = ?1",
            [AI_SETTINGS_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    if let Some(text) = maybe_text {
        if let Ok(settings) = serde_json::from_str::<StoredAiProviderSettings>(&text) {
            return Ok(settings);
        }
    }

    Ok(StoredAiProviderSettings::defaults(AiProvider::Qwen))
}

pub(crate) fn write_legacy_ai_settings(
    conn: &Connection,
    settings: &StoredAiProviderSettings,
) -> AppResult<()> {
    let value = serde_json::to_string(settings)?;
    let now = now_iso();
    conn.execute(
        r#"
        INSERT INTO app_settings(key, value_json, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET
            value_json = excluded.value_json,
            updated_at = excluded.updated_at
        "#,
        params![AI_SETTINGS_KEY, value, now],
    )?;
    Ok(())
}

pub(crate) fn read_ai_profiles(conn: &Connection) -> AppResult<StoredAiProviderProfiles> {
    let maybe_text = conn
        .query_row(
            "SELECT value_json FROM app_settings WHERE key = ?1",
            [AI_SETTINGS_PROFILES_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    if let Some(text) = maybe_text {
        if let Ok(mut state) = serde_json::from_str::<StoredAiProviderProfiles>(&text) {
            state.profiles.retain(|item| !item.id.trim().is_empty());
            for (index, profile) in state.profiles.iter_mut().enumerate() {
                normalize_profile_in_place(profile, index + 1);
            }
            if !state.profiles.is_empty() {
                if !state
                    .profiles
                    .iter()
                    .any(|item| item.id == state.active_profile_id)
                {
                    state.active_profile_id = state.profiles[0].id.clone();
                }
                return Ok(state);
            }
        }
    }

    let legacy = read_legacy_ai_settings(conn)?;
    let fallback_provider = AiProvider::from_db(&legacy.provider);
    let fallback_name = profile_default_name(fallback_provider, 1);
    let created_at = now_iso();
    let profile = build_profile_from_settings(&legacy, fallback_name, created_at);
    Ok(StoredAiProviderProfiles {
        active_profile_id: profile.id.clone(),
        profiles: vec![profile],
    })
}

pub(crate) fn write_ai_profiles(
    conn: &Connection,
    state: &StoredAiProviderProfiles,
) -> AppResult<()> {
    let value = serde_json::to_string(state)?;
    let now = now_iso();
    conn.execute(
        r#"
        INSERT INTO app_settings(key, value_json, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET
            value_json = excluded.value_json,
            updated_at = excluded.updated_at
        "#,
        params![AI_SETTINGS_PROFILES_KEY, value, now],
    )?;

    if let Some(active) = active_profile(state) {
        write_legacy_ai_settings(conn, &profile_to_stored_settings(active))?;
    }

    Ok(())
}

pub(crate) fn read_ai_settings(conn: &Connection) -> AppResult<StoredAiProviderSettings> {
    let state = read_ai_profiles(conn)?;
    if let Some(active) = active_profile(&state) {
        return Ok(profile_to_stored_settings(active));
    }

    Ok(StoredAiProviderSettings::defaults(AiProvider::Qwen))
}

pub(crate) fn write_ai_settings(
    conn: &Connection,
    settings: &StoredAiProviderSettings,
) -> AppResult<()> {
    let mut state = read_ai_profiles(conn)?;
    let active_index = state
        .profiles
        .iter()
        .position(|item| item.id == state.active_profile_id)
        .unwrap_or(0);
    let now = now_iso();

    if let Some(active) = state.profiles.get_mut(active_index) {
        active.provider = settings.provider.trim().to_string();
        active.model = settings.model.trim().to_string();
        active.base_url = settings.base_url.trim().trim_end_matches('/').to_string();
        active.api_key_enc = settings.api_key_enc.clone();
        active.temperature = settings.temperature;
        active.max_tokens = settings.max_tokens;
        active.timeout_secs = settings.timeout_secs;
        active.retry_count = settings.retry_count;
        active.updated_at = now;
        normalize_profile_in_place(active, active_index + 1);
    } else {
        let provider = AiProvider::from_db(&settings.provider);
        let name = profile_default_name(provider, 1);
        let profile = build_profile_from_settings(settings, name, now.clone());
        state.active_profile_id = profile.id.clone();
        state.profiles = vec![profile];
    }

    write_ai_profiles(conn, &state)
}

pub(crate) fn to_ai_profile_views(state: &StoredAiProviderProfiles) -> Vec<AiProviderProfileView> {
    state
        .profiles
        .iter()
        .map(|profile| {
            let provider = AiProvider::from_db(&profile.provider);
            AiProviderProfileView {
                id: profile.id.clone(),
                name: profile.name.clone(),
                provider: provider.as_db().to_string(),
                model: profile.model.clone(),
                base_url: profile.base_url.clone(),
                temperature: profile.temperature,
                max_tokens: profile.max_tokens,
                timeout_secs: profile.timeout_secs,
                retry_count: profile.retry_count,
                has_api_key: profile.api_key_enc.is_some()
                    || provider_specific_api_key(&provider).is_some(),
                is_active: profile.id == state.active_profile_id,
                created_at: profile.created_at.clone(),
                updated_at: profile.updated_at.clone(),
            }
        })
        .collect()
}

pub(crate) fn provider_specific_api_key(provider: &AiProvider) -> Option<String> {
    let key_names: &[&str] = match provider {
        AiProvider::Qwen => &["DOSS_QWEN_API_KEY"],
        AiProvider::Doubao => &["DOSS_DOUBAO_API_KEY"],
        AiProvider::Deepseek => &["DOSS_DEEPSEEK_API_KEY"],
        AiProvider::Minimax => &["DOSS_MINIMAX_API_KEY"],
        AiProvider::Glm => &["DOSS_GLM_API_KEY"],
        AiProvider::OpenApi => &[
            "DOSS_OPENAPI_API_KEY",
            "DOSS_OPENAI_COMPAT_API_KEY",
            "DOSS_OPENAI_API_KEY",
            "OPENAI_API_KEY",
        ],
    };

    for key_name in key_names {
        if let Ok(value) = std::env::var(key_name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}

pub(crate) fn resolve_ai_settings(
    conn: &Connection,
    cipher: &FieldCipher,
) -> AppResult<ResolvedAiProviderSettings> {
    let stored = read_ai_settings(conn)?;
    resolve_ai_settings_from_stored(&stored, cipher)
}

pub(crate) fn resolve_ai_settings_from_stored(
    stored: &StoredAiProviderSettings,
    cipher: &FieldCipher,
) -> AppResult<ResolvedAiProviderSettings> {
    let stored_provider_raw = stored.provider.trim().to_lowercase();
    let legacy_mock_provider = stored_provider_raw == "mock";

    let provider = std::env::var("DOSS_AI_PROVIDER")
        .ok()
        .map(|value| AiProvider::from_db(&value))
        .unwrap_or_else(|| AiProvider::from_db(&stored.provider));

    let decrypted_api_key = stored
        .api_key_enc
        .as_deref()
        .map(|value| cipher.decrypt(value))
        .transpose()?;

    let model = std::env::var("DOSS_AI_MODEL")
        .ok()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .unwrap_or_else(|| {
            let trimmed = stored.model.trim();
            if legacy_mock_provider || trimmed.is_empty() {
                provider.default_model().to_string()
            } else {
                trimmed.to_string()
            }
        });

    let base_url = std::env::var("DOSS_AI_BASE_URL")
        .ok()
        .map(|item| item.trim().trim_end_matches('/').to_string())
        .filter(|item| !item.is_empty())
        .unwrap_or_else(|| {
            let trimmed = stored.base_url.trim().trim_end_matches('/');
            if legacy_mock_provider || trimmed.is_empty() {
                provider.default_base_url().to_string()
            } else {
                trimmed.to_string()
            }
        });

    let api_key = std::env::var("DOSS_AI_API_KEY")
        .ok()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .or_else(|| provider_specific_api_key(&provider))
        .or(decrypted_api_key);

    let temperature = std::env::var("DOSS_AI_TEMPERATURE")
        .ok()
        .and_then(|item| item.parse::<f64>().ok())
        .unwrap_or(stored.temperature)
        .clamp(0.0, 1.2);
    let max_tokens = std::env::var("DOSS_AI_MAX_TOKENS")
        .ok()
        .and_then(|item| item.parse::<i32>().ok())
        .unwrap_or(stored.max_tokens)
        .clamp(200, 8192);
    let timeout_secs = std::env::var("DOSS_AI_TIMEOUT_SECS")
        .ok()
        .and_then(|item| item.parse::<i32>().ok())
        .unwrap_or(stored.timeout_secs)
        .clamp(8, 180);
    let retry_count = std::env::var("DOSS_AI_RETRY_COUNT")
        .ok()
        .and_then(|item| item.parse::<i32>().ok())
        .unwrap_or(stored.retry_count)
        .clamp(1, 5);

    Ok(ResolvedAiProviderSettings {
        provider,
        model,
        base_url,
        api_key,
        temperature,
        max_tokens,
        timeout_secs,
        retry_count,
    })
}

pub(crate) fn resolve_ai_settings_with_input_overrides(
    conn: &Connection,
    cipher: &FieldCipher,
    input: &UpsertAiProviderSettingsInput,
) -> AppResult<ResolvedAiProviderSettings> {
    let mut resolved = resolve_ai_settings(conn, cipher)?;
    let requested_provider = AiProvider::from_db(&input.provider);
    let provider_changed = requested_provider != resolved.provider;
    resolved.provider = requested_provider;

    if let Some(model) = input
        .model
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        resolved.model = model.to_string();
    } else if provider_changed || resolved.model.trim().is_empty() {
        resolved.model = resolved.provider.default_model().to_string();
    }

    if let Some(base_url) = input
        .base_url
        .as_deref()
        .map(|item| item.trim().trim_end_matches('/'))
        .filter(|item| !item.is_empty())
    {
        resolved.base_url = base_url.to_string();
    } else if provider_changed || resolved.base_url.trim().is_empty() {
        resolved.base_url = resolved.provider.default_base_url().to_string();
    }

    if let Some(api_key_raw) = input.api_key.as_deref() {
        let trimmed = api_key_raw.trim();
        resolved.api_key = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    } else if provider_changed {
        resolved.api_key = provider_specific_api_key(&resolved.provider);
    }

    resolved.temperature = input
        .temperature
        .unwrap_or(resolved.temperature)
        .clamp(0.0, 1.2);
    resolved.max_tokens = input
        .max_tokens
        .unwrap_or(resolved.max_tokens)
        .clamp(200, 8192);
    resolved.timeout_secs = input
        .timeout_secs
        .unwrap_or(resolved.timeout_secs)
        .clamp(8, 180);
    resolved.retry_count = input
        .retry_count
        .unwrap_or(resolved.retry_count)
        .clamp(1, 5);

    Ok(resolved)
}

pub(crate) fn resolve_ai_settings_for_profile(
    conn: &Connection,
    cipher: &FieldCipher,
    profile_id: &str,
) -> AppResult<ResolvedAiProviderSettings> {
    let state = read_ai_profiles(conn)?;
    let profile = state
        .profiles
        .iter()
        .find(|item| item.id == profile_id)
        .ok_or_else(|| AppError::NotFound(format!("AI profile {profile_id} not found")))?;
    let stored = profile_to_stored_settings(profile);
    resolve_ai_settings_from_stored(&stored, cipher)
}

pub(crate) fn to_ai_settings_view(settings: &ResolvedAiProviderSettings) -> AiProviderSettingsView {
    AiProviderSettingsView {
        provider: settings.provider.as_db().to_string(),
        model: settings.model.clone(),
        base_url: settings.base_url.clone(),
        temperature: settings.temperature,
        max_tokens: settings.max_tokens,
        timeout_secs: settings.timeout_secs,
        retry_count: settings.retry_count,
        has_api_key: settings.api_key.is_some(),
    }
}

pub(crate) fn trim_resume_excerpt(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

#[derive(Debug, Clone)]
pub(crate) struct TextGenerationAttachment {
    pub(crate) file_name: String,
    pub(crate) bytes: Vec<u8>,
    pub(crate) content_type: Option<String>,
}

impl TextGenerationAttachment {
    #[cfg(test)]
    pub(crate) fn from_text(file_name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            file_name: file_name.into(),
            bytes: content.into().into_bytes(),
            content_type: Some("text/plain; charset=utf-8".to_string()),
        }
    }

    pub(crate) fn from_bytes(
        file_name: impl Into<String>,
        bytes: Vec<u8>,
        content_type: Option<String>,
    ) -> Self {
        Self {
            file_name: file_name.into(),
            bytes,
            content_type: content_type
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum FileInputMode {
    OpenAiContentFile,
    QwenFileId,
}

fn normalize_upload_file_name(file_name: &str) -> String {
    let sanitized = file_name
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let normalized = sanitized.trim_matches('_');
    if normalized.is_empty() {
        return "resume.txt".to_string();
    }
    if normalized.contains('.') {
        normalized.to_string()
    } else {
        format!("{normalized}.txt")
    }
}

fn provider_file_input_mode(settings: &ResolvedAiProviderSettings) -> Option<FileInputMode> {
    let model = settings.model.trim().to_lowercase();
    match settings.provider {
        AiProvider::OpenApi => {
            let allowlist = [
                "gpt-5",
                "gpt-5-mini",
                "gpt-5-nano",
                "gpt-4.1",
                "gpt-4.1-mini",
                "gpt-4.1-nano",
                "o4-mini",
            ];
            if allowlist.iter().any(|item| model == *item) {
                Some(FileInputMode::OpenAiContentFile)
            } else {
                None
            }
        }
        AiProvider::Qwen => model
            .contains("qwen-long")
            .then_some(FileInputMode::QwenFileId)
            .or_else(|| model.contains("qwen-doc").then_some(FileInputMode::QwenFileId)),
        _ => None,
    }
}

pub(crate) fn model_supports_file_upload(settings: &ResolvedAiProviderSettings) -> bool {
    provider_file_input_mode(settings).is_some()
}

fn provider_file_upload_purpose(settings: &ResolvedAiProviderSettings) -> &'static str {
    match settings.provider {
        AiProvider::Qwen => "file-extract",
        _ => "user_data",
    }
}

fn ensure_openai_files_endpoint(base_url: &str) -> String {
    let normalized = base_url.trim().trim_end_matches('/');
    if normalized.ends_with("/files") {
        normalized.to_string()
    } else if normalized.ends_with("/chat/completions") {
        format!("{}/files", normalized.trim_end_matches("/chat/completions"))
    } else if normalized.ends_with("/text/chatcompletion_v2") {
        format!(
            "{}/files",
            normalized.trim_end_matches("/text/chatcompletion_v2")
        )
    } else {
        format!("{normalized}/files")
    }
}

fn upload_text_attachment_file(
    client: &Client,
    settings: &ResolvedAiProviderSettings,
    attachment: &TextGenerationAttachment,
) -> Result<String, String> {
    let api_key = settings
        .api_key
        .as_ref()
        .ok_or_else(|| "provider_api_key_missing".to_string())?;
    if attachment.bytes.is_empty() {
        return Err("provider_file_content_empty".to_string());
    }

    let endpoint = ensure_openai_files_endpoint(&settings.base_url);
    let purpose = provider_file_upload_purpose(settings);
    let file_name = normalize_upload_file_name(&attachment.file_name);
    let mut file_part = Part::bytes(attachment.bytes.clone()).file_name(file_name);
    if let Some(content_type) = attachment.content_type.as_deref() {
        file_part = file_part
            .mime_str(content_type)
            .map_err(|error| error.to_string())?;
    }
    let form = Form::new()
        .text("purpose", purpose.to_string())
        .part("file", file_part);

    let response = client
        .post(endpoint)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .map_err(|error| error.to_string())?;

    let status = response.status();
    let body_text = response.text().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "provider_file_upload_http_{}: {}",
            status.as_u16(),
            trim_resume_excerpt(&body_text, 300)
        ));
    }

    let body_json = serde_json::from_str::<Value>(&body_text).map_err(|error| error.to_string())?;
    let file_id = body_json
        .get("id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "provider_file_upload_missing_id".to_string())?;
    Ok(file_id.to_string())
}

pub(crate) fn read_resume_attachment(
    conn: &Connection,
    candidate_id: i64,
) -> Result<Option<TextGenerationAttachment>, String> {
    let row = conn
        .query_row(
            "SELECT file_name, content_type, content_blob FROM resume_files WHERE candidate_id = ?1",
            [candidate_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;

    let Some((file_name, content_type, bytes)) = row else {
        return Ok(None);
    };
    if bytes.is_empty() {
        return Ok(None);
    }

    Ok(Some(TextGenerationAttachment::from_bytes(
        file_name,
        bytes,
        content_type,
    )))
}

#[cfg(test)]
pub(crate) fn build_ai_prompts(context: &AiPromptContext) -> (String, String) {
    let system_prompt = r#"你是资深招聘顾问。请基于给定候选人资料，输出严格 JSON（不要 markdown），字段如下：
{
  "overall_score": 0-100 的整数,
  "dimension_scores": [
    {"key":"skill_match","score":0-100,"reason":"..."},
    {"key":"experience","score":0-100,"reason":"..."},
    {"key":"compensation","score":0-100,"reason":"..."},
    {"key":"stability","score":0-100,"reason":"..."}
  ],
  "risks": ["..."],
  "highlights": ["..."],
  "suggestions": ["..."],
  "evidence": [{"dimension":"...","statement":"...","source_snippet":"..."}],
  "confidence": 0-1
}
所有结论必须可解释，禁止编造不存在的信息。"#;

    let user_payload = serde_json::json!({
        "requiredSkills": context.required_skills,
        "candidateSkills": context.extracted_skills,
        "candidateYears": context.candidate_years,
        "expectedSalaryK": context.expected_salary_k,
        "jobMaxSalaryK": context.max_salary_k,
        "stage": context.stage,
        "tags": context.tags,
        "resumeParsed": context.resume_parsed,
        "resumeText": context.resume_raw_text,
        "resumeExcerpt": trim_resume_excerpt(&context.resume_raw_text, 2600),
    });

    (system_prompt.to_string(), user_payload.to_string())
}

pub(crate) fn ensure_openai_endpoint(base_url: &str) -> String {
    let normalized = base_url.trim().trim_end_matches('/');
    if normalized.ends_with("/chat/completions") {
        normalized.to_string()
    } else {
        format!("{normalized}/chat/completions")
    }
}

pub(crate) fn ensure_minimax_endpoint(base_url: &str) -> String {
    let normalized = base_url.trim().trim_end_matches('/');
    if normalized.ends_with("/text/chatcompletion_v2") {
        normalized.to_string()
    } else {
        format!("{normalized}/text/chatcompletion_v2")
    }
}

pub(crate) fn parse_openai_content(response: &Value) -> Option<String> {
    if let Some(content) = response
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_str())
    {
        return Some(content.to_string());
    }

    response
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("text").and_then(|value| value.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .filter(|value| !value.trim().is_empty())
}

pub(crate) fn parse_minimax_content(response: &Value) -> Result<String, String> {
    if let Some(status_code) = response
        .pointer("/base_resp/status_code")
        .and_then(|value| value.as_i64())
    {
        if status_code != 0 {
            let status_msg = response
                .pointer("/base_resp/status_msg")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("unknown_error");
            return Err(format!(
                "provider_api_error_{}: {}",
                status_code, status_msg
            ));
        }
    }

    if let Some(reply) = response
        .get("reply")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(reply.to_string());
    }

    if let Some(content) = parse_openai_content(response) {
        return Ok(content);
    }

    Err("provider_response_content_missing".to_string())
}

pub(crate) fn call_openai_compatible_provider(
    client: &Client,
    settings: &ResolvedAiProviderSettings,
    system_prompt: &str,
    user_prompt: &str,
    attachment: Option<&TextGenerationAttachment>,
) -> Result<String, String> {
    let endpoint = ensure_openai_endpoint(&settings.base_url);
    let api_key = settings
        .api_key
        .as_ref()
        .ok_or_else(|| "provider_api_key_missing".to_string())?;

    let fallback_messages = vec![
        serde_json::json!({"role": "system", "content": system_prompt}),
        serde_json::json!({"role": "user", "content": user_prompt}),
    ];

    let mut used_file_payload = false;
    let messages = if let Some(mode) = provider_file_input_mode(settings) {
        if let Some(file) = attachment.filter(|item| !item.bytes.is_empty()) {
            match upload_text_attachment_file(client, settings, file) {
                Ok(file_id) => {
                    used_file_payload = true;
                    match mode {
                        FileInputMode::OpenAiContentFile => vec![
                            serde_json::json!({"role": "system", "content": system_prompt}),
                            serde_json::json!({
                                "role": "user",
                                "content": [
                                    {
                                        "type": "file",
                                        "file": {
                                            "file_id": file_id
                                        }
                                    },
                                    {
                                        "type": "text",
                                        "text": user_prompt
                                    }
                                ]
                            }),
                        ],
                        FileInputMode::QwenFileId => vec![
                            serde_json::json!({"role": "system", "content": system_prompt}),
                            serde_json::json!({"role": "system", "content": format!("fileid://{file_id}")}),
                            serde_json::json!({"role": "user", "content": user_prompt}),
                        ],
                    }
                }
                Err(_) => fallback_messages.clone(),
            }
        } else {
            fallback_messages.clone()
        }
    } else {
        fallback_messages.clone()
    };

    let send_chat = |messages_payload: &[Value]| -> Result<String, String> {
        let response = client
            .post(&endpoint)
            .bearer_auth(api_key)
            .json(&serde_json::json!({
                "model": settings.model,
                "temperature": settings.temperature,
                "max_tokens": settings.max_tokens,
                "messages": messages_payload
            }))
            .send()
            .map_err(|error| error.to_string())?;

        let status = response.status();
        let body_text = response.text().map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!(
                "provider_http_{}: {}",
                status.as_u16(),
                trim_resume_excerpt(&body_text, 300)
            ));
        }
        let body_json =
            serde_json::from_str::<Value>(&body_text).map_err(|error| error.to_string())?;
        parse_openai_content(&body_json)
            .ok_or_else(|| "provider_response_content_missing".to_string())
    };

    match send_chat(&messages) {
        Ok(content) => Ok(content),
        Err(error) if used_file_payload => send_chat(&fallback_messages).or(Err(error)),
        Err(error) => Err(error),
    }
}

pub(crate) fn call_minimax_provider(
    client: &Client,
    settings: &ResolvedAiProviderSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, String> {
    let endpoint = ensure_minimax_endpoint(&settings.base_url);
    let api_key = settings
        .api_key
        .as_ref()
        .ok_or_else(|| "provider_api_key_missing".to_string())?;

    let response = client
        .post(endpoint)
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "model": settings.model,
            "temperature": settings.temperature,
            "max_tokens": settings.max_tokens,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ]
        }))
        .send()
        .map_err(|error| error.to_string())?;

    let status = response.status();
    let body_text = response.text().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "provider_http_{}: {}",
            status.as_u16(),
            trim_resume_excerpt(&body_text, 300)
        ));
    }

    let body_json = serde_json::from_str::<Value>(&body_text).map_err(|error| error.to_string())?;
    parse_minimax_content(&body_json).map_err(|error| {
        if error == "provider_response_content_missing" {
            format!("{error}: {}", trim_resume_excerpt(&body_text, 300))
        } else {
            error
        }
    })
}

pub(crate) fn probe_provider_connectivity(
    settings: ResolvedAiProviderSettings,
) -> Result<(Result<String, String>, u64), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(settings.timeout_secs as u64))
        .build()
        .map_err(|error| error.to_string())?;

    let start = Instant::now();
    let response = match settings.provider {
        AiProvider::Minimax => call_minimax_provider(
            &client,
            &settings,
            "You are a connectivity checker. Reply with exactly OK.",
            "ping",
        ),
        _ => call_openai_compatible_provider(
            &client,
            &settings,
            "You are a connectivity checker. Reply with exactly OK.",
            "ping",
            None,
        ),
    };
    let latency_ms = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
    Ok((response, latency_ms))
}

#[derive(Debug, Clone)]
#[cfg(test)]
pub(crate) enum AiInvokeProgressEvent {
    AttemptStart {
        attempt: i32,
        total: i32,
    },
    AttemptFailure {
        attempt: i32,
        total: i32,
        error: String,
    },
    Parsed {
        confidence: Option<f64>,
    },
}

#[cfg(test)]
pub(crate) fn invoke_cloud_provider(
    settings: &ResolvedAiProviderSettings,
    context: &AiPromptContext,
    fallback: &AiAnalysisPayload,
    attachment: Option<&TextGenerationAttachment>,
    mut on_progress: Option<&mut dyn FnMut(AiInvokeProgressEvent)>,
) -> Result<AiAnalysisPayload, String> {
    if settings.api_key.is_none() {
        return Err(format!("{}_api_key_missing", settings.provider.as_db()));
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(settings.timeout_secs as u64))
        .build()
        .map_err(|error| error.to_string())?;
    let (system_prompt, user_prompt) = build_ai_prompts(context);
    let fallback_attachment = TextGenerationAttachment::from_text(
        "candidate-resume.txt",
        context.resume_raw_text.clone(),
    );
    let resume_attachment = attachment.unwrap_or(&fallback_attachment);

    let attempts = settings.retry_count.max(1);
    let mut last_error = "provider_call_unknown_error".to_string();
    for attempt in 1..=attempts {
        if let Some(callback) = on_progress.as_mut() {
            (**callback)(AiInvokeProgressEvent::AttemptStart {
                attempt,
                total: attempts,
            });
        }

        let call_result = match settings.provider {
            AiProvider::Qwen
            | AiProvider::Doubao
            | AiProvider::Deepseek
            | AiProvider::Glm
            | AiProvider::OpenApi => call_openai_compatible_provider(
                &client,
                settings,
                &system_prompt,
                &user_prompt,
                Some(resume_attachment),
            ),
            AiProvider::Minimax => {
                call_minimax_provider(&client, settings, &system_prompt, &user_prompt)
            }
        };

        match call_result {
            Ok(content) => {
                let parsed = parse_ai_provider_response(&content)?;
                if let Some(callback) = on_progress.as_mut() {
                    (**callback)(AiInvokeProgressEvent::Parsed {
                        confidence: parsed.confidence,
                    });
                }
                return Ok(ensure_analysis_payload(parsed, fallback));
            }
            Err(error) => {
                if let Some(callback) = on_progress.as_mut() {
                    (**callback)(AiInvokeProgressEvent::AttemptFailure {
                        attempt,
                        total: attempts,
                        error: error.clone(),
                    });
                }
                last_error = error;
                if attempt < attempts {
                    std::thread::sleep(Duration::from_millis(280));
                }
            }
        }
    }

    Err(last_error)
}

pub(crate) fn invoke_text_generation(
    settings: &ResolvedAiProviderSettings,
    system_prompt: &str,
    user_prompt: &str,
    attachment: Option<&TextGenerationAttachment>,
) -> Result<String, String> {
    if settings.api_key.is_none() {
        return Err(format!("{}_api_key_missing", settings.provider.as_db()));
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(settings.timeout_secs as u64))
        .build()
        .map_err(|error| error.to_string())?;

    let attempts = settings.retry_count.max(1);
    let mut last_error = "provider_call_unknown_error".to_string();
    for attempt in 1..=attempts {
        let call_result = match settings.provider {
            AiProvider::Qwen
            | AiProvider::Doubao
            | AiProvider::Deepseek
            | AiProvider::Glm
            | AiProvider::OpenApi => call_openai_compatible_provider(
                &client,
                settings,
                system_prompt,
                user_prompt,
                attachment,
            ),
            AiProvider::Minimax => {
                call_minimax_provider(&client, settings, system_prompt, user_prompt)
            }
        };

        match call_result {
            Ok(content) => {
                let normalized = content.trim();
                if normalized.is_empty() {
                    last_error = "provider_response_content_missing".to_string();
                } else {
                    return Ok(normalized.to_string());
                }
            }
            Err(error) => {
                last_error = error;
            }
        }

        if attempt < attempts {
            std::thread::sleep(Duration::from_millis(280));
        }
    }

    Err(last_error)
}
