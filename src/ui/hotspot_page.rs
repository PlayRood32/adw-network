// File: hotspot_page.rs
// Location: /src/ui/hotspot_page.rs

use gtk4::prelude::*;
use gtk4::glib;
use libadwaita::{self as adw, prelude::*};

use crate::config::{self, HotspotConfig, HotspotPasswordStorage};
use crate::hotspot;
use crate::nm;
use crate::secrets;
use crate::qr_dialog;
use crate::ui::icon_name;
use crate::window::AppPrefs;
use rand::distr::Alphanumeric;
use rand::RngExt;
use rand::seq::SliceRandom;

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

const MIN_PASSWORD_LEN: usize = 8;
const MAX_PASSWORD_LEN: usize = 63;

const QR_CODE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" height="24px" viewBox="0 -960 960 960" width="24px" fill="#000000">
  <path d="M120-520v-320h320v320H120Zm80-80h160v-160H200v160Zm-80 480v-320h320v320H120Zm80-80h160v-160H200v160Zm320-320v-320h320v320H520Zm80-80h160v-160H600v160Zm160 480v-80h80v80h-80ZM520-360v-80h80v80h-80Zm80 80v-80h80v80h-80Zm-80 80v-80h80v80h-80Zm80 80v-80h80v80h-80Zm80-80v-80h80v80h-80Zm0-160v-80h80v80h-80Zm80 80v-80h80v80h-80Z"/>
</svg>
"##;

fn band_to_index(band: &str) -> u32 {
    match band {
        "2.4 GHz" => 0,
        "5 GHz" => 1,
        _ => 2, // Auto
    }
}

pub struct HotspotPage {
    pub widget: gtk4::Box,
    toast_overlay: adw::ToastOverlay,
    hotspot_switch: adw::SwitchRow,
    ssid_entry: adw::EntryRow,
    password_entry: adw::PasswordEntryRow,
    band_combo: adw::ComboRow,
    hidden_switch: adw::SwitchRow,
    interface_combo: adw::ComboRow,
    config_group: adw::PreferencesGroup,
    qr_button: gtk4::Button,
    status_label: gtk4::Label,
    status_subtitle: gtk4::Label,
    status_meta: gtk4::Label,
    status_icon: gtk4::Image,
    reveal_switch: adw::SwitchRow,
    revealed_password_row: adw::ActionRow,
    revealed_password_label: gtk4::Label,
    strength_label: gtk4::Label,
    strength_bar: gtk4::ProgressBar,
    devices: Rc<RefCell<Vec<String>>>,
    is_active: Rc<Cell<bool>>,
    wifi_present: Rc<Cell<bool>>,
    wifi_enabled: Rc<Cell<bool>>,
    prefs: Rc<RefCell<AppPrefs>>,
    operation_in_progress: Rc<Cell<bool>>,
}

impl HotspotPage {
    pub fn new(prefs: Rc<RefCell<AppPrefs>>) -> Self {
        let widget = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let toast_overlay = adw::ToastOverlay::new();

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .build();

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);

        // Status Header with icon and label
        let status_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        status_box.set_halign(gtk4::Align::Center);
        status_box.set_margin_bottom(12);

        let status_icon = gtk4::Image::from_icon_name(icon_name(
            "network-wireless-hotspot-symbolic",
            &[
                "network-wireless-symbolic",
                "network-wireless-signal-excellent-symbolic",
            ][..],
        ));
        status_icon.set_pixel_size(100);
        status_icon.add_css_class("hotspot-icon");
        status_icon.set_margin_bottom(8);

        let status_label = gtk4::Label::new(Some("Hotspot is off"));
        status_label.add_css_class("title-2");

        let status_subtitle = gtk4::Label::new(None);
        status_subtitle.set_opacity(0.7);
        status_subtitle.set_wrap(true);
        status_subtitle.set_visible(false);

        let status_meta = gtk4::Label::new(None);
        status_meta.set_opacity(0.6);
        status_meta.set_wrap(true);
        status_meta.set_visible(false);

        status_box.append(&status_icon);
        status_box.append(&status_label);
        status_box.append(&status_subtitle);
        status_box.append(&status_meta);
        content.append(&status_box);

        // Action buttons (placed near status for quick access)
        let action_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        action_box.set_margin_top(6);
        action_box.set_margin_bottom(12);
        action_box.set_margin_start(12);
        action_box.set_margin_end(12);
        action_box.set_halign(gtk4::Align::End);

        let qr_button = gtk4::Button::builder()
            .tooltip_text("Show QR code for this hotspot")
            .css_classes(vec![
                "action-pill".to_string(),
                "qr-pill".to_string(),
                "touch-target".to_string(),
            ])
            .build();
        qr_button.set_visible(false);

        let qr_content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let qr_icon = {
            let bytes = glib::Bytes::from_static(QR_CODE_SVG.as_bytes());
            match gtk4::gdk::Texture::from_bytes(&bytes) {
                Ok(texture) => gtk4::Image::from_paintable(Some(&texture)),
                Err(_) => gtk4::Image::from_icon_name(icon_name(
                    "qr-code-symbolic",
                    &["view-qr-symbolic", "view-qr", "view-qr-code-symbolic"][..],
                )),
            }
        };
        qr_icon.set_pixel_size(18);
        let qr_label = gtk4::Label::new(Some("Show QR Code to Connect"));
        qr_label.set_xalign(0.0);
        qr_content.append(&qr_icon);
        qr_content.append(&qr_label);
        qr_button.set_child(Some(&qr_content));

        action_box.append(&qr_button);
        content.append(&action_box);

        // Switch
        let hotspot_switch = adw::SwitchRow::builder()
            .title("Hotspot")
            .build();

        let switch_group = adw::PreferencesGroup::new();
        switch_group.add(&hotspot_switch);
        content.append(&switch_group);

        // Configuration Group
        let config_group = adw::PreferencesGroup::builder()
            .title("Configuration")
            .margin_top(12)
            .build();

        let ssid_entry = adw::EntryRow::builder()
            .title("Network Name (SSID)")
            .build();

        let password_entry = adw::PasswordEntryRow::builder()
            .title("Password")
            .build();

        let generate_button = gtk4::Button::builder()
            .icon_name(icon_name(
                "view-refresh-symbolic",
                &["view-refresh", "reload-symbolic"][..],
            ))
            .valign(gtk4::Align::Center)
            .tooltip_text("Generate password")
            .css_classes(vec!["flat".to_string(), "touch-target".to_string()])
            .build();

        password_entry.add_suffix(&generate_button);

        let generate_popover = gtk4::Popover::new();
        generate_popover.set_has_arrow(true);
        generate_popover.set_parent(&generate_button);
        generate_popover.add_css_class("menu");

        let generate_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        generate_box.add_css_class("menu");
        generate_box.set_margin_top(6);
        generate_box.set_margin_bottom(6);

        let make_generate_button = |label: &str| {
            gtk4::Button::builder()
                .label(label)
                .css_classes(vec!["flat".to_string()])
                .build()
        };

        let gen_8 = make_generate_button("8 characters");
        let gen_12 = make_generate_button("12 characters");
        let gen_16 = make_generate_button("16 characters");
        let gen_20 = make_generate_button("20 characters");
        let gen_32 = make_generate_button("32 characters");

        generate_box.append(&gen_8);
        generate_box.append(&gen_12);
        generate_box.append(&gen_16);
        generate_box.append(&gen_20);
        generate_box.append(&gen_32);
        generate_popover.set_child(Some(&generate_box));

        // Reveal password row
        let reveal_switch = adw::SwitchRow::builder()
            .title("Reveal password")
            .subtitle("Show the hotspot password in plain text")
            .build();

        let revealed_password_row = adw::ActionRow::builder()
            .title("Password")
            .build();
        let revealed_password_label = gtk4::Label::new(None);
        revealed_password_label.set_selectable(true);
        revealed_password_label.add_css_class("monospace");
        revealed_password_row.add_suffix(&revealed_password_label);
        revealed_password_row.set_visible(false);

        let length_row = adw::ActionRow::builder()
            .title("Password length")
            .build();
        let length_adjustment = gtk4::Adjustment::new(
            16.0,
            MIN_PASSWORD_LEN as f64,
            MAX_PASSWORD_LEN as f64,
            1.0,
            4.0,
            0.0,
        );
        let length_spin = gtk4::SpinButton::builder()
            .adjustment(&length_adjustment)
            .numeric(true)
            .digits(0)
            .build();
        length_spin.set_width_chars(3);
        length_row.add_suffix(&length_spin);

        let strength_row = adw::ActionRow::builder()
            .title("Password strength")
            .build();
        let strength_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        strength_box.set_hexpand(true);

        let strength_bar = gtk4::ProgressBar::new();
        strength_bar.set_hexpand(true);
        strength_bar.set_show_text(false);

        let strength_label = gtk4::Label::new(Some("Weak"));
        strength_label.set_xalign(0.0);
        strength_label.add_css_class("strength-label");

        strength_box.append(&strength_bar);
        strength_box.append(&strength_label);
        strength_row.add_suffix(&strength_box);

        // Advanced Settings
        let band_model = gtk4::StringList::new(&["2.4 GHz", "5 GHz", "Auto"][..]);
        let band_combo = adw::ComboRow::builder()
            .title("Frequency Band")
            .model(&band_model)
            .selected(2) // Auto by default
            .build();

        let hidden_switch = adw::SwitchRow::builder()
            .title("Hidden Network")
            .subtitle("Network won't be visible in WiFi lists")
            .build();

        let interface_model = gtk4::StringList::new(&[][..]);
        let interface_combo = adw::ComboRow::builder()
            .title("Network Interface")
            .model(&interface_model)
            .selected(0)
            .build();

        config_group.add(&ssid_entry);
        config_group.add(&password_entry);
        config_group.add(&length_row);
        config_group.add(&strength_row);
        config_group.add(&reveal_switch);
        config_group.add(&revealed_password_row);
        config_group.add(&band_combo);
        config_group.add(&hidden_switch);
        config_group.add(&interface_combo);

        content.append(&config_group);

        scrolled.set_child(Some(&content));
        toast_overlay.set_child(Some(&scrolled));
        widget.append(&toast_overlay);

        let devices = Rc::new(RefCell::new(Vec::new()));
        let is_active = Rc::new(Cell::new(false));
        let wifi_present = Rc::new(Cell::new(false));
        let wifi_enabled = Rc::new(Cell::new(false));
        let operation_in_progress = Rc::new(Cell::new(false));
        let password_adjusting = Rc::new(Cell::new(false));

        let page = Self {
            widget,
            toast_overlay,
            hotspot_switch: hotspot_switch.clone(),
            ssid_entry: ssid_entry.clone(),
            password_entry: password_entry.clone(),
            band_combo: band_combo.clone(),
            hidden_switch: hidden_switch.clone(),
            interface_combo: interface_combo.clone(),
            config_group: config_group.clone(),
            qr_button: qr_button.clone(),
            status_label: status_label.clone(),
            status_subtitle: status_subtitle.clone(),
            status_meta: status_meta.clone(),
            status_icon: status_icon.clone(),
            reveal_switch: reveal_switch.clone(),
            revealed_password_row: revealed_password_row.clone(),
            revealed_password_label: revealed_password_label.clone(),
            strength_label: strength_label.clone(),
            strength_bar: strength_bar.clone(),
            devices,
            is_active,
            wifi_present,
            wifi_enabled,
            prefs,
            operation_in_progress,
        };

        page.set_wifi_state(false, false);

        // Load configuration and check status
        let page_ref = page.clone_ref();
        glib::spawn_future_local(async move {
            page_ref.load_config().await;
            page_ref.load_interfaces().await;
            page_ref.refresh_status().await;
        });

        // Generate password button
        let set_generated = |len: usize,
                             include_symbols: bool,
                             password_entry: adw::PasswordEntryRow,
                             revealed_label: gtk4::Label,
                             popover: Option<gtk4::Popover>| {
            let password = generate_password(len, include_symbols);
            password_entry.set_text(&password);
            revealed_label.set_text(&password);
            if let Some(popover) = popover {
                popover.popdown();
            }
        };

        let password_entry_clone = password_entry.clone();
        let revealed_label_clone = revealed_password_label.clone();
        let length_spin_clone = length_spin.clone();
        let toast_overlay_clone = page.toast_overlay.clone();
        generate_button.connect_clicked(move |_| {
            let len = length_spin_clone.value_as_int();
            let target = if len <= 0 { MIN_PASSWORD_LEN as i32 } else { len };
            if target < MIN_PASSWORD_LEN as i32 {
                let toast = adw::Toast::new("Length must be at least 8 characters");
                toast_overlay_clone.add_toast(toast);
                return;
            }
            let target = if target as usize > MAX_PASSWORD_LEN {
                let toast = adw::Toast::new("Maximum length is 63 characters");
                toast_overlay_clone.add_toast(toast);
                length_spin_clone.set_value(MAX_PASSWORD_LEN as f64);
                MAX_PASSWORD_LEN as i32
            } else {
                target
            };
            set_generated(
                target as usize,
                true,
                password_entry_clone.clone(),
                revealed_label_clone.clone(),
                None,
            );
        });

        // Right click menu for common lengths
        let popover_clone = generate_popover.clone();
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(3);
        gesture.connect_released(move |_gesture, _n_press, _x, _y| {
            popover_clone.popup();
        });
        generate_button.add_controller(gesture);

        let password_entry_clone = password_entry.clone();
        let revealed_label_clone = revealed_password_label.clone();
        let popover_clone = generate_popover.clone();
        gen_8.connect_clicked(move |_| {
            set_generated(
                8,
                true,
                password_entry_clone.clone(),
                revealed_label_clone.clone(),
                Some(popover_clone.clone()),
            );
        });

        let password_entry_clone = password_entry.clone();
        let revealed_label_clone = revealed_password_label.clone();
        let popover_clone = generate_popover.clone();
        gen_12.connect_clicked(move |_| {
            set_generated(
                12,
                true,
                password_entry_clone.clone(),
                revealed_label_clone.clone(),
                Some(popover_clone.clone()),
            );
        });

        let password_entry_clone = password_entry.clone();
        let revealed_label_clone = revealed_password_label.clone();
        let popover_clone = generate_popover.clone();
        gen_16.connect_clicked(move |_| {
            set_generated(
                16,
                true,
                password_entry_clone.clone(),
                revealed_label_clone.clone(),
                Some(popover_clone.clone()),
            );
        });

        let password_entry_clone = password_entry.clone();
        let revealed_label_clone = revealed_password_label.clone();
        let popover_clone = generate_popover.clone();
        gen_20.connect_clicked(move |_| {
            set_generated(
                20,
                true,
                password_entry_clone.clone(),
                revealed_label_clone.clone(),
                Some(popover_clone.clone()),
            );
        });

        let password_entry_clone = password_entry.clone();
        let revealed_label_clone = revealed_password_label.clone();
        let popover_clone = generate_popover.clone();
        gen_32.connect_clicked(move |_| {
            set_generated(
                32,
                true,
                password_entry_clone.clone(),
                revealed_label_clone.clone(),
                Some(popover_clone.clone()),
            );
        });

        // Reveal password toggle
        let revealed_row_clone = revealed_password_row.clone();
        reveal_switch.connect_active_notify(move |row| {
            revealed_row_clone.set_visible(row.is_active());
        });

        // Keep revealed password in sync, update strength, and auto-save when inactive
        let revealed_label_clone = revealed_password_label.clone();
        let strength_label_clone = strength_label.clone();
        let strength_bar_clone = strength_bar.clone();
        let page_ref = page.clone_ref();
        let password_adjusting = password_adjusting.clone();
        password_entry.connect_changed(move |entry| {
            if password_adjusting.get() {
                return;
            }
            let text = entry.text();
            let mut value = text.to_string();
            if value.chars().count() > MAX_PASSWORD_LEN {
                let truncated: String = value.chars().take(MAX_PASSWORD_LEN).collect();
                password_adjusting.set(true);
                entry.set_text(&truncated);
                password_adjusting.set(false);
                value = truncated;
            }
            revealed_label_clone.set_text(&value);
            update_strength_indicator(&value, &strength_label_clone, &strength_bar_clone);
            let page = page_ref.clone_ref();
            glib::spawn_future_local(async move {
                page.save_configuration_if_inactive().await;
            });
        });

        let page_ref = page.clone_ref();
        ssid_entry.connect_changed(move |_| {
            let page = page_ref.clone_ref();
            glib::spawn_future_local(async move {
                page.save_configuration_if_inactive().await;
            });
        });

        let page_ref = page.clone_ref();
        band_combo.connect_selected_notify(move |_| {
            let page = page_ref.clone_ref();
            glib::spawn_future_local(async move {
                page.save_configuration_if_inactive().await;
            });
        });

        let page_ref = page.clone_ref();
        hidden_switch.connect_active_notify(move |_| {
            let page = page_ref.clone_ref();
            glib::spawn_future_local(async move {
                page.save_configuration_if_inactive().await;
            });
        });

        let page_ref = page.clone_ref();
        interface_combo.connect_selected_notify(move |_| {
            let page = page_ref.clone_ref();
            glib::spawn_future_local(async move {
                page.save_configuration_if_inactive().await;
            });
        });

        // Hotspot switch handler
        let page_ref = page.clone_ref();
        hotspot_switch.connect_active_notify(move |switch| {
            let page = page_ref.clone_ref();
            let active = switch.is_active();
            
            glib::spawn_future_local(async move {
                if active && !page.is_active.get() {
                    page.start_hotspot().await;
                } else if !active && page.is_active.get() {
                    page.stop_hotspot().await;
                }
            });
        });

        // Hotspot switch handler - kept for compatibility
        let page_ref = page.clone_ref();
        hotspot_switch.connect_active_notify(move |switch| {
            let page = page_ref.clone_ref();
            let active = switch.is_active();

            // Prevent loops
            if active == page.is_active.get() {
                return;
            }

            glib::spawn_future_local(async move {
                if active {
                    page.start_hotspot().await;
                } else {
                    page.stop_hotspot().await;
                }
            });
        });

        // QR button handler
        let page_ref = page.clone_ref();
        qr_button.connect_clicked(move |_| {
            let page = page_ref.clone_ref();
            glib::spawn_future_local(async move {
                page.show_qr().await;
            });
        });

        // Refresh status details periodically while active
        let page_ref = page.clone_ref();
        glib::timeout_add_seconds_local(5, move || {
            if page_ref.is_active.get() {
                page_ref.refresh_status_details();
            }
            glib::ControlFlow::Continue
        });

        // Refresh Wi-Fi interfaces periodically to handle hot-plug/unplug
        let page_ref = page.clone_ref();
        glib::timeout_add_seconds_local(3, move || {
            let page = page_ref.clone_ref();
            glib::spawn_future_local(async move {
                page.load_interfaces().await;
                page.refresh_status().await;
            });
            glib::ControlFlow::Continue
        });

        page
    }

    pub fn clone_ref(&self) -> Self {
        Self {
            widget: self.widget.clone(),
            toast_overlay: self.toast_overlay.clone(),
            hotspot_switch: self.hotspot_switch.clone(),
            ssid_entry: self.ssid_entry.clone(),
            password_entry: self.password_entry.clone(),
            band_combo: self.band_combo.clone(),
            hidden_switch: self.hidden_switch.clone(),
            interface_combo: self.interface_combo.clone(),
            config_group: self.config_group.clone(),
            qr_button: self.qr_button.clone(),
            status_label: self.status_label.clone(),
            status_subtitle: self.status_subtitle.clone(),
            status_meta: self.status_meta.clone(),
            status_icon: self.status_icon.clone(),
            reveal_switch: self.reveal_switch.clone(),
            revealed_password_row: self.revealed_password_row.clone(),
            revealed_password_label: self.revealed_password_label.clone(),
            strength_label: self.strength_label.clone(),
            strength_bar: self.strength_bar.clone(),
            devices: self.devices.clone(),
            is_active: self.is_active.clone(),
            wifi_present: self.wifi_present.clone(),
            wifi_enabled: self.wifi_enabled.clone(),
            prefs: self.prefs.clone(),
            operation_in_progress: self.operation_in_progress.clone(),
        }
    }

    async fn load_config(&self) {
        let storage = self.load_password_storage();
        match config::load_config(&config::hotspot_config_path()) {
            Ok(config) => {
                self.ssid_entry.set_text(&config.ssid);
                let password = self.resolve_password_for_storage(&storage, Some(&config)).await;
                self.password_entry.set_text(&password);
                self.revealed_password_label.set_text(&password);
                update_strength_indicator(
                    &password,
                    &self.strength_label,
                    &self.strength_bar,
                );
                self.band_combo.set_selected(band_to_index(&config.band));
                self.hidden_switch.set_active(config.hidden);
            }
            Err(_) => {
                let config = HotspotConfig::default();
                self.ssid_entry.set_text(&config.ssid);
                let password = self.resolve_password_for_storage(&storage, None).await;
                self.password_entry.set_text(&password);
                self.revealed_password_label.set_text(&password);
                update_strength_indicator(
                    &password,
                    &self.strength_label,
                    &self.strength_bar,
                );
            }
        }
    }

    async fn save_configuration_if_inactive(&self) {
        if self.is_active.get() || self.operation_in_progress.get() {
            return;
        }

        let config = HotspotConfig {
            ssid: self.ssid_entry.text().to_string(),
            password: self.password_entry.text().to_string(),
            band: match self.band_combo.selected() {
                0 => "2.4 GHz".to_string(),
                1 => "5 GHz".to_string(),
                _ => "Auto".to_string(),
            },
            channel: "Auto".to_string(),
            hidden: self.hidden_switch.is_active(),
        };

        if let Err(e) = config.validate() {
            self.show_toast(&format!("Invalid configuration: {}", e));
            return;
        }

        let storage = self.load_password_storage();
        if let Err(e) = self.persist_password_for_storage(&storage, &config.password) {
            log::error!("Failed to store hotspot password: {}", e);
            self.show_toast(&format!("Failed to store hotspot password: {}", e));
        }

        let config_to_save = Self::config_for_storage(&config, &storage);
        if let Err(e) = config::save_config(&config::hotspot_config_path(), &config_to_save) {
            log::error!("Failed to save configuration: {}", e);
            self.show_toast(&format!("Failed to save configuration: {}", e));
        }
    }

    async fn start_hotspot(&self) {
        if self.operation_in_progress.get() {
            return;
        }

        if !self.wifi_present.get() {
            self.show_toast("No Wi-Fi adapter found");
            self.hotspot_switch.set_active(false);
            return;
        }
        if !self.wifi_enabled.get() {
            self.show_toast("Wi-Fi is off");
            self.hotspot_switch.set_active(false);
            return;
        }

        self.operation_in_progress.set(true);
        self.status_label.set_text("Starting hotspot…");
        self.status_icon.add_css_class("spinning");

        // First save the configuration
        let storage = self.load_password_storage();
        let mut password = self.password_entry.text().to_string();
        if password.is_empty() {
            let resolved = self.resolve_password_for_storage(&storage, None).await;
            if !resolved.is_empty() {
                password = resolved;
                self.password_entry.set_text(&password);
                self.revealed_password_label.set_text(&password);
                update_strength_indicator(
                    &password,
                    &self.strength_label,
                    &self.strength_bar,
                );
            }
        }

        let config = HotspotConfig {
            ssid: self.ssid_entry.text().to_string(),
            password,
            band: match self.band_combo.selected() {
                0 => "2.4 GHz".to_string(),
                1 => "5 GHz".to_string(),
                _ => "Auto".to_string(),
            },
            channel: "Auto".to_string(),
            hidden: self.hidden_switch.is_active(),
        };

        if let Err(e) = config.validate() {
            self.show_toast(&format!("Invalid configuration: {}", e));
            self.hotspot_switch.set_active(false);
            self.operation_in_progress.set(false);
            return;
        }

        let iface_idx = self.interface_combo.selected() as usize;
        let devices = self.devices.borrow();
        let interface = devices.get(iface_idx).map(|s| s.as_str()).unwrap_or("wlan0");

        match hotspot::create_hotspot_on(&config, interface).await {
            Ok(_) => {
                let _ = self.persist_password_for_storage(&storage, &config.password);
                let config_to_save = Self::config_for_storage(&config, &storage);
                let _ = config::save_config(&config::hotspot_config_path(), &config_to_save);
                self.is_active.set(true);
                self.show_toast("Hotspot started successfully");
                self.update_ui();
            }
            Err(e) => {
                log::error!("Failed to start hotspot: {}", e);
                self.show_toast(&format!("Failed to start hotspot: {}", e));
                self.is_active.set(false);
                self.hotspot_switch.set_active(false);
                self.update_ui();
            }
        }

        self.operation_in_progress.set(false);
        self.status_icon.remove_css_class("spinning");
    }

    async fn stop_hotspot(&self) {
        if self.operation_in_progress.get() {
            return;
        }

        self.operation_in_progress.set(true);
        self.status_label.set_text("Stopping hotspot…");
        self.status_icon.add_css_class("spinning");

        match hotspot::stop_hotspot().await {
            Ok(_) => {
                self.is_active.set(false);
                self.show_toast("Hotspot stopped");
                self.update_ui();
            }
            Err(e) => {
                log::error!("Failed to stop hotspot: {}", e);
                self.show_toast(&format!("Failed to stop hotspot: {}", e));
                
                // Even if stop failed, try to recover state
                if let Ok(active) = hotspot::is_hotspot_active().await {
                    self.is_active.set(active);
                    self.hotspot_switch.set_active(active);
                }
                
                self.update_ui();
            }
        }

        self.operation_in_progress.set(false);
        self.status_icon.remove_css_class("spinning");
    }

    async fn refresh_status(&self) {
        if !self.wifi_present.get() {
            self.is_active.set(false);
            self.hotspot_switch.set_active(false);
            self.update_ui();
            return;
        }

        match hotspot::is_hotspot_active().await {
            Ok(active) => {
                self.is_active.set(active);
                self.hotspot_switch.set_active(active);
                self.update_ui();
            }
            Err(e) => {
                log::error!("Failed to check hotspot status: {}", e);
                self.is_active.set(false);
                self.hotspot_switch.set_active(false);
                self.update_ui();
            }
        }
    }

    async fn load_interfaces(&self) {
        let present = nm::is_wifi_present().await.unwrap_or(false);
        let enabled = nm::is_wifi_enabled().await.unwrap_or(false);

        match hotspot::get_wifi_devices().await {
            Ok(ifaces) if !ifaces.is_empty() => {
                *self.devices.borrow_mut() = ifaces.clone();
                let model = gtk4::StringList::new(
                    &ifaces.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                );
                self.interface_combo.set_model(Some(&model));
                self.interface_combo.set_selected(0);
                self.set_wifi_state(present, enabled);
                log::info!("Loaded {} WiFi interfaces, selected: {}", ifaces.len(), &ifaces[0]);
            }
            Ok(_) | Err(_) => {
                *self.devices.borrow_mut() = Vec::new();
                let empty_model = gtk4::StringList::new(&[][..]);
                self.interface_combo.set_model(Some(&empty_model));
                self.set_wifi_state(present, enabled);
            }
        }
    }

    fn set_wifi_state(&self, present: bool, enabled: bool) {
        self.wifi_present.set(present);
        self.wifi_enabled.set(enabled);
        self.hotspot_switch.set_sensitive(present && enabled);
        self.config_group.set_sensitive(present && enabled);
        self.interface_combo.set_sensitive(present);
        self.qr_button.set_visible(present && enabled && self.is_active.get());

        if !present {
            self.devices.borrow_mut().clear();
            let empty_model = gtk4::StringList::new(&[][..]);
            self.interface_combo.set_model(Some(&empty_model));
            self.hotspot_switch.set_active(false);
        }

        self.update_ui();
    }

    fn update_ui(&self) {
        if !self.wifi_present.get() {
            self.status_label.set_text("No Wi-Fi adapter found");
            self.status_label.remove_css_class("hotspot-active-header");
            self.status_icon.remove_css_class("hotspot-pulse");
            self.status_subtitle.set_visible(false);
            self.status_meta.set_visible(false);
            self.status_subtitle.set_text("");
            self.status_meta.set_text("");
            self.qr_button.set_visible(false);
            return;
        }
        if !self.wifi_enabled.get() {
            self.status_label.set_text("Wi-Fi is off");
            self.status_label.remove_css_class("hotspot-active-header");
            self.status_icon.remove_css_class("hotspot-pulse");
            self.status_subtitle.set_visible(false);
            self.status_meta.set_visible(false);
            self.status_subtitle.set_text("");
            self.status_meta.set_text("");
            self.qr_button.set_visible(false);
            return;
        }

        let active = self.is_active.get();

        if active {
            // Update status
            self.status_label.set_text("Hotspot is active");
            self.status_label.add_css_class("hotspot-active-header");
            self.status_icon.add_css_class("hotspot-pulse");
            self.status_subtitle.set_visible(true);
            self.status_meta.set_visible(true);
            // Show QR button
            self.qr_button.set_visible(true);
            self.refresh_status_details();
        } else {
            // Update status
            self.status_label.set_text("Hotspot is off");
            self.status_label.remove_css_class("hotspot-active-header");
            self.status_icon.remove_css_class("hotspot-pulse");
            self.status_subtitle.set_visible(false);
            self.status_meta.set_visible(false);
            self.status_subtitle.set_text("");
            self.status_meta.set_text("");

            // Hide QR button
            self.qr_button.set_visible(false);
        }
    }

    fn refresh_status_details(&self) {
        let ssid = self.ssid_entry.text().to_string();
        let iface_idx = self.interface_combo.selected() as usize;
        let iface = self
            .devices
            .borrow()
            .get(iface_idx)
            .cloned()
            .unwrap_or_else(|| "wlan0".to_string());
        let status_subtitle = self.status_subtitle.clone();
        let status_meta = self.status_meta.clone();

        glib::spawn_future_local(async move {
            let count = hotspot::get_connected_device_count().await.unwrap_or(0);
            let device_text = match count {
                0 => "No devices connected".to_string(),
                1 => "1 device connected".to_string(),
                _ => format!("{} devices connected", count),
            };
            status_subtitle.set_text(&format!("{} • {}", ssid, device_text));

            let ip = hotspot::get_hotspot_ip().await.ok().flatten();
            let meta = match ip {
                Some(ip) => format!("Share internet from: {} • Hotspot IP: {}", iface, ip),
                std::prelude::v1::None => format!("Share internet from: {}", iface),
            };
            status_meta.set_text(&meta);
        });
    }

    async fn show_qr(&self) {
        let ssid = self.ssid_entry.text().to_string();
        let storage = self.load_password_storage();
        let password = self.resolve_password_for_storage(&storage, None).await;

        qr_dialog::show_qr_dialog(&ssid, &password, 200, &self.toast_overlay).await;
    }

    fn show_toast(&self, message: &str) {
        let toast = adw::Toast::new(message);
        self.toast_overlay.add_toast(toast);
    }

    fn load_password_storage(&self) -> HotspotPasswordStorage {
        config::load_app_settings(&config::app_settings_path())
            .map(|s| s.hotspot_password_storage)
            .unwrap_or(HotspotPasswordStorage::Keyring)
    }

    async fn resolve_password_for_storage(
        &self,
        storage: &HotspotPasswordStorage,
        config: Option<&HotspotConfig>,
    ) -> String {
        let entry_password = self.password_entry.text().to_string();
        if !entry_password.is_empty() {
            return entry_password;
        }

        match storage {
            HotspotPasswordStorage::PlainJson => {
                config.map(|c| c.password.clone()).unwrap_or_default()
            }
            HotspotPasswordStorage::Keyring => match secrets::load_hotspot_password() {
                Ok(Some(password)) => password,
                _ => config.map(|c| c.password.clone()).unwrap_or_default(),
            },
            HotspotPasswordStorage::NetworkManager => {
                match nm::get_saved_password_for_ssid("Hotspot").await {
                    Ok(password) => password,
                    Err(_) => config.map(|c| c.password.clone()).unwrap_or_default(),
                }
            }
        }
    }

    fn persist_password_for_storage(
        &self,
        storage: &HotspotPasswordStorage,
        password: &str,
    ) -> anyhow::Result<()> {
        match storage {
            HotspotPasswordStorage::PlainJson => Ok(()),
            HotspotPasswordStorage::NetworkManager => Ok(()),
            HotspotPasswordStorage::Keyring => {
                if password.is_empty() {
                    secrets::delete_hotspot_password()
                } else {
                    secrets::store_hotspot_password(password)
                }
            }
        }
    }

    fn config_for_storage(config: &HotspotConfig, storage: &HotspotPasswordStorage) -> HotspotConfig {
        if *storage == HotspotPasswordStorage::PlainJson {
            config.clone()
        } else {
            let mut scrubbed = config.clone();
            scrubbed.password.clear();
            scrubbed
        }
    }
}

fn update_strength_indicator(
    password: &str,
    label: &gtk4::Label,
    bar: &gtk4::ProgressBar,
) {
    let len = password.chars().count();
    let mut has_lower = false;
    let mut has_upper = false;
    let mut has_digit = false;
    let mut has_symbol = false;

    for ch in password.chars() {
        if ch.is_ascii_lowercase() {
            has_lower = true;
        } else if ch.is_ascii_uppercase() {
            has_upper = true;
        } else if ch.is_ascii_digit() {
            has_digit = true;
        } else {
            has_symbol = true;
        }
    }

    let variety = has_lower as u8 + has_upper as u8 + has_digit as u8 + has_symbol as u8;
    let mut pool_size = 0usize;
    if has_lower {
        pool_size += 26;
    }
    if has_upper {
        pool_size += 26;
    }
    if has_digit {
        pool_size += 10;
    }
    if has_symbol {
        pool_size += 32;
    }

    let entropy = if pool_size == 0 || len == 0 {
        0.0
    } else {
        (len as f64) * (pool_size as f64).log2()
    };

    let (text, class) = if len > MAX_PASSWORD_LEN {
        ("Too long", "strength-weak")
    } else if len < MIN_PASSWORD_LEN || variety <= 1 {
        ("Weak", "strength-weak")
    } else if variety < 4 {
        ("Medium", "strength-medium")
    } else if len >= 16 {
        ("Very Strong", "strength-very-strong")
    } else if len >= 12 {
        ("Strong", "strength-strong")
    } else {
        ("Medium", "strength-medium")
    };

    bar.remove_css_class("strength-weak");
    bar.remove_css_class("strength-medium");
    bar.remove_css_class("strength-strong");
    bar.remove_css_class("strength-very-strong");
    bar.add_css_class(class);

    let fraction = if entropy <= 0.0 || len > MAX_PASSWORD_LEN {
        0.0
    } else {
        (entropy / 80.0).min(1.0)
    };

    label.set_text(text);
    bar.set_fraction(fraction);
}

fn generate_password(len: usize, include_symbols: bool) -> String {
    let len = len.min(MAX_PASSWORD_LEN);
    if len == 0 {
        return String::new();
    }

    if include_symbols {
        let symbols: Vec<char> = "!@#$%^&*()-_=+[]{};:,.?/"
            .chars()
            .collect();
        let charset: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()-_=+[]{};:,.?/"
            .chars()
            .collect();
        let mut rng = rand::rng();
        let mut chars = Vec::with_capacity(len);

        if let Some(symbol) = symbols.get(rng.random_range(0..symbols.len())) {
            chars.push(*symbol);
        }

        for _ in 1..len {
            let idx = rng.random_range(0..charset.len());
            chars.push(charset[idx]);
        }

        chars.shuffle(&mut rng);
        chars.into_iter().collect()
    } else {
        let rng = rand::rng();
        rng.sample_iter(Alphanumeric)
            .take(len)
            .map(char::from)
            .collect()
    }
}
