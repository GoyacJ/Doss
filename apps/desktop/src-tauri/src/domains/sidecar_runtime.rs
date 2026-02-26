use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tauri::State;

use crate::core::state::{AppState, SidecarManager, SidecarRuntime};

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SidecarCrawlJobsInput {
    source: String,
    mode: String,
    keyword: String,
    city: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SidecarCrawlCandidatesInput {
    source: String,
    mode: String,
    job_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SidecarCrawlResumeInput {
    source: String,
    mode: String,
    candidate_id: String,
}

pub(crate) fn sidecar_base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

pub(crate) fn sidecar_port_candidates(preferred_port: u16) -> Vec<u16> {
    let mut ports = vec![preferred_port];
    for offset in 1..=5_u16 {
        if let Some(port) = preferred_port.checked_add(offset) {
            ports.push(port);
        }
    }
    ports
}

fn sidecar_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn sidecar_health_ok(port: u16) -> bool {
    let endpoint = format!("{}/health", sidecar_base_url(port));
    let client = match Client::builder()
        .timeout(Duration::from_millis(850))
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };

    let response = match client.get(endpoint).send() {
        Ok(response) => response,
        Err(_) => return false,
    };
    if !response.status().is_success() {
        return false;
    }

    let payload = match response.json::<Value>() {
        Ok(payload) => payload,
        Err(_) => return false,
    };

    payload.get("ok").and_then(|value| value.as_bool()) == Some(true)
        && payload.get("service").and_then(|value| value.as_str()) == Some("crawler-sidecar")
}

fn sidecar_wait_until_healthy(port: u16, attempts: usize, delay_ms: u64) -> bool {
    for attempt in 0..attempts {
        if sidecar_health_ok(port) {
            return true;
        }

        if attempt + 1 < attempts {
            std::thread::sleep(Duration::from_millis(delay_ms));
        }
    }

    false
}

fn sidecar_spawn_process(manager: &SidecarManager, port: u16) -> Result<Child, String> {
    let mut command = if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(&manager.command);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.arg("-lc").arg(&manager.command);
        cmd
    };

    command
        .current_dir(&manager.cwd)
        .env("CRAWLER_PORT", port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    command.spawn().map_err(|error| error.to_string())
}

fn sidecar_stop_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub(crate) fn ensure_sidecar_running(state: &AppState) -> Result<SidecarRuntime, String> {
    let mut manager = state
        .sidecar
        .lock()
        .map_err(|_| "sidecar_lock_poisoned".to_string())?;

    let mut exited_recently = false;
    if let Some(child) = manager.child.as_mut() {
        match child.try_wait() {
            Ok(Some(status)) => {
                manager.last_error = Some(format!("sidecar_exit_status_{status}"));
                manager.child = None;
                exited_recently = true;
            }
            Ok(None) => {}
            Err(error) => {
                manager.last_error = Some(error.to_string());
                manager.child = None;
                exited_recently = true;
            }
        }
    }

    let mut probe_ports = vec![manager.active_port];
    for port in sidecar_port_candidates(manager.preferred_port) {
        if !probe_ports.contains(&port) {
            probe_ports.push(port);
        }
    }

    for port in probe_ports {
        if sidecar_health_ok(port) {
            manager.active_port = port;
            return Ok(SidecarRuntime {
                ok: true,
                port,
                base_url: sidecar_base_url(port),
                source: if exited_recently {
                    "recovered_existing".to_string()
                } else {
                    "existing".to_string()
                },
                message: manager.last_error.clone(),
                restart_count: manager.restart_count,
            });
        }
    }

    let mut any_port_available = false;
    let mut last_spawn_error = manager.last_error.clone().unwrap_or_default();
    let mut spawned_at_least_once = false;

    for port in sidecar_port_candidates(manager.preferred_port) {
        if !sidecar_port_available(port) {
            continue;
        }
        any_port_available = true;

        let mut child = match sidecar_spawn_process(&manager, port) {
            Ok(child) => child,
            Err(error) => {
                last_spawn_error = error;
                continue;
            }
        };
        spawned_at_least_once = true;

        if sidecar_wait_until_healthy(port, 9, 320) {
            manager.active_port = port;
            manager.restart_count = manager.restart_count.saturating_add(1);
            manager.last_error = None;
            manager.child = Some(child);

            return Ok(SidecarRuntime {
                ok: true,
                port,
                base_url: sidecar_base_url(port),
                source: if exited_recently {
                    "restarted"
                } else {
                    "spawned"
                }
                .to_string(),
                message: None,
                restart_count: manager.restart_count,
            });
        }

        sidecar_stop_child(&mut child);
    }

    if !any_port_available {
        manager.last_error = Some("sidecar_port_conflict".to_string());
        return Err("sidecar_port_conflict".to_string());
    }

    if spawned_at_least_once {
        manager.last_error = Some("sidecar_start_timeout".to_string());
        return Err("sidecar_start_timeout".to_string());
    }

    let error_text = if last_spawn_error.trim().is_empty() {
        "sidecar_start_failed".to_string()
    } else {
        last_spawn_error
    };
    manager.last_error = Some(error_text.clone());
    Err(error_text)
}

fn sidecar_post_json(state: &AppState, path: &str, payload: Value) -> Result<Value, String> {
    let runtime = ensure_sidecar_running(state)?;
    let endpoint = format!("{}{}", runtime.base_url, path);
    let client = Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|error| format!("sidecar_client_build_failed:{error}"))?;

    let response = client
        .post(endpoint)
        .json(&payload)
        .send()
        .map_err(|error| format!("sidecar_request_failed:{error}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        let body_text = body.trim();
        if body_text.is_empty() {
            return Err(format!("sidecar_http_{}", status.as_u16()));
        }
        return Err(format!("sidecar_http_{}:{body_text}", status.as_u16()));
    }

    response
        .json::<Value>()
        .map_err(|error| format!("sidecar_invalid_json:{error}"))
}

#[tauri::command]
pub(crate) fn ensure_sidecar(state: State<'_, AppState>) -> Result<SidecarRuntime, String> {
    ensure_sidecar_running(state.inner())
}

#[tauri::command]
pub(crate) fn sidecar_health(state: State<'_, AppState>) -> Result<Value, String> {
    ensure_sidecar_running(state.inner())?;
    Ok(serde_json::json!({
      "ok": true,
      "service": "crawler-sidecar"
    }))
}

#[tauri::command]
pub(crate) fn sidecar_crawl_jobs(
    state: State<'_, AppState>,
    input: SidecarCrawlJobsInput,
) -> Result<Value, String> {
    sidecar_post_json(
        state.inner(),
        "/v1/crawl/jobs",
        serde_json::json!({
          "source": input.source,
          "mode": input.mode,
          "params": {
            "keyword": input.keyword,
            "city": input.city
          }
        }),
    )
}

#[tauri::command]
pub(crate) fn sidecar_crawl_candidates(
    state: State<'_, AppState>,
    input: SidecarCrawlCandidatesInput,
) -> Result<Value, String> {
    sidecar_post_json(
        state.inner(),
        "/v1/crawl/candidates",
        serde_json::json!({
          "source": input.source,
          "mode": input.mode,
          "params": {
            "jobId": input.job_id
          }
        }),
    )
}

#[tauri::command]
pub(crate) fn sidecar_crawl_resume(
    state: State<'_, AppState>,
    input: SidecarCrawlResumeInput,
) -> Result<Value, String> {
    sidecar_post_json(
        state.inner(),
        "/v1/crawl/resume",
        serde_json::json!({
          "source": input.source,
          "mode": input.mode,
          "candidateId": input.candidate_id
        }),
    )
}
