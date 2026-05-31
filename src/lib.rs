// * ./src/lib.rs

use chrono::Local;
use gtk4::prelude::*;
use libadwaita as adw;
use std::fs::OpenOptions;
use std::io::Write;

use crate::window::AdwNetworkWindow;

pub mod config;
pub mod hotspot;
pub mod hotspot_runtime;
pub mod leases;
pub mod modem_manager;
pub mod nm;
pub mod nm_dbus;
pub mod profiles;
pub mod qr;
pub mod qr_dialog;
pub mod secrets;
pub mod state;
mod ui;
mod window;

const APP_ID: &str = "com.github.adw-network";

fn normalize_gsk_renderer_env() {
    // * Critical for wlroots/Hyprland — ngl renderer crashes on some compositors
    if matches!(std::env::var("GSK_RENDERER").ok().as_deref(), Some("ngl")) {
        std::env::set_var("GSK_RENDERER", "gl");
        log::info!("GSK_RENDERER overridden: ngl → gl (wlroots compat)");
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

fn register_cleanup_signals() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("Failed to register SIGINT handler");

    tokio::spawn(async move {
        tokio::select! {
            _ = sigterm.recv() => {
                log::info!("Received SIGTERM — cleaning up hotspot rules");
                cleanup_hotspot_on_exit().await;
                gtk4::Application::default().quit();
            }
            _ = sigint.recv() => {
                log::info!("Received SIGINT — cleaning up hotspot rules");
                cleanup_hotspot_on_exit().await;
                gtk4::Application::default().quit();
            }
        }
    });
}

async fn cleanup_hotspot_on_exit() {
    if let Ok(Some(iface)) = crate::hotspot::get_hotspot_interface().await {
        log::info!("Cleaning up hotspot rules on interface: {}", iface);
        if let Err(e) = crate::hotspot::stop_hotspot().await {
            log::error!("Failed to cleanup hotspot on exit: {}", e);
        }
    }
}

pub fn run() -> glib::ExitCode {
    normalize_gsk_renderer_env();
    setup_logging();
    log::info!("Application starting...");

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            log::error!("Failed to create Tokio runtime: {}", e);
            return glib::ExitCode::FAILURE;
        }
    };
    let _guard = rt.enter();

    rt.block_on(async {
        if let Err(e) = nm::init_signal_listeners().await {
            log::warn!("Failed to initialize NM signal listeners (polling fallback active): {}", e);
        }
    });

    hotspot::spawn_runtime_daemon();

    register_cleanup_signals();

    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_activate(build_ui);
    let result = app.run();

    rt.shutdown_timeout(std::time::Duration::from_secs(5));

    result
}

fn build_ui(app: &adw::Application) {
    log::info!("Building UI...");
    let window = AdwNetworkWindow::new(app);
    window.window.set_size_request(480, 640);
    window.present();
    log::info!("UI built and window presented");
}
