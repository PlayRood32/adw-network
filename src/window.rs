// File: window.rs
// Location: /src/window.rs
//
// Credits & Inspirations:
// - GNOME Settings Network panel for UI/UX patterns
// - airctl for modern clean design

use gtk4::prelude::*;
use gtk4::glib;
use libadwaita::{self as adw, prelude::*};
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::config;
use crate::hotspot;
use crate::nm;
use crate::ui::{
    devices_page::DevicesPage, ethernet_page::EthernetPage, hotspot_page::HotspotPage, icon_name,
    profiles_page::ProfilesPage, wifi_page::WifiPage,
};

pub struct AppPrefs {
    pub auto_scan: bool,
    pub expand_connected_details: bool,
    pub icons_only_navigation: bool,
}

impl Default for AppPrefs {
    fn default() -> Self {
        Self {
            auto_scan: true,
            expand_connected_details: false,
            icons_only_navigation: true,
        }
    }
}

pub struct AdwNetworkWindow {
    pub window: adw::ApplicationWindow,
}

impl AdwNetworkWindow {
    pub fn new(app: &adw::Application) -> Self {
        Self::load_css();
        Self::load_saved_theme();

        let app_settings = config::load_app_settings(&config::app_settings_path()).unwrap_or_default();
        let prefs = Rc::new(RefCell::new(AppPrefs {
            auto_scan: app_settings.auto_scan,
            expand_connected_details: app_settings.expand_connected_details,
            icons_only_navigation: app_settings.icons_only_navigation,
        }));

        let wifi_page = WifiPage::new(prefs.clone());
        let ethernet_page = EthernetPage::new();
        let hotspot_page = HotspotPage::new(prefs.clone());
        let devices_page = DevicesPage::new();
        let profiles_page = ProfilesPage::new();

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
            &["network-wireless-signal-excellent-symbolic", "network-wireless"][..],
        )));

        let ethernet_stack_page = view_stack.add_titled(&ethernet_page.widget, Some("ethernet"), "Ethernet");
        ethernet_stack_page.set_icon_name(Some(icon_name(
            "network-wired-symbolic",
            &["network-wired", "network-transmit-receive-symbolic"][..],
        )));

        let hotspot_stack_page = view_stack.add_titled(&hotspot_page.widget, Some("hotspot"), "Hotspot");
        hotspot_stack_page.set_icon_name(Some(icon_name(
            "network-wireless-hotspot-symbolic",
            &["network-wireless-symbolic", "network-wireless"][..],
        )));

        let devices_stack_page = view_stack.add_titled(&devices_page.widget, Some("devices"), "Devices");
        devices_stack_page.set_icon_name(Some(icon_name(
            "computer-symbolic",
            &["network-workgroup-symbolic", "computer"][..],
        )));

        let profiles_stack_page = view_stack.add_titled(&profiles_page.widget, Some("profiles"), "Profiles");
        profiles_stack_page.set_icon_name(Some(icon_name(
            "network-workgroup-symbolic",
            &["folder-symbolic", "applications-system-symbolic"][..],
        )));

        // Hide pages for unsupported hardware (refresh periodically)
        let wifi_page_ref = wifi_stack_page.clone();
        let hotspot_page_ref = hotspot_stack_page.clone();
        let ethernet_page_ref = ethernet_stack_page.clone();
        let devices_page_ref = devices_stack_page.clone();
        let profiles_page_ref = profiles_stack_page.clone();
        let view_stack_ref = view_stack.clone();
        let update_visibility = move || {
            let wifi_page_ref = wifi_page_ref.clone();
            let hotspot_page_ref = hotspot_page_ref.clone();
            let ethernet_page_ref = ethernet_page_ref.clone();
            let devices_page_ref = devices_page_ref.clone();
            let profiles_page_ref = profiles_page_ref.clone();
            let view_stack_ref = view_stack_ref.clone();

            glib::spawn_future_local(async move {
                match nm::has_wifi_device().await {
                    Ok(true) => {
                        wifi_page_ref.set_visible(true);
                        hotspot_page_ref.set_visible(true);
                        devices_page_ref.set_visible(true);
                    }
                    Ok(false) => {
                        wifi_page_ref.set_visible(false);
                        hotspot_page_ref.set_visible(false);
                        devices_page_ref.set_visible(false);
                    }
                    Err(e) => {
                        log::warn!("Failed to detect Wi-Fi device: {}", e);
                    }
                }

                match nm::has_ethernet_device().await {
                    Ok(true) => ethernet_page_ref.set_visible(true),
                    Ok(false) => ethernet_page_ref.set_visible(false),
                    Err(e) => log::warn!("Failed to detect ethernet device: {}", e),
                }

                // Ensure the currently selected page is visible
                let current_visible = view_stack_ref
                    .visible_child()
                    .map(|w| view_stack_ref.page(&w).is_visible())
                    .unwrap_or(false);

                if !current_visible {
                    if ethernet_page_ref.is_visible() {
                        let child = ethernet_page_ref.child();
                        view_stack_ref.set_visible_child(&child);
                    } else if wifi_page_ref.is_visible() {
                        let child = wifi_page_ref.child();
                        view_stack_ref.set_visible_child(&child);
                    } else if hotspot_page_ref.is_visible() {
                        let child = hotspot_page_ref.child();
                        view_stack_ref.set_visible_child(&child);
                    } else if devices_page_ref.is_visible() {
                        let child = devices_page_ref.child();
                        view_stack_ref.set_visible_child(&child);
                    } else if profiles_page_ref.is_visible() {
                        let child = profiles_page_ref.child();
                        view_stack_ref.set_visible_child(&child);
                    }
                }
            });
        };

        update_visibility();
        glib::timeout_add_seconds_local(3, move || {
            update_visibility();
            glib::ControlFlow::Continue
        });

        let view_switcher = adw::ViewSwitcher::builder()
            .stack(&view_stack)
            .policy(adw::ViewSwitcherPolicy::Wide)
            .build();
        Self::apply_navigation_mode(
            &wifi_stack_page,
            &ethernet_stack_page,
            &hotspot_stack_page,
            &devices_stack_page,
            &profiles_stack_page,
            &view_switcher,
            app_settings.icons_only_navigation,
        );

        // Global connection status header
        let status_icon = gtk4::Image::from_icon_name(icon_name(
            "network-wireless-symbolic",
            &["network-wireless-signal-excellent-symbolic", "network-wireless"][..],
        ));
        status_icon.set_pixel_size(14);

        let status_label = gtk4::Label::new(Some("Checking status…"));
        status_label.add_css_class("status-text");

        let status_pill = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        status_pill.add_css_class("status-pill");
        status_pill.append(&status_icon);
        status_pill.append(&status_label);
        status_pill.set_tooltip_text(Some("Connection status"));

        let speed_down_label = gtk4::Label::new(Some("↓ 0 KB/s"));
        speed_down_label.add_css_class("status-speed-text");
        let speed_up_label = gtk4::Label::new(Some("↑ 0 KB/s"));
        speed_up_label.add_css_class("status-speed-text");
        let speed_sep_label = gtk4::Label::new(Some("|"));
        speed_sep_label.add_css_class("status-speed-sep");

        let speed_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        speed_box.add_css_class("status-speed-box");
        speed_box.append(&speed_down_label);
        speed_box.append(&speed_sep_label);
        speed_box.append(&speed_up_label);
        speed_box.set_halign(gtk4::Align::Center);

        let title_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        title_box.add_css_class("header-title");
        title_box.set_halign(gtk4::Align::Center);
        status_pill.set_halign(gtk4::Align::Center);
        speed_box.set_halign(gtk4::Align::Center);
        view_switcher.set_hexpand(false);
        view_switcher.set_halign(gtk4::Align::Center);
        title_box.append(&status_pill);
        title_box.append(&speed_box);
        title_box.append(&view_switcher);

        let menu_button = gtk4::MenuButton::builder()
            .icon_name("emblem-system-symbolic")
            .tooltip_text("Settings & About")
            .build();
        menu_button.add_css_class("menu-button");
        menu_button.add_css_class("header-mini-button");
        menu_button.set_size_request(16, 16);
        menu_button.set_valign(gtk4::Align::Center);
        
        let menu = gio::Menu::new();
        menu.append(Some("Settings"), Some("app.settings"));
        menu.append(Some("About"), Some("app.about"));
        menu_button.set_menu_model(Some(&menu));

        let header = adw::HeaderBar::builder()
            .title_widget(&title_box)
            .build();
        header.set_centering_policy(adw::CenteringPolicy::Strict);

        // Restore default window close control.
        header.set_decoration_layout(Some(":close"));

        // Keep settings near the close button on the right.
        let right_controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        right_controls.append(&menu_button);

        // Add buttons to the header
        header.pack_end(&right_controls);

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
                        &["network-wireless-symbolic", "network-wireless"][..],
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
                            &["network-wired", "network-transmit-receive-symbolic"][..],
                        )));
                        status_label.set_text("Connected (Wired)");
                        status_pill.set_tooltip_text(Some(&format!("Wired connection: {}", conn_name)));
                        status_pill.add_css_class("status-online");
                        return;
                    }
                    Ok(std::prelude::v1::None) => {}
                    Err(e) => {
                        log::warn!("Failed to update wired status: {}", e);
                    }
                }

                match nm::get_active_wifi_ssid().await {
                    Ok(Some(ssid)) => {
                        status_icon.set_icon_name(Some(icon_name(
                            "network-wireless-signal-excellent-symbolic",
                            &["network-wireless-symbolic", "network-wireless"][..],
                        )));
                        status_label.set_text(&ssid);
                        status_pill.set_tooltip_text(Some(&format!("Connected to {}", ssid)));
                        status_pill.add_css_class("status-online");
                    }
                    Ok(std::prelude::v1::None) => {
                        let wifi_enabled = nm::is_wifi_enabled().await.unwrap_or(false);
                        if wifi_enabled {
                            status_icon.set_icon_name(Some(icon_name(
                                "network-wireless-offline-symbolic",
                                &["network-wireless-symbolic", "network-wireless"][..],
                            )));
                            status_label.set_text("Not connected");
                            status_pill.set_tooltip_text(Some("Not connected"));
                        } else {
                            status_icon.set_icon_name(Some(icon_name(
                                "network-wireless-disabled-symbolic",
                                &["network-wireless-offline-symbolic", "network-wireless"][..],
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

        let speed_state = Arc::new(Mutex::new((0u64, 0u64)));
        let speed_state_ui = Arc::clone(&speed_state);
        let speed_down_label = speed_down_label.clone();
        let speed_up_label = speed_up_label.clone();
        glib::timeout_add_seconds_local(1, move || {
            let (down_bytes, up_bytes) = speed_state_ui
                .lock()
                .map(|v| *v)
                .unwrap_or((0, 0));
            speed_down_label.set_text(&format!("↓ {}", format_speed(down_bytes)));
            speed_up_label.set_text(&format!("↑ {}", format_speed(up_bytes)));
            glib::ControlFlow::Continue
        });

        let speed_state_task = Arc::clone(&speed_state);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            let mut last_iface: Option<String> = None;
            let mut last_rx: Option<u64> = None;
            let mut last_tx: Option<u64> = None;

            loop {
                interval.tick().await;

                let iface = match nm::get_primary_connected_device().await {
                    Ok(Some(dev)) => dev,
                    _ => {
                        last_iface = None;
                        last_rx = None;
                        last_tx = None;
                        if let Ok(mut state) = speed_state_task.lock() {
                            *state = (0, 0);
                        }
                        continue;
                    }
                };

                if last_iface.as_deref() != Some(&iface) {
                    last_iface = Some(iface.clone());
                    last_rx = None;
                    last_tx = None;
                }

                let Some((rx, tx)) = read_interface_bytes(&iface) else {
                    last_rx = None;
                    last_tx = None;
                    if let Ok(mut state) = speed_state_task.lock() {
                        *state = (0, 0);
                    }
                    continue;
                };

                let down = if let Some(prev_rx) = last_rx {
                    rx.saturating_sub(prev_rx)
                } else {
                    0
                };
                let up = if let Some(prev_tx) = last_tx {
                    tx.saturating_sub(prev_tx)
                } else {
                    0
                };

                last_rx = Some(rx);
                last_tx = Some(tx);
                if let Ok(mut state) = speed_state_task.lock() {
                    *state = (down, up);
                }
            }
        });

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Adwaita Network")
            .default_width(700)
            .default_height(520)
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

        let settings_action = gio::SimpleAction::new("settings", None);
        let window_weak = window.downgrade();
        let prefs_for_settings = prefs.clone();
        let wifi_for_settings = wifi_page.clone_ref();
        let wifi_page_for_settings = wifi_stack_page.clone();
        let ethernet_page_for_settings = ethernet_stack_page.clone();
        let hotspot_page_for_settings = hotspot_stack_page.clone();
        let devices_page_for_settings = devices_stack_page.clone();
        let profiles_page_for_settings = profiles_stack_page.clone();
        let view_switcher_for_settings = view_switcher.clone();
        settings_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                Self::show_settings_window(
                    &window,
                    prefs_for_settings.clone(),
                    wifi_for_settings.clone_ref(),
                    wifi_page_for_settings.clone(),
                    ethernet_page_for_settings.clone(),
                    hotspot_page_for_settings.clone(),
                    devices_page_for_settings.clone(),
                    profiles_page_for_settings.clone(),
                    view_switcher_for_settings.clone(),
                );
            }
        });
        app.add_action(&settings_action);

        Self { window }
    }

    fn show_about_dialog(window: &adw::ApplicationWindow) {
        let about = adw::AboutDialog::builder()
            .application_name("Adwaita Network")
            .application_icon("icon")
            .developer_name("PlayRood")
            .version("0.1.5")
            .comments("A modern network management application built with libadwaita")
            .website("https://github.com/PlayRood/adw-network")
            .license_type(gtk4::License::Gpl30)
            .build();

        about.present(Some(window));
    }

    fn show_settings_window(
        window: &adw::ApplicationWindow,
        prefs: Rc<RefCell<AppPrefs>>,
        wifi_page: WifiPage,
        wifi_stack_page: adw::ViewStackPage,
        ethernet_stack_page: adw::ViewStackPage,
        hotspot_stack_page: adw::ViewStackPage,
        devices_stack_page: adw::ViewStackPage,
        profiles_stack_page: adw::ViewStackPage,
        view_switcher: adw::ViewSwitcher,
    ) {
        let style_manager = adw::StyleManager::default();
        let settings_state = Rc::new(RefCell::new(
            config::load_app_settings(&config::app_settings_path()).unwrap_or_default(),
        )); 

        let theme_model = gtk4::StringList::new(&["System", "Light", "Dark"][..]);
        let theme_combo = adw::ComboRow::builder()
            .title("Appearance")
            .subtitle("Follow system style or force light/dark")
            .model(&theme_model)
            .build();

        let selected = match style_manager.color_scheme() {
            adw::ColorScheme::ForceLight => 1,
            adw::ColorScheme::ForceDark => 2,
            _ => 0,
        };
        theme_combo.set_selected(selected);

        let style_manager_for_theme = style_manager.clone();
        let settings_state_for_theme = settings_state.clone();
        theme_combo.connect_selected_notify(move |row: &adw::ComboRow| {
            let scheme = Self::color_scheme_from_selection(row.selected());
            style_manager_for_theme.set_color_scheme(scheme);

            let mut settings = settings_state_for_theme.borrow_mut();
            settings.color_scheme = Self::setting_from_selection(row.selected()).to_string();
            if let Err(e) = config::save_app_settings(&config::app_settings_path(), &*settings) {
                log::warn!("Failed to save app settings: {}", e);
            }
        });

        let group = adw::PreferencesGroup::new();
        group.set_title("General");
        group.set_description(Some("Core app display and behavior settings."));
        group.add(&theme_combo);

        let storage_group = adw::PreferencesGroup::new();
        storage_group.set_title("Security");
        storage_group.set_description(Some("How hotspot credentials are stored."));

        let storage_model = gtk4::StringList::new(&[
            "System Keyring (Recommended)",
            "NetworkManager keyring",
            "Plain JSON (Legacy)",
        ][..]);
        let storage_row = adw::ComboRow::builder()
            .title("Hotspot password storage")
            .subtitle("Choose the credential backend for hotspot passwords")
            .model(&storage_model)
            .build();
        storage_row.set_selected(Self::selection_from_password_storage(
            &settings_state.borrow().hotspot_password_storage,
        ));

        let settings_state_for_storage = settings_state.clone();
        storage_row.connect_selected_notify(move |row| {
            let mut settings = settings_state_for_storage.borrow_mut();
            settings.hotspot_password_storage =
                Self::password_storage_from_selection(row.selected());
            if let Err(e) = config::save_app_settings(&config::app_settings_path(), &*settings) {
                log::warn!("Failed to save app settings: {}", e);
            }
        });

        storage_group.add(&storage_row);

        let settings_state_for_switches = settings_state.clone();
        let auto_scan_row = adw::SwitchRow::builder()
            .title("Auto refresh networks")
            .subtitle("Rescan nearby networks every 10 seconds")
            .active(settings_state_for_switches.borrow().auto_scan)
            .build();

        let settings_state_for_switches = settings_state.clone();
        let expand_details_row = adw::SwitchRow::builder()
            .title("Always show connection details")
            .subtitle("Keep the connected network card expanded")
            .active(settings_state_for_switches.borrow().expand_connected_details)
            .build();

        let settings_state_for_switches = settings_state.clone();
        let nav_icons_only_row = adw::SwitchRow::builder()
            .title("Top navigation icons only")
            .subtitle("Hide tab text labels and keep icons only")
            .active(settings_state_for_switches.borrow().icons_only_navigation)
            .build();

        let prefs_for_auto_scan = prefs.clone();
        let settings_state_for_auto_scan = settings_state.clone();
        auto_scan_row.connect_active_notify(move |row| {
            let active = row.is_active();
            prefs_for_auto_scan.borrow_mut().auto_scan = active;
            let mut settings = settings_state_for_auto_scan.borrow_mut();
            settings.auto_scan = active;
            if let Err(e) = config::save_app_settings(&config::app_settings_path(), &*settings) {
                log::warn!("Failed to save app settings: {}", e);
            }
        });

        let prefs_for_expand = prefs.clone();
        let settings_state_for_expand = settings_state.clone();
        let wifi_for_expand = wifi_page.clone_ref();
        expand_details_row.connect_active_notify(move |row| {
            let active = row.is_active();
            prefs_for_expand.borrow_mut().expand_connected_details = active;
            wifi_for_expand.apply_expand_details_setting(active);
            let mut settings = settings_state_for_expand.borrow_mut();
            settings.expand_connected_details = active;
            if let Err(e) = config::save_app_settings(&config::app_settings_path(), &*settings) {
                log::warn!("Failed to save app settings: {}", e);
            }
        });

        let prefs_for_nav_mode = prefs.clone();
        let settings_state_for_nav_mode = settings_state.clone();
        let wifi_stack_page_for_nav = wifi_stack_page.clone();
        let ethernet_stack_page_for_nav = ethernet_stack_page.clone();
        let hotspot_stack_page_for_nav = hotspot_stack_page.clone();
        let devices_stack_page_for_nav = devices_stack_page.clone();
        let profiles_stack_page_for_nav = profiles_stack_page.clone();
        let view_switcher_for_nav = view_switcher.clone();
        nav_icons_only_row.connect_active_notify(move |row| {
            let active = row.is_active();
            prefs_for_nav_mode.borrow_mut().icons_only_navigation = active;
            Self::apply_navigation_mode(
                &wifi_stack_page_for_nav,
                &ethernet_stack_page_for_nav,
                &hotspot_stack_page_for_nav,
                &devices_stack_page_for_nav,
                &profiles_stack_page_for_nav,
                &view_switcher_for_nav,
                active,
            );
            let mut settings = settings_state_for_nav_mode.borrow_mut();
            settings.icons_only_navigation = active;
            if let Err(e) = config::save_app_settings(&config::app_settings_path(), &*settings) {
                log::warn!("Failed to save app settings: {}", e);
            }
        });

        let personalization_group = adw::PreferencesGroup::new();
        personalization_group.set_title("Behavior");
        personalization_group.set_description(Some("Interaction and navigation preferences."));
        personalization_group.add(&auto_scan_row);
        personalization_group.add(&expand_details_row);
        personalization_group.add(&nav_icons_only_row);

        let reset_button = gtk4::Button::builder()
            .label("Reset to defaults")
            .css_classes(vec!["destructive-action".to_string()])
            .build();

        let reset_row = adw::ActionRow::builder()
            .title("Reset settings")
            .subtitle("Clear all changes and restore defaults")
            .build();
        reset_row.add_suffix(&reset_button);
        reset_row.set_activatable_widget(Some(&reset_button));

        let reset_group = adw::PreferencesGroup::new();
        reset_group.set_title("Reset");
        reset_group.add(&reset_row);

        let settings_state_for_reset = settings_state.clone();
        let prefs_for_reset = prefs.clone();
        let wifi_for_reset = wifi_page.clone_ref();
        let theme_combo_for_reset = theme_combo.clone();
        let storage_row_for_reset = storage_row.clone();
        let auto_scan_for_reset = auto_scan_row.clone();
        let expand_details_for_reset = expand_details_row.clone();
        let nav_icons_only_for_reset = nav_icons_only_row.clone();
        let style_manager_for_reset = style_manager.clone();
        let wifi_stack_page_for_reset = wifi_stack_page.clone();
        let ethernet_stack_page_for_reset = ethernet_stack_page.clone();
        let hotspot_stack_page_for_reset = hotspot_stack_page.clone();
        let devices_stack_page_for_reset = devices_stack_page.clone();
        let profiles_stack_page_for_reset = profiles_stack_page.clone();
        let view_switcher_for_reset = view_switcher.clone();
        reset_button.connect_clicked(move |_| {
            let defaults = config::AppSettings::default();
            if let Err(e) = config::save_app_settings(&config::app_settings_path(), &defaults) {
                log::warn!("Failed to save app settings: {}", e);
            }

            *settings_state_for_reset.borrow_mut() = defaults.clone();

            prefs_for_reset.borrow_mut().auto_scan = defaults.auto_scan;
            prefs_for_reset.borrow_mut().expand_connected_details = defaults.expand_connected_details;
            prefs_for_reset.borrow_mut().icons_only_navigation = defaults.icons_only_navigation;

            theme_combo_for_reset.set_selected(0);
            style_manager_for_reset.set_color_scheme(adw::ColorScheme::Default);
            storage_row_for_reset.set_selected(
                Self::selection_from_password_storage(&defaults.hotspot_password_storage),
            );

            auto_scan_for_reset.set_active(defaults.auto_scan);
            expand_details_for_reset.set_active(defaults.expand_connected_details);
            nav_icons_only_for_reset.set_active(defaults.icons_only_navigation);
            Self::apply_navigation_mode(
                &wifi_stack_page_for_reset,
                &ethernet_stack_page_for_reset,
                &hotspot_stack_page_for_reset,
                &devices_stack_page_for_reset,
                &profiles_stack_page_for_reset,
                &view_switcher_for_reset,
                defaults.icons_only_navigation,
            );
            wifi_for_reset.apply_expand_details_setting(defaults.expand_connected_details);
        });

        let page = adw::PreferencesPage::new();
        page.set_title("Settings");
        page.add(&group);
        page.add(&storage_group);
        page.add(&personalization_group);
        page.add(&reset_group);

        let settings = adw::PreferencesDialog::builder()
            .title("Settings")
            .build();
        settings.add(&page);
        settings.present(Some(window));
    }

    fn load_saved_theme() {
        let style_manager = adw::StyleManager::default();
        if let Ok(settings) = config::load_app_settings(&config::app_settings_path()) {
            let scheme = match settings.color_scheme.as_str() {
                "light" => adw::ColorScheme::ForceLight,
                "dark" => adw::ColorScheme::ForceDark,
                _ => adw::ColorScheme::Default,
            };
            style_manager.set_color_scheme(scheme);
        }
    }

    fn color_scheme_from_selection(selected: u32) -> adw::ColorScheme {
        match selected {
            1 => adw::ColorScheme::ForceLight,
            2 => adw::ColorScheme::ForceDark,
            _ => adw::ColorScheme::Default,
        }
    }

    fn setting_from_selection(selected: u32) -> &'static str {
        match selected {
            1 => "light",
            2 => "dark",
            _ => "system",
        }
    }

    fn password_storage_from_selection(selected: u32) -> config::HotspotPasswordStorage {
        match selected {
            1 => config::HotspotPasswordStorage::NetworkManager,
            2 => config::HotspotPasswordStorage::PlainJson,
            _ => config::HotspotPasswordStorage::Keyring,
        }
    }

    fn selection_from_password_storage(storage: &config::HotspotPasswordStorage) -> u32 {
        match storage {
            config::HotspotPasswordStorage::Keyring => 0,
            config::HotspotPasswordStorage::NetworkManager => 1,
            config::HotspotPasswordStorage::PlainJson => 2,
        }
    }

    fn apply_navigation_mode(
        wifi_stack_page: &adw::ViewStackPage,
        ethernet_stack_page: &adw::ViewStackPage,
        hotspot_stack_page: &adw::ViewStackPage,
        devices_stack_page: &adw::ViewStackPage,
        profiles_stack_page: &adw::ViewStackPage,
        view_switcher: &adw::ViewSwitcher,
        icons_only: bool,
    ) {
        let wifi_title = if icons_only { "" } else { "Wi-Fi" };
        let ethernet_title = if icons_only { "" } else { "Ethernet" };
        let hotspot_title = if icons_only { "" } else { "Hotspot" };
        let devices_title = if icons_only { "" } else { "Devices" };
        let profiles_title = if icons_only { "" } else { "Profiles" };

        wifi_stack_page.set_title(Some(wifi_title));
        ethernet_stack_page.set_title(Some(ethernet_title));
        hotspot_stack_page.set_title(Some(hotspot_title));
        devices_stack_page.set_title(Some(devices_title));
        profiles_stack_page.set_title(Some(profiles_title));

        Self::apply_view_switcher_tooltips(
            view_switcher,
            &["Wi-Fi", "Ethernet", "Hotspot", "Devices", "Profiles"][..],
        );
    }

    fn apply_view_switcher_tooltips(view_switcher: &adw::ViewSwitcher, tooltips: &[&str]) {
        fn walk(widget: &gtk4::Widget, tooltips: &[&str], idx: &mut usize) {
            if let Ok(button) = widget.clone().downcast::<gtk4::ToggleButton>() {
                if let Some(text) = tooltips.get(*idx) {
                    button.set_tooltip_text(Some(text));
                }
                *idx += 1;
            }

            let mut child = widget.first_child();
            while let Some(current) = child {
                walk(&current, tooltips, idx);
                child = current.next_sibling();
            }
        }

        let root: &gtk4::Widget = view_switcher.upcast_ref();
        let mut idx = 0usize;
        walk(root, tooltips, &mut idx);
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

/* Main headerbar close button size */
headerbar windowcontrols button.close,
headerbar button.close,
.titlebar windowcontrols button.close,
.titlebar button.close {
    min-width: 18px;
    min-height: 18px;
    padding: 0;
    margin: 0;
    border-radius: 4px;
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

/* Header menu button hitbox */
button.menu-button {
    padding: 0;
    min-width: 18px;
    min-height: 18px;
    border-radius: 4px;
}

button.menu-button:hover {
    background: transparent;
}

button.header-mini-button {
    min-width: 18px;
    min-height: 18px;
    padding: 0;
    border-radius: 4px;
}

headerbar button.header-mini-button {
    min-width: 16px;
    min-height: 16px;
    padding: 0;
    margin: 0;
    border-radius: 4px;
}

headerbar button.window-close {
    border-radius: 999px;
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

.status-speed-box {
    opacity: 0.8;
}

.status-speed-text {
    font-size: 0.8em;
}

.status-speed-sep {
    opacity: 0.6;
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

/* Light mode tuning */
@media (prefers-color-scheme: light) {
    .card {
        background: alpha(@window_fg_color, 0.025);
        border: 1px solid alpha(@window_fg_color, 0.08);
    }

    .connected-card {
        background: alpha(@accent_bg_color, 0.1);
        border-color: alpha(@accent_bg_color, 0.25);
    }

    .network-row:hover,
    .network-item:hover,
    row:hover {
        background: alpha(@window_fg_color, 0.04);
    }

    .status-pill {
        background: alpha(@accent_bg_color, 0.1);
        border: 1px solid alpha(@window_fg_color, 0.08);
    }

    .status-pill.status-online {
        background: alpha(@success_color, 0.1);
    }

    .status-pill.status-offline {
        background: alpha(@warning_color, 0.1);
    }

    .status-pill.status-hotspot {
        background: alpha(@success_color, 0.14);
    }

    button.window-close {
        background: alpha(@window_fg_color, 0.08);
        border-color: alpha(@window_fg_color, 0.16);
    }

    viewswitcher button:checked {
        background: alpha(@accent_bg_color, 0.2);
    }
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

fn read_interface_bytes(iface: &str) -> Option<(u64, u64)> {
    let rx_path = format!("/sys/class/net/{}/statistics/rx_bytes", iface);
    let tx_path = format!("/sys/class/net/{}/statistics/tx_bytes", iface);
    let rx = fs::read_to_string(rx_path).ok()?.trim().parse::<u64>().ok()?;
    let tx = fs::read_to_string(tx_path).ok()?.trim().parse::<u64>().ok()?;
    Some((rx, tx))
}

fn format_speed(bytes_per_sec: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

    let value = bytes_per_sec as f64;
    if value >= GIB {
        format!("{:.1} GiB/s", value / GIB)
    } else if value >= MIB {
        format!("{:.1} MiB/s", value / MIB)
    } else if value >= KIB {
        format!("{:.0} KiB/s", value / KIB)
    } else {
        format!("{} B/s", bytes_per_sec)
    }
}
