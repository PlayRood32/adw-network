// File: window.rs
// Location: /src/window.rs
//
// Credits & Inspirations:
// - GNOME Settings Network panel for UI/UX patterns
// - airctl for modern clean design

use gtk4::prelude::*;
use gtk4::glib;
use libadwaita::{self as adw, prelude::AdwDialogExt};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::config;
use crate::hotspot;
use crate::nm;
use crate::ui::{icon_name, wifi_page::WifiPage, hotspot_page::HotspotPage, devices_page::DevicesPage};

pub struct AppPrefs {
    pub _auto_scan: bool,
}

impl Default for AppPrefs {
    fn default() -> Self {
        Self {
            _auto_scan: true,
        }
    }
}

pub struct AdwNetworkWindow {
    pub window: adw::ApplicationWindow,
}

impl AdwNetworkWindow {
    pub fn new(app: &adw::Application) -> Self {
        Self::load_css();

        let prefs = Rc::new(RefCell::new(AppPrefs::default()));

        let wifi_page = WifiPage::new(prefs.clone());
        let hotspot_page = HotspotPage::new(prefs.clone());
        let devices_page = DevicesPage::new();

        let view_stack = adw::ViewStack::new();
        view_stack.connect_visible_child_notify(|stack| {
            if let Some(child) = stack.visible_child() {
                child.add_css_class("fade-in");
                let child_clone = child.clone();
                glib::timeout_add_local(Duration::from_millis(260), move || {
                    child_clone.remove_css_class("fade-in");
                    glib::ControlFlow::Break
                });
            }
        });
        let wifi_stack_page = view_stack.add_titled(&wifi_page.widget, Some("wifi"), "Wi-Fi");
        wifi_stack_page.set_icon_name(Some(icon_name(
            "network-wireless-symbolic",
            &["network-wireless-signal-excellent-symbolic", "network-wireless"],
        )));

        let hotspot_stack_page = view_stack.add_titled(&hotspot_page.widget, Some("hotspot"), "Hotspot");
        hotspot_stack_page.set_icon_name(Some(icon_name(
            "network-wireless-hotspot-symbolic",
            &["network-wireless-symbolic", "network-wireless"],
        )));

        let devices_stack_page = view_stack.add_titled(&devices_page.widget, Some("devices"), "Devices");
        devices_stack_page.set_icon_name(Some(icon_name(
            "computer-symbolic",
            &["network-workgroup-symbolic", "computer"],
        )));

        let view_switcher = adw::ViewSwitcher::builder()
            .stack(&view_stack)
            .policy(adw::ViewSwitcherPolicy::Wide)
            .build();

        // Global connection status header
        let status_icon = gtk4::Image::from_icon_name(icon_name(
            "network-wireless-symbolic",
            &["network-wireless-signal-excellent-symbolic", "network-wireless"],
        ));
        status_icon.set_pixel_size(14);

        let status_label = gtk4::Label::new(Some("Checking status…"));
        status_label.add_css_class("status-text");

        let status_pill = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        status_pill.add_css_class("status-pill");
        status_pill.append(&status_icon);
        status_pill.append(&status_label);
        status_pill.set_tooltip_text(Some("Connection status"));

        let title_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        title_box.add_css_class("header-title");
        status_pill.set_halign(gtk4::Align::Center);
        view_switcher.set_halign(gtk4::Align::Center);
        title_box.append(&status_pill);
        title_box.append(&view_switcher);

        let menu_button = gtk4::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Menu")
            .build();
        
        let menu = gio::Menu::new();
        menu.append(Some("About"), Some("app.about"));
        menu_button.set_menu_model(Some(&menu));

        let header = adw::HeaderBar::builder()
            .title_widget(&title_box)
            .build();

        // Hide system window buttons (minimize/maximize/close)
        header.set_decoration_layout(Some(""));

        // Add buttons to the header
        header.pack_end(&menu_button);

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_content(Some(&view_stack));

        // Periodically update the global connection status
        let status_icon = status_icon.clone();
        let status_label = status_label.clone();
        let status_pill = status_pill.clone();
        let update_status = move || {
            let status_icon = status_icon.clone();
            let status_label = status_label.clone();
            let status_pill = status_pill.clone();

            glib::spawn_future_local(async move {
                status_pill.remove_css_class("status-online");
                status_pill.remove_css_class("status-offline");
                status_pill.remove_css_class("status-hotspot");

                if hotspot::is_hotspot_active().await.unwrap_or(false) {
                    let ssid = config::load_config(&config::hotspot_config_path())
                        .ok()
                        .map(|c| c.ssid);
                    status_icon.set_icon_name(Some(icon_name(
                        "network-wireless-hotspot-symbolic",
                        &["network-wireless-symbolic", "network-wireless"],
                    )));
                    status_label.set_text("Hotspot active");
                    if let Some(ssid) = ssid {
                        status_pill.set_tooltip_text(Some(&format!("Hotspot: {}", ssid)));
                    } else {
                        status_pill.set_tooltip_text(Some("Hotspot active"));
                    }
                    status_pill.add_css_class("status-hotspot");
                    return;
                }

                match nm::get_active_wired_connection().await {
                    Ok(Some(conn_name)) => {
                        status_icon.set_icon_name(Some(icon_name(
                            "network-wired-symbolic",
                            &["network-wired", "network-transmit-receive-symbolic"],
                        )));
                        status_label.set_text("Connected (Wired)");
                        status_pill.set_tooltip_text(Some(&format!("Wired connection: {}", conn_name)));
                        status_pill.add_css_class("status-online");
                        return;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        log::warn!("Failed to update wired status: {}", e);
                    }
                }

                match nm::get_active_wifi_ssid().await {
                    Ok(Some(ssid)) => {
                        status_icon.set_icon_name(Some(icon_name(
                            "network-wireless-signal-excellent-symbolic",
                            &["network-wireless-symbolic", "network-wireless"],
                        )));
                        status_label.set_text(&ssid);
                        status_pill.set_tooltip_text(Some(&format!("Connected to {}", ssid)));
                        status_pill.add_css_class("status-online");
                    }
                    Ok(None) => {
                        let wifi_enabled = nm::is_wifi_enabled().await.unwrap_or(false);
                        if wifi_enabled {
                            status_icon.set_icon_name(Some(icon_name(
                                "network-wireless-offline-symbolic",
                                &["network-wireless-symbolic", "network-wireless"],
                            )));
                            status_label.set_text("Not connected");
                            status_pill.set_tooltip_text(Some("Not connected"));
                        } else {
                            status_icon.set_icon_name(Some(icon_name(
                                "network-wireless-disabled-symbolic",
                                &["network-wireless-offline-symbolic", "network-wireless"],
                            )));
                            status_label.set_text("Not connected");
                            status_pill.set_tooltip_text(Some("Wi-Fi off"));
                        }
                        status_pill.add_css_class("status-offline");
                    }
                    Err(e) => {
                        log::warn!("Failed to update connection status: {}", e);
                        status_label.set_text("Status unavailable");
                        status_pill.set_tooltip_text(Some("Status unavailable"));
                        status_pill.add_css_class("status-offline");
                    }
                }
            });

            glib::ControlFlow::Continue
        };
        update_status();
        glib::timeout_add_seconds_local(5, update_status);

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Adwaita Network")
            .default_width(820)
            .default_height(640)
            .content(&toolbar_view)
            .build();

        let about_action = gio::SimpleAction::new("about", None);
        let window_weak = window.downgrade();
        about_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                Self::show_about_dialog(&window);
            }
        });
        app.add_action(&about_action);

        Self { window }
    }

    fn show_about_dialog(window: &adw::ApplicationWindow) {
        let about = adw::AboutDialog::builder()
            .application_name("Adwaita Network")
            .application_icon("network-wireless-symbolic")
            .developer_name("PlayRood")
            .version("0.1.0")
            .comments("A modern network management application built with libadwaita")
            .website("https://github.com/PlayRood/adw-network")
            .license_type(gtk4::License::Gpl30)
            .build();

        about.present(Some(window));
    }

    fn load_css() {
        let provider = gtk4::CssProvider::new();
        
        let css = r#"
/* Modern network management design - Inspired by airctl and GNOME */

/* Network rows */
.network-row {
    padding: 8px;
    transition: background 200ms ease;
}

.network-row:hover {
    background: alpha(currentColor, 0.05);
}

/* List styling */
list,
.boxed-list {
    border: none;
    box-shadow: none;
}

.list-loading {
    opacity: 0.6;
}

/* Buttons */
button.flat.circular {
    min-width: 32px;
    min-height: 32px;
    padding: 0;
}

button.touch-target {
    min-height: 44px;
    padding: 8px 14px;
}

button.circular.touch-target {
    min-width: 44px;
    min-height: 44px;
    padding: 0;
}

/* Main window: custom close button (gray, light hover) */
button.window-close {
    background: alpha(@window_fg_color, 0.12);
    border: 1px solid alpha(@window_fg_color, 0.18);
    color: alpha(@window_fg_color, 0.85);
    transition: background 150ms ease, border-color 150ms ease, color 150ms ease;
}

button.window-close:hover {
    background: alpha(@window_fg_color, 0.2);
    border-color: alpha(@window_fg_color, 0.25);
}

button.window-close:active {
    background: alpha(@window_fg_color, 0.26);
    border-color: alpha(@window_fg_color, 0.3);
}

/* Hide dialog close buttons (only show close on main window) */
dialog headerbar button.close,
dialog headerbar windowcontrols,
dialog .titlebar button.close,
dialog .titlebar windowcontrols {
    min-width: 0;
    min-height: 0;
    padding: 0;
    margin: 0;
    border: 0;
    background: transparent;
    box-shadow: none;
    opacity: 0;
}


button.action-pill {
    padding: 8px 14px;
    border-radius: 999px;
    min-height: 36px;
}

button.action-pill image {
    margin-right: 6px;
}

button.qr-pill {
    padding: 8px 14px;
    min-height: 40px;
    font-weight: 600;
    font-size: 1.0em;
}

button.qr-pill image {
    margin-right: 6px;
}

button.action-pill.disconnect {
    background: @warning_bg_color;
    color: @warning_fg_color;
}

button.action-pill.forget {
    background: @error_bg_color;
    color: @error_fg_color;
}

button.action-pill.forget:hover {
    background: @error_color;
    color: @error_fg_color;
}

button.action-pill.forget image {
    color: @error_fg_color;
}

button.destructive-action {
    color: @error_color;
    font-weight: 700;
}

button.destructive-action:hover {
    background: alpha(@error_color, 0.32);
    color: @error_color;
}

button.destructive-action image,
button.destructive-action:hover image {
    color: @error_color;
    opacity: 1;
}

.forget-icon {
    color: @error_color;
    opacity: 1;
}

popover.menu button.destructive-action:hover {
    background: alpha(@error_color, 0.3);
}

/* Connected card */
.connected-card {
    background: alpha(@accent_bg_color, 0.18);
    border: 1px solid alpha(@accent_bg_color, 0.35);
    border-radius: 12px;
    padding: 16px;
    margin-bottom: 8px;
}

.connected-ssid {
    font-weight: 600;
    font-size: 1.1em;
}

.connected-subtitle {
    opacity: 0.8;
    font-size: 0.9em;
}

.detail-label {
    opacity: 0.8;
    font-size: 0.85em;
}

.detail-ip {
    font-weight: 600;
    opacity: 0.95;
    color: @accent_color;
    font-size: 0.9em;
}

.strength-label {
    opacity: 0.7;
    font-size: 0.85em;
}

progressbar.strength-weak trough {
    background: alpha(@error_color, 0.15);
}

progressbar.strength-weak progress {
    background: @error_color;
}

progressbar.strength-medium trough {
    background: alpha(@warning_color, 0.18);
}

progressbar.strength-medium progress {
    background: @warning_color;
}

progressbar.strength-strong trough {
    background: alpha(@success_color, 0.18);
}

progressbar.strength-strong progress {
    background: @success_color;
}

progressbar.strength-very-strong trough {
    background: alpha(@accent_color, 0.2);
}

progressbar.strength-very-strong progress {
    background: @accent_color;
}

.section-header {
    font-size: 0.8em;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    opacity: 0.7;
}

/* Signal strength colors */
.signal-excellent {
    color: @success_color;
}

.signal-good {
    color: @accent_color;
}

.signal-fair {
    color: @warning_color;
}

.signal-weak {
    color: @error_color;
}

/* Hotspot status */
.hotspot-active-header {
    color: @success_color;
    font-weight: 600;
}

.hotspot-icon {
    color: grey;
}

.hotspot-pulse {
    color: @success_color;
    animation: pulse 1.6s ease-in-out infinite;
}

/* Network item */
.network-item {
    padding: 12px 16px;
    border-radius: 8px;
    transition: background 150ms ease;
}

.network-item:hover {
    background: alpha(currentColor, 0.04);
}

/* Preferences groups */
.preferences-group {
    margin-top: 12px;
}

/* Search entry */
searchentry {
    min-height: 40px;
}

.big-spinner {
    min-width: 24px;
    min-height: 24px;
}

/* Filter buttons */
button.toggle {
    padding: 6px 12px;
    min-height: 30px;
    border-radius: 999px;
    font-size: 0.85em;
}

button.toggle:checked {
    background: @accent_bg_color;
    color: @accent_fg_color;
}

.filter-row {
    margin-top: 4px;
}

/* Connected indicator */
.connected-indicator {
    color: @success_color;
    font-weight: 600;
}

/* Focus handling */
*:focus {
    outline: none;
}

/* Cards */
.card {
    background: alpha(currentColor, 0.03);
    border-radius: 12px;
    border: none;
}

/* Menu styling */
popover.menu {
    padding: 0;
}

popover.menu box {
    margin: 0;
}

popover.menu button {
    padding: 8px 12px;
    border-radius: 0;
}

popover.menu button:first-child {
    border-radius: 6px 6px 0 0;
}

popover.menu button:last-child {
    border-radius: 0 0 6px 6px;
}

/* Network subtitle styling */
.network-subtitle {
    font-size: 0.85em;
    opacity: 0.7;
}

/* Signal indicator spacing */
.signal-indicator {
    margin-right: 8px;
}

/* Spinner animation */
@keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
}

.spinning {
    animation: spin 1s linear infinite;
}

@keyframes fadeIn {
    from { opacity: 0; transform: translateY(4px); }
    to { opacity: 1; transform: translateY(0); }
}

.fade-in {
    animation: fadeIn 220ms ease-out;
}

@keyframes pulse {
    0% { transform: scale(1); opacity: 0.8; }
    50% { transform: scale(1.06); opacity: 1; }
    100% { transform: scale(1); opacity: 0.8; }
}

/* Toast styling */
toast {
    border-radius: 8px;
}

/* Dialog spacing */
dialog box {
    padding: 0;
}

/* Action row improvements */
row {
    transition: background 200ms ease;
}

row:hover {
    background: alpha(currentColor, 0.03);
}

/* Improve password entry visibility */
entry.password {
    min-height: 40px;
}

/* Toggle switch improvements */
switch {
    min-width: 48px;
}

/* Toolbar view */
toolbarview {
    background: @window_bg_color;
}

/* View switcher */
viewswitcher button {
    padding: 6px 12px;
}

viewswitcher button:checked {
    background: alpha(@accent_bg_color, 0.25);
}

viewswitcher button:checked label {
    font-weight: 600;
    font-size: 1.02em;
}

/* Status page */
statuspage {
    padding: 32px;
}

statuspage.devices-empty image {
    -gtk-icon-size: 72px;
    opacity: 0.5;
    animation: pulse 1.8s ease-in-out infinite;
}

.status-pill {
    padding: 4px 10px;
    border-radius: 999px;
    background: alpha(@accent_bg_color, 0.12);
}

.header-title {
    padding-top: 2px;
    padding-bottom: 2px;
}

.status-text {
    font-size: 0.85em;
}

.status-pill.status-online {
    background: alpha(@success_color, 0.12);
    color: @success_color;
}

.status-pill.status-offline {
    background: alpha(@warning_color, 0.12);
    color: @warning_color;
}

.status-pill.status-hotspot {
    background: alpha(@success_color, 0.2);
    color: @success_color;
}
"#;

        provider.load_from_data(css);

        gtk4::style_context_add_provider_for_display(
            &gtk4::gdk::Display::default().expect("Could not connect to display"),
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    pub fn present(&self) {
        self.window.present();
    }
}
