use super::super::*;

#[tauri::command]
pub(crate) fn get_ai_provider_catalog() -> Result<AiProviderCatalogView, String> {
    let providers = AiProvider::all()
        .iter()
        .map(AiProvider::to_catalog_item)
        .collect::<Vec<_>>();
    Ok(AiProviderCatalogView {
        providers,
        updated_at: now_iso(),
    })
}

#[tauri::command]
pub(crate) fn list_ai_provider_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<AiProviderProfileView>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let profiles = read_ai_profiles(&conn).map_err(|error| error.to_string())?;
    Ok(to_ai_profile_views(&profiles))
}

#[tauri::command]
pub(crate) fn upsert_ai_provider_profile(
    state: State<'_, AppState>,
    input: UpsertAiProviderProfileInput,
) -> Result<AiProviderProfileView, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut profiles_state = read_ai_profiles(&conn).map_err(|error| error.to_string())?;
    let requested_provider = AiProvider::from_db(&input.provider);
    let target_id = input.profile_id.clone().unwrap_or_else(make_ai_profile_id);
    let now = now_iso();

    let existing_index = profiles_state
        .profiles
        .iter()
        .position(|item| item.id == target_id);

    let mut profile = if let Some(index) = existing_index {
        profiles_state.profiles[index].clone()
    } else {
        let defaults = StoredAiProviderSettings::defaults(requested_provider);
        StoredAiProviderProfile {
            id: target_id.clone(),
            name: profile_default_name(requested_provider, profiles_state.profiles.len() + 1),
            provider: requested_provider.as_db().to_string(),
            model: defaults.model,
            base_url: defaults.base_url,
            api_key_enc: None,
            temperature: defaults.temperature,
            max_tokens: defaults.max_tokens,
            timeout_secs: defaults.timeout_secs,
            retry_count: defaults.retry_count,
            created_at: now.clone(),
            updated_at: now.clone(),
        }
    };

    let previous_provider = AiProvider::from_db(&profile.provider);
    let provider_changed = previous_provider != requested_provider;

    profile.provider = requested_provider.as_db().to_string();

    if let Some(name) = input.name.as_deref().map(str::trim).filter(|item| !item.is_empty()) {
        profile.name = name.to_string();
    } else if profile.name.trim().is_empty() {
        let ordinal = existing_index.unwrap_or(profiles_state.profiles.len()) + 1;
        profile.name = profile_default_name(requested_provider, ordinal);
    }

    profile.model = input
        .model
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.to_string())
        .unwrap_or_else(|| {
            if provider_changed || profile.model.trim().is_empty() {
                requested_provider.default_model().to_string()
            } else {
                profile.model.trim().to_string()
            }
        });

    profile.base_url = input
        .base_url
        .as_deref()
        .map(|item| item.trim().trim_end_matches('/'))
        .filter(|item| !item.is_empty())
        .map(|item| item.to_string())
        .unwrap_or_else(|| {
            if provider_changed || profile.base_url.trim().is_empty() {
                requested_provider.default_base_url().to_string()
            } else {
                profile.base_url.trim().trim_end_matches('/').to_string()
            }
        });

    if let Some(api_key_raw) = input.api_key {
        let trimmed = api_key_raw.trim();
        profile.api_key_enc = if trimmed.is_empty() {
            None
        } else {
            Some(
                state
                    .cipher
                    .encrypt(trimmed)
                    .map_err(|error| error.to_string())?,
            )
        };
    }

    profile.temperature = input
        .temperature
        .unwrap_or(profile.temperature)
        .clamp(0.0, 1.2);
    profile.max_tokens = input.max_tokens.unwrap_or(profile.max_tokens).clamp(200, 8192);
    profile.timeout_secs = input
        .timeout_secs
        .unwrap_or(profile.timeout_secs)
        .clamp(8, 180);
    profile.retry_count = input.retry_count.unwrap_or(profile.retry_count).clamp(1, 5);
    profile.updated_at = now;
    normalize_profile_in_place(
        &mut profile,
        existing_index.unwrap_or(profiles_state.profiles.len()) + 1,
    );

    if let Some(index) = existing_index {
        profiles_state.profiles[index] = profile.clone();
    } else {
        profiles_state.profiles.push(profile.clone());
    }
    profiles_state.active_profile_id = profile.id.clone();

    write_ai_profiles(&conn, &profiles_state).map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "ai.profile.upsert",
        "settings",
        Some(profile.id.clone()),
        serde_json::json!({
            "name": profile.name,
            "provider": profile.provider,
            "model": profile.model,
            "baseUrl": profile.base_url,
            "activeProfileId": profiles_state.active_profile_id,
        }),
    )
    .map_err(|error| error.to_string())?;

    to_ai_profile_views(&profiles_state)
        .into_iter()
        .find(|item| item.id == profile.id)
        .ok_or_else(|| "ai_profile_view_not_found".to_string())
}

#[tauri::command]
pub(crate) fn delete_ai_provider_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Vec<AiProviderProfileView>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut profiles_state = read_ai_profiles(&conn).map_err(|error| error.to_string())?;

    if profiles_state.profiles.len() <= 1 {
        return Err("at_least_one_ai_profile_required".to_string());
    }

    let index = profiles_state
        .profiles
        .iter()
        .position(|item| item.id == profile_id)
        .ok_or_else(|| "ai_profile_not_found".to_string())?;
    let removed = profiles_state.profiles.remove(index);

    if profiles_state.active_profile_id == profile_id {
        profiles_state.active_profile_id = profiles_state
            .profiles
            .first()
            .map(|item| item.id.clone())
            .unwrap_or_default();
    }

    write_ai_profiles(&conn, &profiles_state).map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "ai.profile.delete",
        "settings",
        Some(profile_id),
        serde_json::json!({
            "removedName": removed.name,
            "activeProfileId": profiles_state.active_profile_id,
            "remaining": profiles_state.profiles.len(),
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(to_ai_profile_views(&profiles_state))
}

#[tauri::command]
pub(crate) fn set_default_ai_provider_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Vec<AiProviderProfileView>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut profiles_state = read_ai_profiles(&conn).map_err(|error| error.to_string())?;

    let selected = profiles_state
        .profiles
        .iter()
        .find(|item| item.id == profile_id)
        .ok_or_else(|| "ai_profile_not_found".to_string())?;

    profiles_state.active_profile_id = profile_id.clone();
    write_ai_profiles(&conn, &profiles_state).map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "ai.profile.set_default",
        "settings",
        Some(profile_id),
        serde_json::json!({
            "activeProfileId": profiles_state.active_profile_id,
            "activeProfileName": selected.name,
            "activeProvider": selected.provider,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(to_ai_profile_views(&profiles_state))
}

#[tauri::command]
pub(crate) async fn test_ai_provider_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<AiProviderTestResult, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let settings = resolve_ai_settings_for_profile(&conn, &state.cipher, &profile_id)
        .map_err(|error| error.to_string())?;

    if settings.api_key.is_none() {
        return Err(format!("{}_api_key_missing", settings.provider.as_db()));
    }

    let endpoint = match settings.provider {
        AiProvider::Minimax => ensure_minimax_endpoint(&settings.base_url),
        _ => ensure_openai_endpoint(&settings.base_url),
    };

    let mut probe_settings = settings.clone();
    probe_settings.max_tokens = probe_settings.max_tokens.clamp(16, 256);
    let probe_settings_for_network = probe_settings.clone();
    let (response, latency_ms) = tauri::async_runtime::spawn_blocking(move || {
        probe_provider_connectivity(probe_settings_for_network)
    })
    .await
    .map_err(|error| error.to_string())??;

    match response {
        Ok(content) => {
            let reply_excerpt = trim_resume_excerpt(content.trim(), 120);
            let tested_at = now_iso();
            let _ = write_audit(
                &conn,
                "ai.profile.test",
                "settings",
                Some(profile_id),
                serde_json::json!({
                    "provider": probe_settings.provider.as_db(),
                    "model": probe_settings.model,
                    "endpoint": endpoint,
                    "latencyMs": latency_ms,
                    "ok": true,
                }),
            );
            Ok(AiProviderTestResult {
                ok: true,
                provider: probe_settings.provider.as_db().to_string(),
                model: probe_settings.model,
                endpoint,
                latency_ms,
                reply_excerpt,
                tested_at,
            })
        }
        Err(error) => {
            let _ = write_audit(
                &conn,
                "ai.profile.test",
                "settings",
                Some(profile_id),
                serde_json::json!({
                    "provider": probe_settings.provider.as_db(),
                    "model": probe_settings.model,
                    "endpoint": endpoint,
                    "latencyMs": latency_ms,
                    "ok": false,
                    "error": error,
                }),
            );
            Err(error)
        }
    }
}

#[tauri::command]
pub(crate) fn get_ai_provider_settings(
    state: State<'_, AppState>,
) -> Result<AiProviderSettingsView, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let settings = resolve_ai_settings(&conn, &state.cipher).map_err(|error| error.to_string())?;
    Ok(to_ai_settings_view(&settings))
}

#[tauri::command]
pub(crate) fn upsert_ai_provider_settings(
    state: State<'_, AppState>,
    input: UpsertAiProviderSettingsInput,
) -> Result<AiProviderSettingsView, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let provider = AiProvider::from_db(&input.provider);
    let mut stored = read_ai_settings(&conn).map_err(|error| error.to_string())?;
    let previous_provider = AiProvider::from_db(&stored.provider);
    let previous_provider_raw = stored.provider.trim().to_lowercase();
    let previous_is_legacy_mock = previous_provider_raw == "mock";
    let defaults = StoredAiProviderSettings::defaults(provider.clone());
    let api_key_changed = input.api_key.is_some();

    stored.provider = provider.as_db().to_string();
    stored.model = input
        .model
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .unwrap_or_else(|| {
            if previous_provider == provider && !previous_is_legacy_mock {
                let text = stored.model.trim();
                if text.is_empty() {
                    defaults.model.clone()
                } else {
                    text.to_string()
                }
            } else {
                defaults.model.clone()
            }
        });
    stored.base_url = input
        .base_url
        .map(|item| item.trim().trim_end_matches('/').to_string())
        .filter(|item| !item.is_empty())
        .unwrap_or_else(|| {
            if previous_provider == provider && !previous_is_legacy_mock {
                let text = stored.base_url.trim().trim_end_matches('/');
                if text.is_empty() {
                    defaults.base_url.clone()
                } else {
                    text.to_string()
                }
            } else {
                defaults.base_url.clone()
            }
        });
    stored.temperature = input
        .temperature
        .unwrap_or(stored.temperature)
        .clamp(0.0, 1.2);
    stored.max_tokens = input.max_tokens.unwrap_or(stored.max_tokens).clamp(200, 8192);
    stored.timeout_secs = input.timeout_secs.unwrap_or(stored.timeout_secs).clamp(8, 180);
    stored.retry_count = input.retry_count.unwrap_or(stored.retry_count).clamp(1, 5);

    if let Some(api_key_raw) = input.api_key {
        let trimmed = api_key_raw.trim();
        if trimmed.is_empty() {
            stored.api_key_enc = None;
        } else {
            stored.api_key_enc = Some(
                state
                    .cipher
                    .encrypt(trimmed)
                    .map_err(|error| error.to_string())?,
            );
        }
    }

    write_ai_settings(&conn, &stored).map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "ai.settings.update",
        "settings",
        Some(AI_SETTINGS_KEY.to_string()),
        serde_json::json!({
            "provider": stored.provider,
            "model": stored.model,
            "baseUrl": stored.base_url,
            "temperature": stored.temperature,
            "maxTokens": stored.max_tokens,
            "timeoutSecs": stored.timeout_secs,
            "retryCount": stored.retry_count,
            "apiKeyChanged": api_key_changed,
        }),
    )
    .map_err(|error| error.to_string())?;

    let resolved = resolve_ai_settings(&conn, &state.cipher).map_err(|error| error.to_string())?;
    Ok(to_ai_settings_view(&resolved))
}

#[tauri::command]
pub(crate) async fn test_ai_provider_settings(
    state: State<'_, AppState>,
    input: UpsertAiProviderSettingsInput,
) -> Result<AiProviderTestResult, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let settings = resolve_ai_settings_with_input_overrides(&conn, &state.cipher, &input)
        .map_err(|error| error.to_string())?;

    if settings.api_key.is_none() {
        return Err(format!("{}_api_key_missing", settings.provider.as_db()));
    }

    let endpoint = match settings.provider {
        AiProvider::Minimax => ensure_minimax_endpoint(&settings.base_url),
        _ => ensure_openai_endpoint(&settings.base_url),
    };

    let mut probe_settings = settings.clone();
    probe_settings.max_tokens = probe_settings.max_tokens.clamp(16, 256);
    let probe_settings_for_network = probe_settings.clone();
    let (response, latency_ms) = tauri::async_runtime::spawn_blocking(move || {
        probe_provider_connectivity(probe_settings_for_network)
    })
    .await
    .map_err(|error| error.to_string())??;

    match response {
        Ok(content) => {
            let reply_excerpt = trim_resume_excerpt(content.trim(), 120);
            let tested_at = now_iso();
            let _ = write_audit(
                &conn,
                "ai.settings.test",
                "settings",
                Some(AI_SETTINGS_KEY.to_string()),
                serde_json::json!({
                    "provider": probe_settings.provider.as_db(),
                    "model": probe_settings.model,
                    "endpoint": endpoint,
                    "latencyMs": latency_ms,
                    "ok": true,
                }),
            );
            Ok(AiProviderTestResult {
                ok: true,
                provider: probe_settings.provider.as_db().to_string(),
                model: probe_settings.model,
                endpoint,
                latency_ms,
                reply_excerpt,
                tested_at,
            })
        }
        Err(error) => {
            let _ = write_audit(
                &conn,
                "ai.settings.test",
                "settings",
                Some(AI_SETTINGS_KEY.to_string()),
                serde_json::json!({
                    "provider": probe_settings.provider.as_db(),
                    "model": probe_settings.model,
                    "endpoint": endpoint,
                    "latencyMs": latency_ms,
                    "ok": false,
                    "error": error,
                }),
            );
            Err(error)
        }
    }
}

#[tauri::command]
pub(crate) fn get_task_runtime_settings(
    state: State<'_, AppState>,
) -> Result<TaskRuntimeSettings, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    read_task_runtime_settings(&conn).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn upsert_task_runtime_settings(
    state: State<'_, AppState>,
    input: UpsertTaskRuntimeSettingsInput,
) -> Result<TaskRuntimeSettings, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut settings = read_task_runtime_settings(&conn).map_err(|error| error.to_string())?;

    settings.auto_batch_concurrency = input
        .auto_batch_concurrency
        .unwrap_or(settings.auto_batch_concurrency);
    settings.auto_retry_count = input.auto_retry_count.unwrap_or(settings.auto_retry_count);
    settings.auto_retry_backoff_ms = input
        .auto_retry_backoff_ms
        .unwrap_or(settings.auto_retry_backoff_ms);

    settings = normalize_task_runtime_settings(settings);
    write_task_runtime_settings(&conn, &settings).map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "task.settings.update",
        "settings",
        Some(TASK_RUNTIME_SETTINGS_KEY.to_string()),
        serde_json::json!({
            "autoBatchConcurrency": settings.auto_batch_concurrency,
            "autoRetryCount": settings.auto_retry_count,
            "autoRetryBackoffMs": settings.auto_retry_backoff_ms,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(settings)
}
