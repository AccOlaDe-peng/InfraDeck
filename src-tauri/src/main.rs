mod ai;
mod app_state;
mod commands;
mod config;
mod credentials;
mod error;
mod fs;
mod models;
mod platform;
mod policy;
#[cfg(test)]
mod qa;
mod ssh;
mod storage;
mod tools;

use app_state::AppState;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "infradeck=info".into()))
        .with_target(true)
        .init();

    let state = AppState::new().expect("failed to initialize InfraDeck state");
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
            commands::server_profiles_list,
            commands::server_profile_save,
            commands::server_connect,
            commands::server_reconnect,
            commands::connection_disconnect,
            commands::terminal_open,
            commands::terminal_read,
            commands::terminal_write,
            commands::terminal_resize,
            commands::terminal_close,
            commands::connection_exec,
            commands::host_key_check,
            commands::host_key_resolve,
            commands::credential_set,
            commands::credential_delete,
            commands::credential_exists,
            commands::tool_definitions_list,
            commands::tool_execute,
            commands::batch_tool_execute,
            commands::approval_resolve,
            commands::audit_events_list,
            commands::audit_events_query,
            commands::app_settings_get,
            commands::app_settings_save,
            commands::ai::ai_provider_settings_get,
            commands::ai::ai_provider_settings_save,
            commands::ai::agent_send,
            commands::ai::agent_resume,
            commands::ai::agent_cancel,
            commands::ai::ai_conversations_list,
            commands::ai::ai_messages_list,
            commands::ai::ai_conversation_delete,
            commands::fs::fs_list,
            commands::fs::fs_stat,
            commands::fs::fs_mkdir,
            commands::fs::fs_rename,
            commands::fs::fs_delete,
            commands::fs::fs_transfer_start,
            commands::fs::fs_transfer_cancel,
            commands::fs::fs_transfers_list,
        ])
        .run(tauri::generate_context!())
        .expect("error while running InfraDeck");
}
