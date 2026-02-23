use crate::modules;
use tauri::AppHandle;

#[tauri::command]
pub async fn trigger_wakeup(
    account_id: String,
    model: String,
    prompt: Option<String>,
    max_output_tokens: Option<u32>,
) -> Result<modules::wakeup::WakeupResponse, String> {
    let final_prompt = prompt.unwrap_or_else(|| "hi".to_string());
    let final_tokens = max_output_tokens.unwrap_or(0);
    modules::wakeup::trigger_wakeup(&account_id, &model, &final_prompt, final_tokens).await
}

#[tauri::command]
pub async fn fetch_available_models() -> Result<Vec<modules::wakeup::AvailableModel>, String> {
    modules::wakeup::fetch_available_models().await
}

#[tauri::command]
pub async fn wakeup_sync_state(
    app: AppHandle,
    enabled: bool,
    tasks: Vec<modules::wakeup_scheduler::WakeupTaskInput>,
) -> Result<(), String> {
    modules::wakeup_scheduler::sync_state(enabled, tasks);
    modules::wakeup_scheduler::ensure_started(app);
    Ok(())
}

#[tauri::command]
pub fn wakeup_load_history() -> Result<Vec<modules::wakeup_history::WakeupHistoryItem>, String> {
    modules::wakeup_history::load_history()
}

#[tauri::command]
pub fn wakeup_clear_history() -> Result<(), String> {
    modules::wakeup_history::clear_history()
}
