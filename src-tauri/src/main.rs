mod app_state;
mod commands;
mod config;
mod credentials;
mod error;
mod models;
mod platform;
mod ssh;
mod storage;

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
            commands::connection_exec,
            commands::host_key_check,
            commands::host_key_resolve,
            commands::credential_set,
            commands::credential_delete,
            commands::credential_exists,
        ])
        .run(tauri::generate_context!())
        .expect("error while running InfraDeck");
}
