use serde::Serialize;
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};

use crate::core::cipher::FieldCipher;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db_path: Arc<PathBuf>,
    pub(crate) cipher: Arc<FieldCipher>,
    pub(crate) sidecar: Arc<Mutex<SidecarManager>>,
}

impl AppState {
    pub(crate) fn new(
        db_path: PathBuf,
        seed: &str,
        sidecar_command: String,
        sidecar_cwd: PathBuf,
        preferred_sidecar_port: u16,
    ) -> Self {
        Self {
            db_path: Arc::new(db_path),
            cipher: Arc::new(FieldCipher::from_seed(seed)),
            sidecar: Arc::new(Mutex::new(SidecarManager {
                command: sidecar_command,
                cwd: sidecar_cwd,
                preferred_port: preferred_sidecar_port,
                active_port: preferred_sidecar_port,
                child: None,
                last_error: None,
                restart_count: 0,
            })),
        }
    }
}

#[derive(Debug)]
pub(crate) struct SidecarManager {
    pub(crate) command: String,
    pub(crate) cwd: PathBuf,
    pub(crate) preferred_port: u16,
    pub(crate) active_port: u16,
    pub(crate) child: Option<Child>,
    pub(crate) last_error: Option<String>,
    pub(crate) restart_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SidecarRuntime {
    pub(crate) ok: bool,
    pub(crate) port: u16,
    pub(crate) base_url: String,
    pub(crate) source: String,
    pub(crate) message: Option<String>,
    pub(crate) restart_count: u32,
}
