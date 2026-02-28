use std::path::PathBuf;

use tauri::Manager;

mod app;
mod core;
mod domains;
mod infra;
mod models;

use app::bootstrap::{resolve_db_path, resolve_local_key};
use core::state::AppState;
use domains::ai_settings::{
    delete_ai_provider_profile, get_ai_provider_catalog, get_ai_provider_settings,
    get_task_runtime_settings, list_ai_provider_profiles, set_default_ai_provider_profile,
    test_ai_provider_profile, test_ai_provider_settings, upsert_ai_provider_profile,
    upsert_ai_provider_settings, upsert_task_runtime_settings,
};
use domains::candidate::{
    create_candidate, delete_candidate, list_analysis, list_candidates, list_candidates_page,
    list_decision_candidates_page, list_interview_candidates_page, list_pending_candidates,
    list_pipeline_events, merge_candidate_import, move_candidate_stage, parse_resume_file,
    set_candidate_qualification, sync_pending_candidate_to_candidate,
    update_candidate, upsert_pending_candidates, upsert_resume,
};
use domains::crawl_task::{
    create_crawl_task, delete_crawl_task, list_crawl_task_people, list_crawl_tasks,
    update_crawl_task, update_crawl_task_people_sync, upsert_crawl_task_people,
};
use domains::hiring::{finalize_hiring_decision, list_hiring_decisions};
use domains::interview::{
    generate_interview_kit, list_interview_evaluations, run_interview_evaluation,
    save_interview_kit, save_interview_recording, submit_interview_feedback,
};
use domains::jobs::{create_job, delete_job, list_jobs, stop_job, update_job};
use domains::scoring::{
    create_scoring_template, delete_scoring_template, get_scoring_template,
    list_scoring_results, list_scoring_templates, run_candidate_scoring,
    set_job_scoring_template, update_scoring_template, upsert_scoring_template,
};
use domains::search::search_candidates;
use domains::sidecar_runtime::{
    ensure_sidecar, ensure_sidecar_running, sidecar_crawl_candidates, sidecar_crawl_jobs,
    sidecar_crawl_resume, sidecar_health,
};
use domains::system::{app_health, dashboard_metrics};
use infra::db::migrate_db;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let db_path = resolve_db_path(app.handle())?;
            migrate_db(&db_path)?;

            let seed = resolve_local_key(app.handle(), std::env::var("DOSS_LOCAL_KEY").ok())?;

            let preferred_sidecar_port = std::env::var("CRAWLER_PORT")
                .ok()
                .and_then(|value| value.parse::<u16>().ok())
                .unwrap_or(3791);
            let sidecar_command = std::env::var("DOSS_SIDECAR_CMD")
                .unwrap_or_else(|_| "pnpm --filter @doss/crawler-sidecar dev".to_string());
            let sidecar_cwd = std::env::var("DOSS_SIDECAR_CWD")
                .ok()
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

            let state = AppState::new(
                db_path,
                &seed,
                sidecar_command,
                sidecar_cwd,
                preferred_sidecar_port,
            );

            let sidecar_autostart = std::env::var("DOSS_SIDECAR_AUTOSTART")
                .ok()
                .map(|value| value.trim().to_lowercase())
                .map(|value| !matches!(value.as_str(), "0" | "false" | "no"))
                .unwrap_or(true);

            if sidecar_autostart {
                let _ = ensure_sidecar_running(&state);
            }

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_health,
            ensure_sidecar,
            sidecar_health,
            sidecar_crawl_jobs,
            sidecar_crawl_candidates,
            sidecar_crawl_resume,
            create_job,
            update_job,
            stop_job,
            delete_job,
            list_jobs,
            create_candidate,
            update_candidate,
            delete_candidate,
            set_candidate_qualification,
            merge_candidate_import,
            list_candidates,
            list_candidates_page,
            list_interview_candidates_page,
            list_decision_candidates_page,
            move_candidate_stage,
            list_pipeline_events,
            upsert_resume,
            parse_resume_file,
            upsert_pending_candidates,
            list_pending_candidates,
            sync_pending_candidate_to_candidate,
            get_scoring_template,
            upsert_scoring_template,
            list_scoring_templates,
            create_scoring_template,
            update_scoring_template,
            delete_scoring_template,
            set_job_scoring_template,
            run_candidate_scoring,
            list_scoring_results,
            generate_interview_kit,
            save_interview_kit,
            save_interview_recording,
            submit_interview_feedback,
            run_interview_evaluation,
            list_interview_evaluations,
            finalize_hiring_decision,
            list_hiring_decisions,
            get_ai_provider_catalog,
            list_ai_provider_profiles,
            upsert_ai_provider_profile,
            delete_ai_provider_profile,
            set_default_ai_provider_profile,
            test_ai_provider_profile,
            get_ai_provider_settings,
            upsert_ai_provider_settings,
            test_ai_provider_settings,
            get_task_runtime_settings,
            upsert_task_runtime_settings,
            list_analysis,
            create_crawl_task,
            update_crawl_task,
            list_crawl_tasks,
            delete_crawl_task,
            upsert_crawl_task_people,
            list_crawl_task_people,
            update_crawl_task_people_sync,
            search_candidates,
            dashboard_metrics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests;
