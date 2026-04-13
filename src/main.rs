use chrono::Local;
use gtk4::prelude::*;
use libadwaita as adw;
use std::fs::OpenOptions;
use std::io::Write;

mod config;
mod hotspot;
mod hotspot_runtime;
mod leases;
mod modem_manager;
mod nm;
mod nm_dbus;
mod profiles;
mod qr;
mod qr_dialog;
mod secrets;
mod state;
mod ui;
mod window;

use window::AdwNetworkWindow;

const APP_ID: &str = "com.github.adw-network";

fn normalize_gsk_renderer_env() {
    if matches!(std::env::var("GSK_RENDERER").ok().as_deref(), Some("ngl")) {
        std::env::set_var("GSK_RENDERER", "gl");
    }
}

fn setup_logging() {
    let log_path = std::env::var("HOME")
        .map(|home| std::path::PathBuf::from(home).join(".local/share/adw-network"))
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));

    let _ = std::fs::create_dir_all(&log_path);
    let log_file_path = log_path.join("adwaita-network.log");
    let log_file_path_for_logger = log_file_path.clone();

    env_logger::Builder::from_default_env()
        .format(move |buf, record| {
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let line = format!("[{}] [{}] {}", timestamp, record.level(), record.args());

            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file_path_for_logger)
            {
                let _ = writeln!(file, "{}", line);
            }

            writeln!(buf, "{}", line)
        })
        .filter_level(log::LevelFilter::Debug)
        .try_init()
        .ok();

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file_path)
    {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let _ = writeln!(
            file,
            "\n[{}] [INFO] ========== Adwaita Network Started ==========",
            now
        );
        let _ = writeln!(
            file,
            "[{}] [DEBUG] Log file: {:?}",
            Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
            log_file_path
        );
        let _ = writeln!(
            file,
            "[{}] [INFO] Applying UI theme and layout improvements",
            Local::now().format("%Y-%m-%d %H:%M:%S%.3f")
        );
    }
}

fn main() -> glib::ExitCode {
    // * Normalize renderer quirks before GTK initializes.
    normalize_gsk_renderer_env();
    setup_logging();
    log::info!("Application starting...");

    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    let _guard = rt.enter();
    hotspot::spawn_runtime_daemon();

    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    log::info!("Building UI...");
    let window = AdwNetworkWindow::new(app);
    window.window.set_size_request(480, 640);
    window.present();
    log::info!("UI built and window presented");
}
