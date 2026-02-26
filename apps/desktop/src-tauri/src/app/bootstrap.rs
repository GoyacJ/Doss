use base64::prelude::*;
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;

use crate::core::error::{AppError, AppResult};

pub(crate) fn resolve_db_path(app: &AppHandle) -> AppResult<PathBuf> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| AppError::NotFound("app_data_dir".to_string()))?;
    fs::create_dir_all(&data_dir)?;
    Ok(data_dir.join("doss.sqlite3"))
}

pub(crate) fn normalize_local_key(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|trimmed| !trimmed.is_empty())
}

pub(crate) fn generate_system_local_key() -> String {
    let bytes: [u8; 32] = rand::random();
    BASE64_STANDARD.encode(bytes)
}

pub(crate) fn resolve_local_key(app: &AppHandle, value: Option<String>) -> AppResult<String> {
    if let Some(key) = normalize_local_key(value) {
        return Ok(key);
    }

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| AppError::NotFound("app_data_dir".to_string()))?;
    fs::create_dir_all(&data_dir)?;
    let key_path = data_dir.join("doss.local.key");

    match fs::read_to_string(&key_path) {
        Ok(existing) => {
            if let Some(key) = normalize_local_key(Some(existing)) {
                return Ok(key);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(AppError::Io(error)),
    }

    let generated = generate_system_local_key();
    fs::write(&key_path, format!("{generated}\n"))?;
    Ok(generated)
}
