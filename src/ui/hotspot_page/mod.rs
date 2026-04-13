// File: hotspot_page.rs
// Location: /src/ui/hotspot_page.rs

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::{self as adw, prelude::*};

use crate::config::{
    self, HotspotClientRule, HotspotConfig, HotspotMacFilterMode, HotspotPasswordStorage,
};
use crate::hotspot;
use crate::nm;
use crate::qr_dialog;
use crate::secrets;
use crate::state::{AppState, PageKind};
use crate::ui::{common, icon_name};

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

mod actions;
mod password;
use actions::{band_from_selected, band_to_selection, is_custom_band_selected};
use password::update_strength_indicator;

const MIN_PASSWORD_LEN: usize = 8;
const MAX_PASSWORD_LEN: usize = 63;

const QR_CODE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" height="24px" viewBox="0 -960 960 960" width="24px" fill="#000000">
  <path d="M120-520v-320h320v320H120Zm80-80h160v-160H200v160Zm-80 480v-320h320v320H120Zm80-80h160v-160H200v160Zm320-320v-320h320v320H520Zm80-80h160v-160H600v160Zm160 480v-80h80v80h-80ZM520-360v-80h80v80h-80Zm80 80v-80h80v80h-80Zm-80 80v-80h80v80h-80Zm80 80v-80h80v80h-80Zm80-80v-80h80v80h-80Zm0-160v-80h80v80h-80Zm80 80v-80h80v80h-80Z"/>
</svg>
"##;

pub struct HotspotPage {
    pub widget: gtk4::Box,
    toast_overlay: adw::ToastOverlay,
    apply_button: gtk4::Button,
    hotspot_switch: adw::SwitchRow,
    ssid_entry: adw::EntryRow,
    password_entry: adw::PasswordEntryRow,
    band_combo: adw::ComboRow,
    // * Keep a handle to the custom band input row for free-text hotspot band values.
    custom_band_entry: adw::EntryRow,
    // * Keep a handle to the custom channel input row for free-text hotspot channels.
    channel_entry: adw::EntryRow,
    hidden_switch: adw::SwitchRow,
    interface_combo: adw::ComboRow,
    config_group: adw::PreferencesGroup,
    advanced_group: adw::PreferencesGroup,
    upload_limit_spin: gtk4::SpinButton,
    download_limit_spin: gtk4::SpinButton,
    device_limit_spin: gtk4::SpinButton,
    mac_filter_combo: adw::ComboRow,
    client_rules_row: adw::ActionRow,
    client_rules_button: gtk4::Button,
    advanced_support_row: adw::ActionRow,
    qr_button: gtk4::Button,
    guest_password_row: adw::ActionRow,
    guest_password_label: gtk4::Label,
    status_label: gtk4::Label,
    operation_spinner: gtk4::Spinner,
    status_subtitle: gtk4::Label,
    status_meta: gtk4::Label,
    status_icon: gtk4::Image,
    reveal_switch: adw::SwitchRow,
    revealed_password_row: adw::ActionRow,
    revealed_password_label: gtk4::Label,
    strength_label: gtk4::Label,
    strength_bar: gtk4::ProgressBar,
    // Shared UI state - accessed only from the main thread.
    devices: Rc<RefCell<Vec<String>>>,
    // Shared UI state - accessed only from the main thread.
    is_active: Rc<Cell<bool>>,
    // Shared UI state - accessed only from the main thread.
    wifi_present: Rc<Cell<bool>>,
    // Shared UI state - accessed only from the main thread.
    wifi_enabled: Rc<Cell<bool>>,
    // Shared UI state - accessed only from the main thread.
    app_state: AppState,
    // Shared UI state - accessed only from the main thread.
    operation_in_progress: Rc<Cell<bool>>,
    config_dirty: Rc<Cell<bool>>,
    client_rules: Rc<RefCell<Vec<HotspotClientRule>>>,
    temporary_password: Rc<RefCell<Option<String>>>,
    config_update_source: Rc<RefCell<Option<glib::SourceId>>>,
    status_refresh_source: Rc<RefCell<Option<glib::SourceId>>>,
    interface_refresh_source: Rc<RefCell<Option<glib::SourceId>>>,
    suppress_config_updates: Rc<Cell<u32>>,
}

impl Clone for HotspotPage {
    fn clone(&self) -> Self {
        Self {
            widget: self.widget.clone(),
            toast_overlay: self.toast_overlay.clone(),
            apply_button: self.apply_button.clone(),
            hotspot_switch: self.hotspot_switch.clone(),
            ssid_entry: self.ssid_entry.clone(),
            password_entry: self.password_entry.clone(),
            band_combo: self.band_combo.clone(),
            custom_band_entry: self.custom_band_entry.clone(),
            channel_entry: self.channel_entry.clone(),
            hidden_switch: self.hidden_switch.clone(),
            interface_combo: self.interface_combo.clone(),
            config_group: self.config_group.clone(),
            advanced_group: self.advanced_group.clone(),
            upload_limit_spin: self.upload_limit_spin.clone(),
            download_limit_spin: self.download_limit_spin.clone(),
            device_limit_spin: self.device_limit_spin.clone(),
            mac_filter_combo: self.mac_filter_combo.clone(),
            client_rules_row: self.client_rules_row.clone(),
            client_rules_button: self.client_rules_button.clone(),
            advanced_support_row: self.advanced_support_row.clone(),
            qr_button: self.qr_button.clone(),
            guest_password_row: self.guest_password_row.clone(),
            guest_password_label: self.guest_password_label.clone(),
            status_label: self.status_label.clone(),
            operation_spinner: self.operation_spinner.clone(),
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
            app_state: self.app_state.clone(),
            operation_in_progress: self.operation_in_progress.clone(),
            config_dirty: self.config_dirty.clone(),
            client_rules: self.client_rules.clone(),
            temporary_password: self.temporary_password.clone(),
            config_update_source: self.config_update_source.clone(),
            status_refresh_source: self.status_refresh_source.clone(),
            interface_refresh_source: self.interface_refresh_source.clone(),
            suppress_config_updates: self.suppress_config_updates.clone(),
        }
    }
}

impl HotspotPage {
    pub fn new(app_state: AppState) -> Self {
        let widget = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let toast_overlay = adw::ToastOverlay::new();

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .build();
        let clamp = adw::Clamp::builder()
            .maximum_size(880)
            .tightening_threshold(560)
            .build();

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(16);
        content.set_margin_end(16);

        // Status Header with icon and label
        let status_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        status_box.set_halign(gtk4::Align::Center);
        status_box.set_margin_bottom(12);
        status_box.add_css_class("hotspot-hero");

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

        let operation_spinner = gtk4::Spinner::new();
        operation_spinner.add_css_class("big-spinner");
        operation_spinner.set_size_request(30, 30);
        operation_spinner.set_visible(false);

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
        status_box.append(&operation_spinner);
        status_box.append(&status_subtitle);
        status_box.append(&status_meta);
        content.append(&status_box);

        // Action buttons (placed near status for quick access)
        let action_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        action_box.set_margin_top(6);
        action_box.set_margin_bottom(12);
        action_box.set_halign(gtk4::Align::Center);
        action_box.add_css_class("hotspot-actions");

        let apply_button = gtk4::Button::builder()
            .label("Apply Changes")
            .tooltip_text("Save hotspot changes and apply them if the hotspot is active")
            .css_classes(vec![
                "suggested-action".to_string(),
                "action-pill".to_string(),
                "touch-target".to_string(),
            ])
            .build();
        apply_button.set_sensitive(false);

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

        action_box.append(&apply_button);
        action_box.append(&qr_button);
        content.append(&action_box);

        // Switch
        let hotspot_switch = adw::SwitchRow::builder().title("Hotspot").build();

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

        let password_entry = adw::PasswordEntryRow::builder().title("Password").build();

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
        reveal_switch.set_tooltip_text(Some(
            "Password will be hidden automatically after 30 seconds for security",
        ));

        let revealed_password_row = adw::ActionRow::builder().title("Password").build();
        let revealed_password_label = gtk4::Label::new(None);
        revealed_password_label.set_selectable(true);
        revealed_password_label.add_css_class("monospace");
        revealed_password_row.add_suffix(&revealed_password_label);
        revealed_password_row.set_visible(false);

        let guest_password_row = adw::ActionRow::builder()
            .title("Temporary Guest Password")
            .subtitle(
                "Temporarily replaces the main hotspot password until the next hotspot shutdown",
            )
            .build();
        let guest_password_label = gtk4::Label::new(Some("Not generated"));
        guest_password_label.set_selectable(true);
        guest_password_label.add_css_class("monospace");
        guest_password_label.add_css_class("dim-label");

        let guest_actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let generate_guest_button = gtk4::Button::builder()
            .label("Generate")
            .css_classes(vec!["flat".to_string()])
            .build();
        let clear_guest_button = gtk4::Button::builder()
            .label("Clear")
            .css_classes(vec!["flat".to_string()])
            .build();
        let copy_guest_button = gtk4::Button::builder()
            .label("Copy")
            .css_classes(vec!["flat".to_string()])
            .build();
        guest_actions.append(&generate_guest_button);
        guest_actions.append(&copy_guest_button);
        guest_actions.append(&clear_guest_button);
        guest_password_row.add_suffix(&guest_password_label);
        guest_password_row.add_suffix(&guest_actions);

        let length_row = adw::ActionRow::builder().title("Password length").build();
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

        let strength_row = adw::ActionRow::builder().title("Password strength").build();
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

        // Advanced settings
        // * Keep predefined hotspot bands while still supporting a custom path.
        let band_model = gtk4::StringList::new(&["2.4 GHz", "5 GHz", "Auto", "Custom"][..]);
        let band_combo = adw::ComboRow::builder()
            .title("Frequency Band")
            .model(&band_model)
            .selected(2) // Auto by default
            .build();
        // * Allow a free-text band value when Custom is selected.
        let custom_band_entry = adw::EntryRow::builder().title("Custom Band").build();
        custom_band_entry.set_visible(false);

        // * Allow a free-text channel value when Custom is selected.
        let channel_entry = adw::EntryRow::builder().title("Channel").build();
        channel_entry.set_text("Auto");
        channel_entry.set_visible(false);

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
        config_group.add(&guest_password_row);
        config_group.add(&band_combo);
        config_group.add(&custom_band_entry);
        config_group.add(&channel_entry);
        config_group.add(&hidden_switch);
        config_group.add(&interface_combo);

        content.append(&config_group);

        let advanced_group = adw::PreferencesGroup::builder()
            .title("Advanced Hotspot Controls")
            .margin_top(12)
            .build();

        let download_limit_row = adw::ActionRow::builder()
            .title("Download limit")
            .subtitle("kbit/s, optional")
            .build();
        let download_limit_adjustment =
            gtk4::Adjustment::new(0.0, 0.0, 1_000_000.0, 100.0, 1000.0, 0.0);
        let download_limit_spin = gtk4::SpinButton::builder()
            .adjustment(&download_limit_adjustment)
            .numeric(true)
            .digits(0)
            .build();
        download_limit_row.add_suffix(&download_limit_spin);

        let upload_limit_row = adw::ActionRow::builder()
            .title("Upload limit")
            .subtitle("kbit/s, optional")
            .build();
        let upload_limit_adjustment =
            gtk4::Adjustment::new(0.0, 0.0, 1_000_000.0, 100.0, 1000.0, 0.0);
        let upload_limit_spin = gtk4::SpinButton::builder()
            .adjustment(&upload_limit_adjustment)
            .numeric(true)
            .digits(0)
            .build();
        upload_limit_row.add_suffix(&upload_limit_spin);

        let device_limit_row = adw::ActionRow::builder()
            .title("Maximum connected devices")
            .subtitle("Optional. Extra devices are blocked on the next policy refresh.")
            .build();
        let device_limit_adjustment = gtk4::Adjustment::new(0.0, 0.0, 256.0, 1.0, 5.0, 0.0);
        let device_limit_spin = gtk4::SpinButton::builder()
            .adjustment(&device_limit_adjustment)
            .numeric(true)
            .digits(0)
            .build();
        device_limit_row.add_suffix(&device_limit_spin);

        let mac_filter_model = gtk4::StringList::new(&["Disabled", "Allowlist", "Blocklist"]);
        let mac_filter_combo = adw::ComboRow::builder()
            .title("MAC filtering")
            .subtitle("Allow or block devices by MAC address")
            .model(&mac_filter_model)
            .build();

        let client_rules_button = gtk4::Button::builder()
            .label("Edit rules")
            .css_classes(vec!["flat".to_string()])
            .build();
        let client_rules_row = adw::ActionRow::builder()
            .title("Per-device rules")
            .subtitle("No device-specific hotspot rules")
            .build();
        client_rules_row.add_suffix(&client_rules_button);
        client_rules_row.set_activatable_widget(Some(&client_rules_button));

        let advanced_support_row = adw::ActionRow::builder()
            .title("System support")
            .subtitle("Checking for tc and nftables...")
            .build();

        advanced_group.add(&download_limit_row);
        advanced_group.add(&upload_limit_row);
        advanced_group.add(&device_limit_row);
        advanced_group.add(&mac_filter_combo);
        advanced_group.add(&client_rules_row);
        advanced_group.add(&advanced_support_row);
        content.append(&advanced_group);

        clamp.set_child(Some(&content));
        scrolled.set_child(Some(&clamp));
        toast_overlay.set_child(Some(&scrolled));
        widget.append(&toast_overlay);

        let devices = Rc::new(RefCell::new(Vec::new()));
        let is_active = Rc::new(Cell::new(false));
        let wifi_present = Rc::new(Cell::new(false));
        let wifi_enabled = Rc::new(Cell::new(false));
        let operation_in_progress = Rc::new(Cell::new(false));
        let config_dirty = Rc::new(Cell::new(false));
        let client_rules = Rc::new(RefCell::new(Vec::new()));
        let temporary_password = Rc::new(RefCell::new(hotspot::load_temporary_password()));
        let config_update_source = Rc::new(RefCell::new(None));
        let status_refresh_source = Rc::new(RefCell::new(None));
        let interface_refresh_source = Rc::new(RefCell::new(None));
        let suppress_config_updates = Rc::new(Cell::new(0));
        let password_adjusting = Rc::new(Cell::new(false));
        let reveal_timeout_generation = Rc::new(Cell::new(0u64));
        let password_was_set = Rc::new(Cell::new(!password_entry.text().is_empty()));

        let page = Self {
            widget,
            toast_overlay,
            apply_button: apply_button.clone(),
            hotspot_switch: hotspot_switch.clone(),
            ssid_entry: ssid_entry.clone(),
            password_entry: password_entry.clone(),
            band_combo: band_combo.clone(),
            custom_band_entry: custom_band_entry.clone(),
            channel_entry: channel_entry.clone(),
            hidden_switch: hidden_switch.clone(),
            interface_combo: interface_combo.clone(),
            config_group: config_group.clone(),
            advanced_group: advanced_group.clone(),
            upload_limit_spin: upload_limit_spin.clone(),
            download_limit_spin: download_limit_spin.clone(),
            device_limit_spin: device_limit_spin.clone(),
            mac_filter_combo: mac_filter_combo.clone(),
            client_rules_row: client_rules_row.clone(),
            client_rules_button: client_rules_button.clone(),
            advanced_support_row: advanced_support_row.clone(),
            qr_button: qr_button.clone(),
            guest_password_row: guest_password_row.clone(),
            guest_password_label: guest_password_label.clone(),
            status_label: status_label.clone(),
            operation_spinner: operation_spinner.clone(),
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
            app_state,
            operation_in_progress,
            config_dirty,
            client_rules,
            temporary_password,
            config_update_source,
            status_refresh_source,
            interface_refresh_source,
            suppress_config_updates,
        };

        page.set_wifi_state(false, false);
        page.update_guest_password_ui();
        page.set_config_dirty(false);
        // * Keep the custom band and channel rows aligned with the current selection on load.
        page.update_custom_band_channel_visibility();

        // Load configuration and check status
        let page_ref = page.clone();
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
            let password =
                crate::ui::hotspot_page::password::generate_password(len, include_symbols);
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
            let target = if len <= 0 {
                MIN_PASSWORD_LEN as i32
            } else {
                len
            };
            if target < MIN_PASSWORD_LEN as i32 {
                common::show_toast(
                    &toast_overlay_clone,
                    &format!("Length must be at least {} characters", MIN_PASSWORD_LEN),
                );
                return;
            }
            let target = if target as usize > MAX_PASSWORD_LEN {
                common::show_toast(&toast_overlay_clone, "Maximum length is 63 characters");
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

        let page_ref = page.clone();
        let length_spin_for_guest = length_spin.clone();
        generate_guest_button.connect_clicked(move |_| {
            let len = length_spin_for_guest
                .value_as_int()
                .max(MIN_PASSWORD_LEN as i32) as usize;
            let password = crate::ui::hotspot_page::password::generate_password(len, true);
            if let Ok(mut temporary_password) = page_ref.temporary_password.try_borrow_mut() {
                *temporary_password = Some(password);
            }
            page_ref.update_guest_password_ui();
            page_ref.set_config_dirty(true);
        });

        let page_ref = page.clone();
        clear_guest_button.connect_clicked(move |_| {
            if let Ok(mut temporary_password) = page_ref.temporary_password.try_borrow_mut() {
                temporary_password.take();
            }
            page_ref.update_guest_password_ui();
            page_ref.set_config_dirty(true);
        });

        let page_ref = page.clone();
        copy_guest_button.connect_clicked(move |_| {
            if let Some(password) = page_ref.current_temporary_password() {
                if let Some(display) = gtk4::gdk::Display::default() {
                    display.clipboard().set_text(&password);
                }
                page_ref.show_toast("Temporary guest password copied");
            } else {
                page_ref.show_toast("Generate a temporary guest password first");
            }
        });

        // Reveal password toggle
        let revealed_row_clone = revealed_password_row.clone();
        let reveal_switch_clone = reveal_switch.clone();
        let reveal_timeout_generation_clone = reveal_timeout_generation.clone();
        reveal_switch.connect_active_notify(move |row| {
            let active = row.is_active();
            revealed_row_clone.set_visible(active);

            let generation = reveal_timeout_generation_clone.get().wrapping_add(1);
            reveal_timeout_generation_clone.set(generation);
            if !active {
                return;
            }

            let reveal_switch_timeout = reveal_switch_clone.clone();
            let revealed_row_timeout = revealed_row_clone.clone();
            let reveal_generation_timeout = reveal_timeout_generation_clone.clone();
            glib::timeout_add_local(std::time::Duration::from_secs(30), move || {
                if reveal_generation_timeout.get() != generation {
                    return glib::ControlFlow::Break;
                }
                if reveal_switch_timeout.is_active() {
                    reveal_switch_timeout.set_active(false);
                    revealed_row_timeout.set_visible(false);
                }
                glib::ControlFlow::Break
            });
        });

        // Keep revealed password in sync, update strength, and auto-save when inactive
        let revealed_label_clone = revealed_password_label.clone();
        let strength_label_clone = strength_label.clone();
        let strength_bar_clone = strength_bar.clone();
        let page_ref = page.clone();
        let password_adjusting = password_adjusting.clone();
        let password_was_set = password_was_set.clone();
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
            let was_set = password_was_set.get();
            let is_empty = value.is_empty();
            if was_set && is_empty {
                let page = page_ref.clone();
                page.show_toast("Open network – not secure!");
            }
            password_was_set.set(!is_empty);
            page_ref.schedule_configuration_update();
        });

        let page_ref = page.clone();
        ssid_entry.connect_changed(move |_| {
            page_ref.schedule_configuration_update();
        });

        let page_ref = page.clone();
        band_combo.connect_selected_notify(move |_| {
            // * Toggle the free-text band and channel rows when Custom is selected.
            page_ref.update_custom_band_channel_visibility();
            page_ref.schedule_configuration_update();
        });

        let page_ref = page.clone();
        custom_band_entry.connect_changed(move |_| {
            page_ref.schedule_configuration_update();
        });

        let page_ref = page.clone();
        channel_entry.connect_changed(move |_| {
            page_ref.schedule_configuration_update();
        });

        let page_ref = page.clone();
        hidden_switch.connect_active_notify(move |_| {
            page_ref.schedule_configuration_update();
        });

        let page_ref = page.clone();
        download_limit_spin.connect_value_changed(move |_| {
            page_ref.schedule_configuration_update();
        });

        let page_ref = page.clone();
        upload_limit_spin.connect_value_changed(move |_| {
            page_ref.schedule_configuration_update();
        });

        let page_ref = page.clone();
        mac_filter_combo.connect_selected_notify(move |_| {
            page_ref.schedule_configuration_update();
        });

        let page_ref = page.clone();
        client_rules_button.connect_clicked(move |_| {
            let page = page_ref.clone();
            glib::spawn_future_local(async move {
                page.edit_client_rules().await;
            });
        });

        let page_ref = page.clone();
        interface_combo.connect_selected_notify(move |_| {
            page_ref.schedule_configuration_update();
        });

        let page_ref = page.clone();
        apply_button.connect_clicked(move |_| {
            let page = page_ref.clone();
            glib::spawn_future_local(async move {
                page.apply_changes().await;
            });
        });

        // Hotspot switch handler - kept for compatibility
        let page_ref = page.clone();
        hotspot_switch.connect_active_notify(move |switch| {
            let page = page_ref.clone();
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
        let page_ref = page.clone();
        qr_button.connect_clicked(move |_| {
            let page = page_ref.clone();
            glib::spawn_future_local(async move {
                page.show_qr().await;
            });
        });

        page.set_page_visible(false);
        page
    }

    pub fn set_page_visible(&self, visible: bool) {
        self.app_state.set_page_visible(PageKind::Hotspot, visible);
        if visible {
            self.start_auto_refresh();
            let page = self.clone();
            glib::spawn_future_local(async move {
                page.load_interfaces().await;
                page.refresh_status().await;
            });
        } else {
            self.stop_auto_refresh();
        }
    }

    fn start_auto_refresh(&self) {
        if self.status_refresh_source.borrow().is_none() {
            let page_ref = self.clone();
            let source = glib::timeout_add_seconds_local(5, move || {
                if !page_ref.app_state.is_page_visible(PageKind::Hotspot) {
                    return glib::ControlFlow::Continue;
                }
                if page_ref.is_active.get() {
                    page_ref.refresh_status_details();
                }
                glib::ControlFlow::Continue
            });
            *self.status_refresh_source.borrow_mut() = Some(source);
        }

        if self.interface_refresh_source.borrow().is_none() {
            let page_ref = self.clone();
            let source = glib::timeout_add_seconds_local(3, move || {
                if !page_ref.app_state.is_page_visible(PageKind::Hotspot) {
                    return glib::ControlFlow::Continue;
                }
                let page = page_ref.clone();
                glib::spawn_future_local(async move {
                    page.load_interfaces().await;
                    page.refresh_status().await;
                });
                glib::ControlFlow::Continue
            });
            *self.interface_refresh_source.borrow_mut() = Some(source);
        }
    }

    fn stop_auto_refresh(&self) {
        if let Some(source) = self.status_refresh_source.borrow_mut().take() {
            source.remove();
        }
        if let Some(source) = self.interface_refresh_source.borrow_mut().take() {
            source.remove();
        }
    }

    fn set_operation_state(&self, active: bool, status: &str) {
        if active {
            common::set_busy(
                &self.operation_spinner,
                &self.status_label,
                None,
                true,
                Some(status),
            );
            return;
        }

        common::set_busy(
            &self.operation_spinner,
            &self.status_label,
            None,
            false,
            None,
        );
        self.status_label.set_visible(true);
        self.apply_button
            .set_sensitive(self.config_dirty.get() && !self.operation_in_progress.get());
        self.update_ui();
    }

    fn update_custom_band_channel_visibility(&self) {
        // * Expose editable band and channel controls only when Custom band is chosen.
        let custom_selected = is_custom_band_selected(self.band_combo.selected());
        self.custom_band_entry.set_visible(custom_selected);
        self.channel_entry.set_visible(custom_selected);
    }

    fn current_temporary_password(&self) -> Option<String> {
        self.temporary_password
            .borrow()
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    }

    fn update_guest_password_ui(&self) {
        let temporary_password = self.current_temporary_password();
        match temporary_password {
            Some(password) => {
                self.guest_password_label.set_text(&password);
                self.guest_password_label.remove_css_class("dim-label");
                self.guest_password_row.set_subtitle(
                    "Temporarily replaces the main hotspot password until the next hotspot shutdown",
                );
            }
            None => {
                self.guest_password_label.set_text("Not generated");
                self.guest_password_label.add_css_class("dim-label");
                self.guest_password_row.set_subtitle(
                    "Generate a temporary password when you want to share access for one hotspot session",
                );
            }
        }
    }

    fn set_config_dirty(&self, dirty: bool) {
        self.config_dirty.set(dirty);
        let can_apply = dirty && !self.operation_in_progress.get();
        self.apply_button.set_sensitive(can_apply);
        self.apply_button.set_label(if dirty {
            "Apply Changes"
        } else {
            "Changes Applied"
        });
    }

    fn schedule_configuration_update(&self) {
        if self.suppress_config_updates.get() > 0 {
            return;
        }
        self.set_config_dirty(true);
    }

    fn with_suppressed_config_updates<F>(&self, update: F)
    where
        F: FnOnce(),
    {
        let depth = self.suppress_config_updates.get();
        self.suppress_config_updates.set(depth.saturating_add(1));
        update();
        self.suppress_config_updates.set(depth);
    }

    async fn apply_changes(&self) {
        if self.operation_in_progress.get() {
            return;
        }

        self.operation_in_progress.set(true);
        self.set_operation_state(true, "Applying hotspot changes...");

        let storage = self.load_password_storage();
        let config = self.build_hotspot_config(self.password_entry.text().to_string());
        let temporary_password = self.current_temporary_password();

        if let Some(guest_password) = temporary_password.as_deref() {
            let guest_validation = HotspotConfig {
                password: guest_password.to_string(),
                ..config.clone()
            };
            if let Err(e) = guest_validation.validate_password() {
                self.show_toast(&format!("Invalid temporary guest password: {}", e));
                self.operation_in_progress.set(false);
                self.set_operation_state(false, "");
                return;
            }
        }

        if let Err(message) = self.validate_channel_on_apply(&config) {
            self.show_toast(&message);
            self.operation_in_progress.set(false);
            self.set_operation_state(false, "");
            return;
        }
        if let Err(e) = config.validate() {
            self.show_toast(&format!("Invalid configuration: {}", e));
            self.operation_in_progress.set(false);
            self.set_operation_state(false, "");
            return;
        }

        hotspot::store_temporary_password(temporary_password.as_deref());

        if !self.persist_configuration(&config, &storage, true) {
            self.operation_in_progress.set(false);
            self.set_operation_state(false, "");
            return;
        }
        if self.is_active.get() {
            self.restart_hotspot_with_config(&config).await;
        } else {
            self.set_config_dirty(false);
            self.show_toast(if temporary_password.is_some() {
                "Hotspot settings saved. The temporary guest password will apply on next start"
            } else {
                "Hotspot settings saved"
            });
            self.operation_in_progress.set(false);
            self.set_operation_state(false, "");
        }
    }

    fn current_interface_name(&self) -> String {
        let iface_idx = self.interface_combo.selected() as usize;
        let devices = self.devices.borrow();
        devices
            .get(iface_idx)
            .cloned()
            .unwrap_or_else(|| "wlan0".to_string())
    }

    fn persist_configuration(
        &self,
        config: &HotspotConfig,
        storage: &HotspotPasswordStorage,
        show_errors: bool,
    ) -> bool {
        let config_storage = self.persist_password_with_fallback(storage, &config.password);
        let config_to_save = Self::config_for_storage(config, &config_storage);
        match config::save_config(&config::hotspot_config_path(), &config_to_save) {
            Ok(_) => true,
            Err(e) => {
                log::error!("Failed to save configuration: {}", e);
                if show_errors {
                    self.show_toast(&format!("Failed to save configuration: {}", e));
                }
                false
            }
        }
    }

    async fn restart_hotspot_with_config(&self, config: &HotspotConfig) {
        let interface = self.current_interface_name();

        self.operation_in_progress.set(true);
        self.set_operation_state(true, "Applying hotspot changes...");

        let stop_result = hotspot::stop_hotspot().await;
        if let Err(e) = stop_result {
            log::error!("Failed to stop hotspot during update: {}", e);
            if nm::is_nmcli_retrieval_error(&e.to_string()) {
                self.show_toast(nm::NMCLI_RETRIEVAL_TOAST);
            } else {
                self.show_toast(&format!("Failed to update hotspot: {}", e));
            }
            self.operation_in_progress.set(false);
            self.set_operation_state(false, "");
            self.refresh_status().await;
            return;
        }

        match hotspot::create_hotspot_on(config, &interface).await {
            Ok(_) => {
                self.is_active.set(true);
                self.hotspot_switch.set_active(true);
                self.set_config_dirty(false);
                self.show_toast("Hotspot updated");
            }
            Err(e) => {
                log::error!("Failed to restart hotspot: {}", e);
                let error_text = e.to_string();
                if nm::is_nmcli_retrieval_error(&error_text) {
                    self.show_toast(nm::NMCLI_RETRIEVAL_TOAST);
                } else if hotspot::is_hotspot_mode_not_supported_error(&error_text) {
                    self.show_toast(hotspot::HOTSPOT_UNSUPPORTED_TOAST);
                } else {
                    self.show_toast(&format!(
                        "Failed to restart hotspot on {}: {}",
                        interface, error_text
                    ));
                }
                self.is_active.set(false);
                self.hotspot_switch.set_active(false);
            }
        }

        self.operation_in_progress.set(false);
        self.set_operation_state(false, "");
        self.refresh_status().await;
    }

    fn build_hotspot_config(&self, password: String) -> HotspotConfig {
        let selected_band =
            band_from_selected(self.band_combo.selected(), &self.custom_band_entry.text());
        let channel = if is_custom_band_selected(self.band_combo.selected()) {
            let trimmed = self.channel_entry.text().trim().to_string();
            if trimmed.is_empty() {
                "Auto".to_string()
            } else {
                trimmed
            }
        } else {
            "Auto".to_string()
        };

        HotspotConfig {
            ssid: self.ssid_entry.text().to_string(),
            password,
            band: selected_band,
            channel,
            hidden: self.hidden_switch.is_active(),
            upload_limit_kbps: spin_value_to_option(&self.upload_limit_spin),
            download_limit_kbps: spin_value_to_option(&self.download_limit_spin),
            max_connected_devices: spin_value_to_option(&self.device_limit_spin),
            mac_filter_mode: mac_filter_mode_from_selection(self.mac_filter_combo.selected()),
            client_rules: self.client_rules.borrow().clone(),
        }
    }

    fn validate_channel_on_apply(&self, config: &HotspotConfig) -> Result<(), String> {
        // * Reject obviously invalid custom channels before applying hotspot settings.
        if !is_custom_band_selected(self.band_combo.selected()) {
            return Ok(());
        }

        if self.custom_band_entry.text().trim().is_empty() {
            // * Prevent applying Custom mode without an explicit band value.
            return Err("Custom band cannot be empty".to_string());
        }

        let channel = config.channel.trim();
        if channel.eq_ignore_ascii_case("auto") {
            return Ok(());
        }
        if !channel.chars().all(|c| c.is_ascii_digit()) {
            return Err("Channel must be a number or Auto".to_string());
        }

        let value = channel
            .parse::<u16>()
            .map_err(|_| "Channel must be a number or Auto".to_string())?;
        if value == 0 || value > 233 {
            return Err("Channel must be between 1 and 233 or Auto".to_string());
        }

        Ok(())
    }

    async fn load_config(&self) {
        let storage = self.load_password_storage();
        match config::load_config(&config::hotspot_config_path()) {
            Ok(config) => {
                let password = self
                    .resolve_password_for_storage(&storage, Some(&config))
                    .await;
                self.with_suppressed_config_updates(|| {
                    self.ssid_entry.set_text(&config.ssid);
                    self.password_entry.set_text(&password);
                    self.revealed_password_label.set_text(&password);
                    update_strength_indicator(&password, &self.strength_label, &self.strength_bar);
                    // * Restore custom band text when the stored value is outside the predefined list.
                    let (band_index, custom_band) = band_to_selection(&config.band);
                    self.band_combo.set_selected(band_index);
                    self.custom_band_entry.set_text(&custom_band);
                    self.channel_entry.set_text(&config.channel);
                    self.update_custom_band_channel_visibility();
                    self.hidden_switch.set_active(config.hidden);
                    self.download_limit_spin
                        .set_value(config.download_limit_kbps.unwrap_or_default() as f64);
                    self.upload_limit_spin
                        .set_value(config.upload_limit_kbps.unwrap_or_default() as f64);
                    self.device_limit_spin
                        .set_value(config.max_connected_devices.unwrap_or_default() as f64);
                    self.mac_filter_combo
                        .set_selected(selection_from_mac_filter_mode(&config.mac_filter_mode));
                });
                if let Ok(mut temporary_password) = self.temporary_password.try_borrow_mut() {
                    *temporary_password = hotspot::load_temporary_password();
                }
                self.update_guest_password_ui();
                if let Ok(mut rules) = self.client_rules.try_borrow_mut() {
                    *rules = config.client_rules.clone();
                }
                self.update_client_rules_summary();
                self.set_config_dirty(false);
            }
            Err(_) => {
                let config = HotspotConfig::default();
                let password = self.resolve_password_for_storage(&storage, None).await;
                self.with_suppressed_config_updates(|| {
                    self.ssid_entry.set_text(&config.ssid);
                    self.password_entry.set_text(&password);
                    self.revealed_password_label.set_text(&password);
                    update_strength_indicator(&password, &self.strength_label, &self.strength_bar);
                    self.band_combo.set_selected(2);
                    self.custom_band_entry.set_text("");
                    self.channel_entry.set_text(&config.channel);
                    self.update_custom_band_channel_visibility();
                    self.hidden_switch.set_active(false);
                    self.download_limit_spin.set_value(0.0);
                    self.upload_limit_spin.set_value(0.0);
                    self.device_limit_spin.set_value(0.0);
                    self.mac_filter_combo.set_selected(0);
                });
                if let Ok(mut temporary_password) = self.temporary_password.try_borrow_mut() {
                    *temporary_password = hotspot::load_temporary_password();
                }
                self.update_guest_password_ui();
                if let Ok(mut rules) = self.client_rules.try_borrow_mut() {
                    rules.clear();
                }
                self.update_client_rules_summary();
                self.set_config_dirty(false);
            }
        }
        self.refresh_advanced_support().await;
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
        self.set_operation_state(true, "Starting hotspot...");

        // First save the configuration
        let storage = self.load_password_storage();
        if storage == HotspotPasswordStorage::PlainJson && !self.confirm_plain_json_usage().await {
            self.operation_in_progress.set(false);
            self.set_operation_state(false, "");
            self.hotspot_switch.set_active(false);
            return;
        }
        let mut password = self.password_entry.text().to_string();
        if password.is_empty() {
            let resolved = self.resolve_password_for_storage(&storage, None).await;
            if !resolved.is_empty() {
                password = resolved;
                self.with_suppressed_config_updates(|| {
                    self.password_entry.set_text(&password);
                    self.revealed_password_label.set_text(&password);
                    update_strength_indicator(&password, &self.strength_label, &self.strength_bar);
                });
            }
        }

        // * Build the hotspot config from either the predefined or custom band and channel inputs.
        let config = self.build_hotspot_config(password);
        let temporary_password = self.current_temporary_password();

        if let Some(guest_password) = temporary_password.as_deref() {
            let guest_validation = HotspotConfig {
                password: guest_password.to_string(),
                ..config.clone()
            };
            if let Err(e) = guest_validation.validate_password() {
                self.show_toast(&format!("Invalid temporary guest password: {}", e));
                self.hotspot_switch.set_active(false);
                self.operation_in_progress.set(false);
                self.set_operation_state(false, "");
                self.update_ui();
                return;
            }
        }

        if let Err(message) = self.validate_channel_on_apply(&config) {
            self.show_toast(&message);
            self.hotspot_switch.set_active(false);
            self.operation_in_progress.set(false);
            self.set_operation_state(false, "");
            self.update_ui();
            return;
        }

        if let Err(e) = config.validate() {
            self.show_toast(&format!("Invalid configuration: {}", e));
            self.hotspot_switch.set_active(false);
            self.operation_in_progress.set(false);
            self.set_operation_state(false, "");
            self.update_ui();
            return;
        }

        let interface = self.current_interface_name();
        hotspot::store_temporary_password(temporary_password.as_deref());

        match hotspot::create_hotspot_on(&config, &interface).await {
            Ok(_) => {
                let _ = self.persist_configuration(&config, &storage, true);
                self.is_active.set(true);
                self.set_config_dirty(false);
                self.show_toast("Hotspot started successfully");
            }
            Err(e) => {
                log::error!("Failed to start hotspot: {}", e);
                let error_text = e.to_string();
                if nm::is_nmcli_retrieval_error(&error_text) {
                    self.show_toast(nm::NMCLI_RETRIEVAL_TOAST);
                } else if hotspot::is_hotspot_mode_not_supported_error(&error_text) {
                    // * Show a specific unsupported-adapter message for hotspot mode.
                    self.show_toast(hotspot::HOTSPOT_UNSUPPORTED_TOAST);
                } else {
                    // * Keep hotspot start failures contextual instead of generic.
                    self.show_toast(&format!(
                        "Failed to start hotspot on {}: {}",
                        interface, error_text
                    ));
                }
                self.is_active.set(false);
                self.hotspot_switch.set_active(false);
                hotspot::store_temporary_password(None);
            }
        }

        self.operation_in_progress.set(false);
        self.set_operation_state(false, "");
        self.update_ui();
    }

    async fn stop_hotspot(&self) {
        if self.operation_in_progress.get() {
            return;
        }

        self.operation_in_progress.set(true);
        self.set_operation_state(true, "Stopping hotspot...");

        match hotspot::stop_hotspot().await {
            Ok(_) => {
                self.is_active.set(false);
                self.show_toast("Hotspot stopped");
            }
            Err(e) => {
                log::error!("Failed to stop hotspot: {}", e);
                if nm::is_nmcli_retrieval_error(&e.to_string()) {
                    self.show_toast(nm::NMCLI_RETRIEVAL_TOAST);
                } else {
                    self.show_toast(&format!("Failed to stop hotspot: {}", e));
                }

                // Even if stop failed, try to recover state
                if let Ok(active) = hotspot::is_hotspot_active().await {
                    self.is_active.set(active);
                    self.hotspot_switch.set_active(active);
                }
            }
        }

        self.operation_in_progress.set(false);
        self.set_operation_state(false, "");
        self.update_ui();
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
                if nm::is_nmcli_retrieval_error(&e.to_string()) {
                    self.show_toast(nm::NMCLI_RETRIEVAL_TOAST);
                }
                self.is_active.set(false);
                self.hotspot_switch.set_active(false);
                self.update_ui();
            }
        }
    }

    async fn load_interfaces(&self) {
        let present = nm::is_wifi_present().await.unwrap_or(false);
        let enabled = nm::is_wifi_enabled().await.unwrap_or(false);
        let previous_iface = {
            let selected_idx = self.interface_combo.selected() as usize;
            self.devices.borrow().get(selected_idx).cloned()
        };

        match hotspot::get_wifi_devices().await {
            Ok(mut ifaces) if !ifaces.is_empty() => {
                ifaces.sort();
                debug_assert!(
                    self.devices.try_borrow_mut().is_ok(),
                    "Shared state borrow conflict: hotspot_devices_set"
                );
                if let Ok(mut devices) = self.devices.try_borrow_mut() {
                    *devices = ifaces.clone();
                } else {
                    log::error!("Borrow conflict in UI state");
                    return;
                }
                let model =
                    gtk4::StringList::new(&ifaces.iter().map(|s| s.as_str()).collect::<Vec<_>>());
                let selected_idx = previous_iface
                    .as_ref()
                    .and_then(|iface| ifaces.iter().position(|item| item == iface))
                    .unwrap_or(0);
                self.with_suppressed_config_updates(|| {
                    self.interface_combo.set_model(Some(&model));
                    self.interface_combo.set_selected(selected_idx as u32);
                });
                self.set_wifi_state(present, enabled);
                log::info!(
                    "Loaded {} WiFi interfaces, selected: {}",
                    ifaces.len(),
                    &ifaces[selected_idx]
                );
            }
            Ok(_) => {
                debug_assert!(
                    self.devices.try_borrow_mut().is_ok(),
                    "Shared state borrow conflict: hotspot_devices_clear"
                );
                if let Ok(mut devices) = self.devices.try_borrow_mut() {
                    *devices = Vec::new();
                } else {
                    log::error!("Borrow conflict in UI state");
                    return;
                }
                let empty_model = gtk4::StringList::new(&[][..]);
                self.with_suppressed_config_updates(|| {
                    self.interface_combo.set_model(Some(&empty_model));
                });
                self.set_wifi_state(present, enabled);
            }
            Err(e) => {
                log::error!("Failed to load hotspot interfaces: {}", e);
                debug_assert!(
                    self.devices.try_borrow_mut().is_ok(),
                    "Shared state borrow conflict: hotspot_devices_error"
                );
                if let Ok(mut devices) = self.devices.try_borrow_mut() {
                    devices.clear();
                } else {
                    log::error!("Borrow conflict in UI state");
                    return;
                }
                let empty_model = gtk4::StringList::new(&[][..]);
                self.with_suppressed_config_updates(|| {
                    self.interface_combo.set_model(Some(&empty_model));
                });
                self.set_wifi_state(present, enabled);
            }
        }
    }

    fn set_wifi_state(&self, present: bool, enabled: bool) {
        self.wifi_present.set(present);
        self.wifi_enabled.set(enabled);
        let controls_enabled = present && enabled && !self.operation_in_progress.get();
        self.hotspot_switch.set_sensitive(controls_enabled);
        self.config_group.set_sensitive(controls_enabled);
        self.advanced_group.set_sensitive(controls_enabled);
        self.interface_combo
            .set_sensitive(present && !self.operation_in_progress.get());
        self.qr_button
            .set_visible(present && enabled && self.is_active.get());

        if !present {
            debug_assert!(
                self.devices.try_borrow_mut().is_ok(),
                "Shared state borrow conflict: hotspot_devices_no_present"
            );
            if let Ok(mut devices) = self.devices.try_borrow_mut() {
                devices.clear();
            } else {
                log::error!("Borrow conflict in UI state");
                return;
            }
            let empty_model = gtk4::StringList::new(&[][..]);
            self.interface_combo.set_model(Some(&empty_model));
            self.hotspot_switch.set_active(false);
        }

        self.update_ui();
    }

    fn update_ui(&self) {
        self.apply_button
            .set_sensitive(self.config_dirty.get() && !self.operation_in_progress.get());
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
        let guest_password_active = self.current_temporary_password().is_some();

        glib::spawn_future_local(async move {
            hotspot::sync_runtime_rules_from_disk().await.ok();
            let count_info = hotspot::get_connected_device_count_info().await.unwrap_or(
                hotspot::ConnectedClientCountInfo {
                    count: 0,
                    estimated: false,
                },
            );
            let device_text = match count_info.count {
                0 => "No devices connected".to_string(),
                1 => "1 device connected".to_string(),
                _ => format!("{} devices connected", count_info.count),
            };
            let confidence = if count_info.estimated {
                // * Mark the hotspot client count as estimated when sources diverge.
                "estimated"
            } else {
                "approximate"
            };
            status_subtitle.set_text(&format!("{} • {} ({})", ssid, device_text, confidence));

            let ip = hotspot::get_hotspot_ip().await.ok().flatten();
            let mut meta_parts = match ip {
                Some(ip) => vec![
                    format!("Share internet from: {}", iface),
                    format!("Hotspot IP: {}", ip),
                ],
                std::prelude::v1::None => vec![format!("Share internet from: {}", iface)],
            };
            if guest_password_active {
                meta_parts.push("Temporary guest password active".to_string());
            }
            let meta = meta_parts.join(" • ");
            status_meta.set_text(&meta);
        });
    }

    async fn show_qr(&self) {
        let ssid = self.ssid_entry.text().to_string();
        let storage = self.load_password_storage();
        let password = self
            .current_temporary_password()
            .unwrap_or(self.resolve_password_for_storage(&storage, None).await);

        qr_dialog::show_qr_dialog(&ssid, &password, None, 200, &self.toast_overlay).await;
    }

    fn show_toast(&self, message: &str) {
        common::show_toast(&self.toast_overlay, message);
    }

    async fn confirm_plain_json_usage(&self) -> bool {
        let dialog = adw::AlertDialog::builder()
            .heading("Confirm insecure password storage")
            .body("Plain JSON stores your hotspot password in clear text on disk. Continue only for debugging.")
            .default_response("cancel")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("continue", "Continue (debug only)")][..]);
        dialog.set_response_appearance("continue", adw::ResponseAppearance::Destructive);

        let response = if let Some(parent) = self.widget.root().and_downcast::<gtk4::Window>() {
            dialog.choose_future(Some(&parent)).await
        } else {
            dialog.choose_future(None::<&gtk4::Window>).await
        };

        response == "continue"
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
                config.map(|c| c.password.clone()).unwrap_or_default()
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

    fn persist_password_with_fallback(
        &self,
        storage: &HotspotPasswordStorage,
        password: &str,
    ) -> HotspotPasswordStorage {
        match self.persist_password_for_storage(storage, password) {
            Ok(()) => storage.clone(),
            Err(e) => {
                log::error!("Failed to store hotspot password: {}", e);
                self.show_toast("Could not access secure storage; password was not updated");
                storage.clone()
            }
        }
    }

    fn config_for_storage(
        config: &HotspotConfig,
        storage: &HotspotPasswordStorage,
    ) -> HotspotConfig {
        let mut to_save = config.clone();
        match storage {
            HotspotPasswordStorage::PlainJson => {}
            HotspotPasswordStorage::Keyring | HotspotPasswordStorage::NetworkManager => {
                to_save.password.clear();
            }
        }
        to_save
    }

    async fn refresh_advanced_support(&self) {
        let support = hotspot::advanced_support().await;
        if let Some(reason) = support.missing_reason() {
            self.advanced_support_row.set_subtitle(&reason);
        } else {
            self.advanced_support_row
                .set_subtitle("tc and nftables are available");
        }
        self.download_limit_spin.set_sensitive(support.tc_available);
        self.upload_limit_spin.set_sensitive(support.tc_available);
        self.device_limit_spin.set_sensitive(support.nft_available);
        self.mac_filter_combo.set_sensitive(support.nft_available);
        self.client_rules_button
            .set_sensitive(support.tc_available || support.nft_available);
    }

    fn update_client_rules_summary(&self) {
        let rules = self.client_rules.borrow();
        let subtitle = if rules.is_empty() {
            "No device-specific hotspot rules".to_string()
        } else {
            format!("{} device rule(s) configured", rules.len())
        };
        self.client_rules_row.set_subtitle(&subtitle);
    }

    async fn edit_client_rules(&self) {
        let initial_rules = self.client_rules.borrow().clone();
        let rules_state = Rc::new(RefCell::new(initial_rules.clone()));

        let list_box = gtk4::ListBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .css_classes(vec!["boxed-list".to_string()])
            .build();

        let add_button = gtk4::Button::builder()
            .label("Add rule")
            .css_classes(vec!["flat".to_string()])
            .build();

        repopulate_client_rule_rows(&list_box, &rules_state, &self.toast_overlay);

        let list_box_for_add = list_box.clone();
        let rules_state_for_add = rules_state.clone();
        let parent_widget = self.widget.clone();
        let toast_overlay = self.toast_overlay.clone();
        add_button.connect_clicked(move |_| {
            let list_box = list_box_for_add.clone();
            let rules_state = rules_state_for_add.clone();
            let parent_widget = parent_widget.clone();
            let toast_overlay = toast_overlay.clone();
            glib::spawn_future_local(async move {
                if let Ok(Some(rule)) =
                    show_single_client_rule_editor(parent_widget.upcast_ref(), None).await
                {
                    if let Ok(mut rules) = rules_state.try_borrow_mut() {
                        rules.push(rule);
                        rules.sort_by(|a, b| a.mac_address.cmp(&b.mac_address));
                    }
                    repopulate_client_rule_rows(&list_box, &rules_state, &toast_overlay);
                }
            });
        });

        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        body.set_margin_top(12);
        body.set_margin_bottom(12);
        body.set_margin_start(12);
        body.set_margin_end(12);
        body.append(&add_button);
        body.append(&list_box);

        let dialog = adw::AlertDialog::builder()
            .heading("Device rules")
            .body("Add optional per-device limits and MAC entries for allow/block lists.")
            .extra_child(&body)
            .default_response("save")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("save", "Save")]);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);

        let response = if let Some(parent) = self.widget.root().and_downcast_ref::<gtk4::Window>() {
            dialog.choose_future(Some(parent)).await
        } else {
            dialog.choose_future(None::<&gtk4::Window>).await
        };

        if response.as_str() != "save" {
            return;
        }

        if let Ok(mut rules) = self.client_rules.try_borrow_mut() {
            *rules = rules_state.borrow().clone();
        }
        self.update_client_rules_summary();
        self.schedule_configuration_update();
    }
}

fn spin_value_to_option(spin: &gtk4::SpinButton) -> Option<u32> {
    let value = spin.value_as_int();
    if value <= 0 {
        None
    } else {
        Some(value as u32)
    }
}

fn mac_filter_mode_from_selection(selected: u32) -> HotspotMacFilterMode {
    match selected {
        1 => HotspotMacFilterMode::Allowlist,
        2 => HotspotMacFilterMode::Blocklist,
        _ => HotspotMacFilterMode::Disabled,
    }
}

fn selection_from_mac_filter_mode(mode: &HotspotMacFilterMode) -> u32 {
    match mode {
        HotspotMacFilterMode::Disabled => 0,
        HotspotMacFilterMode::Allowlist => 1,
        HotspotMacFilterMode::Blocklist => 2,
    }
}

fn repopulate_client_rule_rows(
    list_box: &gtk4::ListBox,
    rules_state: &Rc<RefCell<Vec<HotspotClientRule>>>,
    toast_overlay: &adw::ToastOverlay,
) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    let rules_snapshot = rules_state.borrow().clone();
    for (index, rule) in rules_snapshot.into_iter().enumerate() {
        let mut subtitle_parts = vec![rule.mac_address.clone()];
        if rule.blocked {
            subtitle_parts.push("Blocked".to_string());
        }
        if let Some(limit) = rule.download_limit_kbps {
            subtitle_parts.push(format!("Down {} kbit/s", limit));
        }
        if let Some(limit) = rule.upload_limit_kbps {
            subtitle_parts.push(format!("Up {} kbit/s", limit));
        }
        if let Some(limit) = rule.time_limit_minutes {
            subtitle_parts.push(format!("{} min", limit));
        }
        if let Some(limit) = rule.download_quota_mb {
            subtitle_parts.push(format!("{} MB down", limit));
        }
        if let Some(limit) = rule.upload_quota_mb {
            subtitle_parts.push(format!("{} MB up", limit));
        }
        if !rule.blocked_domains.is_empty() {
            subtitle_parts.push(format!("{} blocked site(s)", rule.blocked_domains.len()));
        }
        let subtitle = subtitle_parts.join(" • ");
        let row = adw::ActionRow::builder()
            .title(rule.display_name.as_deref().unwrap_or(&rule.mac_address))
            .subtitle(&subtitle)
            .build();

        let edit_btn = gtk4::Button::builder()
            .label("Edit")
            .css_classes(vec!["flat".to_string()])
            .build();
        let delete_btn = gtk4::Button::builder()
            .label("Delete")
            .css_classes(vec!["flat".to_string(), "destructive-action".to_string()])
            .build();

        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        actions.append(&edit_btn);
        actions.append(&delete_btn);
        row.add_suffix(&actions);

        let list_box_for_edit = list_box.clone();
        let rules_state_for_edit = rules_state.clone();
        let toast_overlay_for_edit = toast_overlay.clone();
        edit_btn.connect_clicked(move |_| {
            let list_box = list_box_for_edit.clone();
            let rules_state = rules_state_for_edit.clone();
            let toast_overlay = toast_overlay_for_edit.clone();
            let existing = rules_state.borrow().get(index).cloned();
            glib::spawn_future_local(async move {
                match show_single_client_rule_editor(list_box.upcast_ref(), existing).await {
                    Ok(Some(rule)) => {
                        if let Ok(mut rules) = rules_state.try_borrow_mut() {
                            if index < rules.len() {
                                rules[index] = rule;
                            }
                            rules.sort_by(|a, b| a.mac_address.cmp(&b.mac_address));
                        }
                        repopulate_client_rule_rows(&list_box, &rules_state, &toast_overlay);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        common::show_toast(&toast_overlay, &format!("Failed to edit rule: {}", e))
                    }
                }
            });
        });

        let list_box_for_delete = list_box.clone();
        let rules_state_for_delete = rules_state.clone();
        let toast_overlay_for_delete = toast_overlay.clone();
        delete_btn.connect_clicked(move |_| {
            if let Ok(mut rules) = rules_state_for_delete.try_borrow_mut() {
                if index < rules.len() {
                    rules.remove(index);
                }
            }
            repopulate_client_rule_rows(
                &list_box_for_delete,
                &rules_state_for_delete,
                &toast_overlay_for_delete,
            );
        });

        list_box.append(&row);
    }
}

async fn show_single_client_rule_editor(
    parent_widget: &gtk4::Widget,
    existing: Option<HotspotClientRule>,
) -> anyhow::Result<Option<HotspotClientRule>> {
    let mac_entry = adw::EntryRow::builder().title("MAC address").build();
    let display_name_entry = adw::EntryRow::builder().title("Device name").build();
    let blocked_switch = adw::SwitchRow::builder()
        .title("Block this device")
        .subtitle("Drop hotspot traffic for this MAC address")
        .build();
    let download_row = adw::ActionRow::builder()
        .title("Download limit")
        .subtitle("kbit/s, optional")
        .build();
    let download_adjustment = gtk4::Adjustment::new(0.0, 0.0, 1_000_000.0, 100.0, 1000.0, 0.0);
    let download_spin = gtk4::SpinButton::builder()
        .adjustment(&download_adjustment)
        .numeric(true)
        .digits(0)
        .build();
    download_row.add_suffix(&download_spin);

    let upload_row = adw::ActionRow::builder()
        .title("Upload limit")
        .subtitle("kbit/s, optional")
        .build();
    let upload_adjustment = gtk4::Adjustment::new(0.0, 0.0, 1_000_000.0, 100.0, 1000.0, 0.0);
    let upload_spin = gtk4::SpinButton::builder()
        .adjustment(&upload_adjustment)
        .numeric(true)
        .digits(0)
        .build();
    upload_row.add_suffix(&upload_spin);

    let time_row = adw::ActionRow::builder()
        .title("Connected time limit")
        .subtitle("minutes in the current quota window, optional")
        .build();
    let time_adjustment = gtk4::Adjustment::new(0.0, 0.0, 100_000.0, 5.0, 60.0, 0.0);
    let time_spin = gtk4::SpinButton::builder()
        .adjustment(&time_adjustment)
        .numeric(true)
        .digits(0)
        .build();
    time_row.add_suffix(&time_spin);

    let download_quota_row = adw::ActionRow::builder()
        .title("Download quota")
        .subtitle("MB in the current quota window, optional")
        .build();
    let download_quota_adjustment = gtk4::Adjustment::new(0.0, 0.0, 1_000_000.0, 50.0, 500.0, 0.0);
    let download_quota_spin = gtk4::SpinButton::builder()
        .adjustment(&download_quota_adjustment)
        .numeric(true)
        .digits(0)
        .build();
    download_quota_row.add_suffix(&download_quota_spin);

    let upload_quota_row = adw::ActionRow::builder()
        .title("Upload quota")
        .subtitle("MB in the current quota window, optional")
        .build();
    let upload_quota_adjustment = gtk4::Adjustment::new(0.0, 0.0, 1_000_000.0, 50.0, 500.0, 0.0);
    let upload_quota_spin = gtk4::SpinButton::builder()
        .adjustment(&upload_quota_adjustment)
        .numeric(true)
        .digits(0)
        .build();
    upload_quota_row.add_suffix(&upload_quota_spin);

    let blocked_domains_entry = adw::EntryRow::builder().title("Blocked sites").build();
    blocked_domains_entry.set_text("");
    blocked_domains_entry.set_tooltip_text(Some(
        "Comma-separated domains, for example: youtube.com, instagram.com",
    ));

    if let Some(existing) = existing.as_ref() {
        mac_entry.set_text(&existing.mac_address);
        display_name_entry.set_text(existing.display_name.as_deref().unwrap_or(""));
        blocked_switch.set_active(existing.blocked);
        download_spin.set_value(existing.download_limit_kbps.unwrap_or_default() as f64);
        upload_spin.set_value(existing.upload_limit_kbps.unwrap_or_default() as f64);
        time_spin.set_value(existing.time_limit_minutes.unwrap_or_default() as f64);
        download_quota_spin.set_value(existing.download_quota_mb.unwrap_or_default() as f64);
        upload_quota_spin.set_value(existing.upload_quota_mb.unwrap_or_default() as f64);
        blocked_domains_entry.set_text(&existing.blocked_domains.join(", "));
    }

    let group = adw::PreferencesGroup::new();
    group.add(&display_name_entry);
    group.add(&mac_entry);
    group.add(&blocked_switch);
    group.add(&download_row);
    group.add(&upload_row);
    group.add(&time_row);
    group.add(&download_quota_row);
    group.add(&upload_quota_row);
    group.add(&blocked_domains_entry);

    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    body.set_margin_top(12);
    body.set_margin_bottom(12);
    body.set_margin_start(12);
    body.set_margin_end(12);
    body.append(&group);

    let dialog = adw::AlertDialog::builder()
        .heading(if existing.is_some() {
            "Edit rule"
        } else {
            "New rule"
        })
        .body("Configure optional speed limits, quotas, blocked sites, or a hard block for one hotspot device.")
        .extra_child(&body)
        .default_response("save")
        .close_response("cancel")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("save", "Save")]);
    dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);

    let response = if let Some(parent) = parent_widget.root().and_downcast_ref::<gtk4::Window>() {
        dialog.choose_future(Some(parent)).await
    } else {
        dialog.choose_future(None::<&gtk4::Window>).await
    };

    if response.as_str() != "save" {
        return Ok(None);
    }

    let mac_address = config::normalize_mac_address(mac_entry.text().as_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid MAC address"))?;
    let blocked_domains = blocked_domains_entry
        .text()
        .split(',')
        .filter_map(config::normalize_blocked_domain)
        .collect::<Vec<_>>();
    Ok(Some(HotspotClientRule {
        mac_address,
        display_name: Some(display_name_entry.text().trim().to_string())
            .filter(|value| !value.is_empty()),
        blocked: blocked_switch.is_active(),
        upload_limit_kbps: spin_value_to_option(&upload_spin),
        download_limit_kbps: spin_value_to_option(&download_spin),
        time_limit_minutes: spin_value_to_option(&time_spin),
        upload_quota_mb: spin_value_to_option(&upload_quota_spin).map(u64::from),
        download_quota_mb: spin_value_to_option(&download_quota_spin).map(u64::from),
        blocked_domains,
    }))
}
