// File: main.rs
// Location: /src/main.rs

use gtk4::prelude::*;
use libadwaita as adw;
use std::fs::OpenOptions;
use std::io::Write;
use chrono::Local;

mod window;
mod nm;
mod hotspot;
mod qr;
mod qr_dialog;
mod config;
mod ui;
mod secrets;
mod profiles;

use window::AdwNetworkWindow;

const APP_ID: &str = "com.github.adw-network";

fn setup_logging() {
    let log_path = std::env::var("HOME")
        .map(|home| {
            std::path::PathBuf::from(home)
                .join(".local/share/adw-network")
        })
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));
    
    let _ = std::fs::create_dir_all(&log_path);
    let log_file_path = log_path.join("adwaita-network.log");
    
    env_logger::Builder::from_default_env()
        .format(move |buf, record| {
            writeln!(
                buf,
                "[{}] [{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                record.args()
            )
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
        let _ = writeln!(file, "\n[{}] [INFO] ========== Adwaita Network Started ==========", now);
        let _ = writeln!(file, "[{}] [DEBUG] Log file: {:?}", Local::now().format("%Y-%m-%d %H:%M:%S%.3f"), log_file_path);
    }
}

fn main() -> glib::ExitCode {
    // Use the Flat-Remix theme for the original colorful look.
    setup_logging();
    log::info!("Application starting...");

    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    let _guard = rt.enter();

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    log::info!("Building UI...");
    let window = AdwNetworkWindow::new(app);
    window.present();
    log::info!("UI built and window presented");
}
