// File: devices_page.rs
// Location: /src/ui/devices_page.rs

use chrono::Utc;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::{self as adw, prelude::*};
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::config::{self, HotspotClientRule};
use crate::hotspot;
use crate::modem_manager;
use crate::state::{AppState, PageKind};
use crate::ui::{common, icon_name};
use anyhow::Result;

#[derive(Clone)]
pub struct DevicesPage {
    pub widget: gtk4::Box,
    toast_overlay: adw::ToastOverlay,
    mobile_group: adw::PreferencesGroup,
    mobile_status_row: adw::ActionRow,
    mobile_signal_row: adw::ActionRow,
    mobile_network_row: adw::ActionRow,
    mobile_sim_row: adw::ActionRow,
    mobile_apn_entry: adw::EntryRow,
    mobile_connect_button: gtk4::Button,
    mobile_radio_button: gtk4::Button,
    list_box: gtk4::ListBox,
    empty_state: adw::StatusPage,
    client_count_label: gtk4::Label,
    refresh_button: gtk4::Button,
    spinner: gtk4::Spinner,
    operation_status_label: gtk4::Label,
    app_state: AppState,
}

#[derive(Debug, Clone)]
struct ConnectedDevice {
    ip: String,
    mac: String,
    hostname: Option<String>,
    lease_expiry: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
enum DeviceKind {
    Phone,
    Computer,
    Tv,
    Iot,
    Unknown,
}

impl DevicesPage {
    pub fn new(app_state: AppState) -> Self {
        let widget = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        let toast_overlay = adw::ToastOverlay::new();

        // Header with refresh button
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 16);
        header.set_margin_top(16);
        header.set_margin_bottom(16);
        header.set_margin_start(16);
        header.set_margin_end(16);

        let title = gtk4::Label::builder()
            .label("Connected Devices")
            .css_classes(vec!["title-2".to_string()])
            .hexpand(true)
            .xalign(0.0)
            .build();

        let refresh_button = gtk4::Button::builder()
            .icon_name(icon_name(
                "view-refresh-symbolic",
                &["view-refresh", "reload-symbolic"][..],
            ))
            .tooltip_text("Refresh devices")
            .css_classes(vec![
                "flat".to_string(),
                "circular".to_string(),
                "touch-target".to_string(),
            ])
            .build();

        let spinner = gtk4::Spinner::new();
        spinner.add_css_class("big-spinner");
        spinner.set_size_request(22, 22);
        spinner.set_visible(false);

        let operation_status_label = gtk4::Label::new(None);
        operation_status_label.set_halign(gtk4::Align::Start);
        operation_status_label.set_opacity(0.7);
        operation_status_label.set_visible(false);

        let client_count_label = gtk4::Label::new(Some("0 clients (approximate)"));
        client_count_label.set_halign(gtk4::Align::Start);
        client_count_label.set_margin_start(16);
        client_count_label.set_margin_end(16);
        client_count_label.set_opacity(0.72);
        client_count_label.add_css_class("title-3");

        header.append(&title);
        header.append(&spinner);
        header.append(&refresh_button);
        widget.append(&header);
        // * Add a dedicated client-count confidence line under the Devices header.
        widget.append(&client_count_label);
        widget.append(&operation_status_label);

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .build();
        let clamp = adw::Clamp::builder()
            .maximum_size(920)
            .tightening_threshold(560)
            .build();

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.set_margin_start(16);
        content.set_margin_end(16);
        content.set_margin_bottom(16);

        let mobile_group = adw::PreferencesGroup::new();
        mobile_group.set_title("Mobile Data");
        mobile_group.set_description(Some(
            "Manage cellular modem status, APN, and connection state from ModemManager.",
        ));

        let mobile_status_row = adw::ActionRow::builder()
            .title("Status")
            .subtitle("Checking for modems...")
            .build();
        let mobile_actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let mobile_connect_button = gtk4::Button::builder()
            .label("Connect")
            .css_classes(vec!["flat".to_string()])
            .build();
        let mobile_radio_button = gtk4::Button::builder()
            .label("Turn radio on")
            .css_classes(vec!["flat".to_string()])
            .build();
        mobile_actions.append(&mobile_connect_button);
        mobile_actions.append(&mobile_radio_button);
        mobile_status_row.add_suffix(&mobile_actions);

        let mobile_signal_row = adw::ActionRow::builder()
            .title("Signal")
            .subtitle("Unavailable")
            .build();
        let mobile_network_row = adw::ActionRow::builder()
            .title("Network")
            .subtitle("Unavailable")
            .build();
        let mobile_sim_row = adw::ActionRow::builder()
            .title("SIM / PIN")
            .subtitle("Unavailable")
            .build();
        let mobile_apn_entry = adw::EntryRow::builder().title("APN").build();
        mobile_apn_entry.set_text("");

        mobile_group.add(&mobile_status_row);
        mobile_group.add(&mobile_signal_row);
        mobile_group.add(&mobile_network_row);
        mobile_group.add(&mobile_sim_row);
        mobile_group.add(&mobile_apn_entry);
        content.append(&mobile_group);

        let list_box = gtk4::ListBox::builder()
            .css_classes(vec!["boxed-list".to_string()])
            .selection_mode(gtk4::SelectionMode::None)
            .build();

        let empty_state = adw::StatusPage::builder()
            .icon_name(icon_name(
                // "network-wireless-hotspot-symbolic",
                "",
                &[
                    "network-wireless-symbolic",
                    "network-workgroup-symbolic",
                    "computer",
                ][..],
            ))
            .title("Waiting for devices to connect…")
            .description("Devices will appear here when they join your hotspot")
            .build();
        empty_state.add_css_class("devices-empty");
        empty_state.set_visible(true);
        list_box.set_visible(false);

        content.append(&list_box);
        content.append(&empty_state);

        clamp.set_child(Some(&content));
        scrolled.set_child(Some(&clamp));
        toast_overlay.set_child(Some(&scrolled));
        widget.append(&toast_overlay);

        let page = Self {
            widget,
            toast_overlay,
            mobile_group: mobile_group.clone(),
            mobile_status_row: mobile_status_row.clone(),
            mobile_signal_row: mobile_signal_row.clone(),
            mobile_network_row: mobile_network_row.clone(),
            mobile_sim_row: mobile_sim_row.clone(),
            mobile_apn_entry: mobile_apn_entry.clone(),
            mobile_connect_button: mobile_connect_button.clone(),
            mobile_radio_button: mobile_radio_button.clone(),
            list_box,
            empty_state,
            client_count_label: client_count_label.clone(),
            refresh_button: refresh_button.clone(),
            spinner: spinner.clone(),
            operation_status_label: operation_status_label.clone(),
            app_state: app_state.clone(),
        };

        // Initial refresh
        let page_ref = page.clone();
        glib::spawn_future_local(async move {
            page_ref.refresh_devices(false).await;
        });

        // Refresh button handler
        let page_ref = page.clone();
        refresh_button.connect_clicked(move |_| {
            let page = page_ref.clone();
            glib::spawn_future_local(async move {
                page.refresh_devices(true).await;
            });
        });

        let page_ref = page.clone();
        mobile_connect_button.connect_clicked(move |_| {
            let page = page_ref.clone();
            glib::spawn_future_local(async move {
                page.toggle_mobile_connection().await;
            });
        });

        let page_ref = page.clone();
        mobile_radio_button.connect_clicked(move |_| {
            let page = page_ref.clone();
            glib::spawn_future_local(async move {
                page.toggle_mobile_radio().await;
            });
        });

        page.set_page_visible(false);

        page
    }

    pub fn set_page_visible(&self, visible: bool) {
        self.app_state.set_page_visible(PageKind::Devices, visible);
        if visible {
            self.start_auto_refresh();
            let page = self.clone();
            glib::spawn_future_local(async move {
                page.refresh_devices(false).await;
            });
        } else {
            self.stop_auto_refresh();
        }
    }

    fn start_auto_refresh(&self) {
        if self.app_state.devices_has_refresh_source() {
            return;
        }

        let page_ref = self.clone();
        let source = glib::timeout_add_seconds_local(8, move || {
            if !page_ref.app_state.is_page_visible(PageKind::Devices) {
                return glib::ControlFlow::Continue;
            }
            if page_ref.app_state.devices_refresh_in_flight() {
                return glib::ControlFlow::Continue;
            }

            let page = page_ref.clone();
            glib::spawn_future_local(async move {
                page.refresh_devices(false).await;
            });

            glib::ControlFlow::Continue
        });

        self.app_state.set_devices_refresh_source(Some(source));
    }

    fn stop_auto_refresh(&self) {
        if let Some(id) = self.app_state.take_devices_refresh_source() {
            id.remove();
        }
    }

    pub async fn refresh_devices(&self, show_feedback: bool) {
        if self.app_state.devices_refresh_in_flight() {
            return;
        }
        self.app_state.set_devices_refresh_in_flight(true);
        let hotspot_active = hotspot::is_hotspot_active().await.unwrap_or(false);
        self.app_state
            .set_devices_auto_refresh_active(hotspot_active);
        if show_feedback {
            common::set_busy(
                &self.spinner,
                &self.operation_status_label,
                Some(&self.refresh_button),
                true,
                Some("Refreshing..."),
            );
            self.list_box.add_css_class("list-loading");
        }

        self.refresh_mobile_data().await;

        match self.get_connected_devices().await {
            Ok(devices) => {
                let displayed_count = devices.len();
                self.update_list(devices);
                let count_info = hotspot::get_connected_device_count_info().await.unwrap_or(
                    hotspot::ConnectedClientCountInfo {
                        count: displayed_count,
                        estimated: false,
                    },
                );
                self.update_client_count(count_info.count, count_info.estimated);
                if self.list_box.first_child().is_none() {
                    self.update_empty_state_message(hotspot_active);
                }
            }
            Err(e) => {
                log::error!("Failed to get connected devices: {}", e);
                // * Show operation-specific device-refresh failures with the actual error.
                self.show_toast(&format!("Failed to refresh connected devices: {}", e));
                self.update_client_count(0, false);
                self.update_empty_state_message(hotspot_active);
                self.show_empty_state();
            }
        }

        if show_feedback {
            common::set_busy(
                &self.spinner,
                &self.operation_status_label,
                Some(&self.refresh_button),
                false,
                None,
            );
            self.list_box.remove_css_class("list-loading");
        }
        self.app_state.set_devices_refresh_in_flight(false);
    }

    async fn refresh_mobile_data(&self) {
        match modem_manager::get_mobile_data_status().await {
            Ok(status) => self.apply_mobile_data_status(status),
            Err(e) => {
                log::error!("Failed to refresh mobile data status: {}", e);
                self.mobile_group
                    .set_description(Some("Could not read ModemManager status."));
                self.mobile_status_row.set_subtitle("Status unavailable");
                self.mobile_signal_row.set_subtitle("Unavailable");
                self.mobile_network_row.set_subtitle("Unavailable");
                self.mobile_sim_row.set_subtitle("Unavailable");
                self.mobile_apn_entry.set_sensitive(false);
                self.mobile_connect_button.set_sensitive(false);
                self.mobile_radio_button.set_sensitive(false);
            }
        }
    }

    fn apply_mobile_data_status(&self, status: modem_manager::MobileDataStatus) {
        if !status.service_available {
            self.mobile_group.set_description(Some(
                "ModemManager is not available on this system, so mobile data controls are disabled.",
            ));
            self.mobile_status_row.set_subtitle(&status.state_label);
            self.mobile_signal_row.set_subtitle("Unavailable");
            self.mobile_network_row.set_subtitle("Unavailable");
            self.mobile_sim_row.set_subtitle("Unavailable");
            self.mobile_apn_entry.set_text("");
            self.mobile_apn_entry.set_sensitive(false);
            self.mobile_connect_button.set_label("Connect");
            self.mobile_connect_button.set_sensitive(false);
            self.mobile_radio_button.set_label("Turn radio on");
            self.mobile_radio_button.set_sensitive(false);
            return;
        }

        if !status.modem_present {
            self.mobile_group.set_description(Some(
                "No mobile broadband modem was detected. If you attach one, it will appear here automatically.",
            ));
            self.mobile_status_row.set_subtitle(&status.state_label);
            self.mobile_signal_row.set_subtitle("Unavailable");
            self.mobile_network_row.set_subtitle("Unavailable");
            self.mobile_sim_row.set_subtitle("Unavailable");
            self.mobile_apn_entry.set_text("");
            self.mobile_apn_entry.set_sensitive(false);
            self.mobile_connect_button.set_label("Connect");
            self.mobile_connect_button.set_sensitive(false);
            self.mobile_radio_button.set_label("Turn radio on");
            self.mobile_radio_button.set_sensitive(false);
            return;
        }

        self.mobile_group.set_description(Some(
            "Cellular modem controls are available through ModemManager.",
        ));

        let status_subtitle = match status.device_name.as_deref() {
            Some(device) => format!("{} • {}", status.state_label, device),
            None => status.state_label.clone(),
        };
        self.mobile_status_row.set_subtitle(&status_subtitle);

        let signal_text = match status.signal_quality_percent {
            Some(signal) => format!("{}%", signal),
            None => "Unavailable".to_string(),
        };
        self.mobile_signal_row.set_subtitle(&signal_text);

        let network_text = format!(
            "{} • {}",
            status
                .operator_name
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or("Operator unavailable"),
            status
                .network_generation
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or("Unavailable")
        );
        self.mobile_network_row.set_subtitle(&network_text);
        self.mobile_sim_row
            .set_subtitle(&format!("{} • {}", status.sim_status, status.pin_status));
        self.mobile_apn_entry
            .set_text(status.apn.as_deref().unwrap_or(""));
        self.mobile_apn_entry.set_sensitive(status.radio_enabled);
        self.mobile_connect_button.set_label(if status.connected {
            "Disconnect"
        } else {
            "Connect"
        });
        self.mobile_connect_button.set_sensitive(
            status.radio_enabled && !matches!(status.state_label.as_str(), "Locked"),
        );
        self.mobile_radio_button.set_label(if status.radio_enabled {
            "Turn radio off"
        } else {
            "Turn radio on"
        });
        self.mobile_radio_button.set_sensitive(true);
    }

    async fn toggle_mobile_connection(&self) {
        let status = match modem_manager::get_mobile_data_status().await {
            Ok(status) => status,
            Err(e) => {
                self.show_toast(&format!("Failed to read mobile data status: {}", e));
                return;
            }
        };

        if !status.modem_present {
            self.show_toast("No mobile modem detected");
            return;
        }

        let apn = self.mobile_apn_entry.text().trim().to_string();
        let result = if status.connected {
            modem_manager::disconnect_mobile_data().await
        } else {
            modem_manager::connect_mobile_data((!apn.is_empty()).then_some(apn.as_str())).await
        };

        match result {
            Ok(()) => {
                self.show_toast(if status.connected {
                    "Mobile data disconnected"
                } else {
                    "Mobile data connected"
                });
                self.refresh_mobile_data().await;
            }
            Err(e) => {
                log::error!("Failed to toggle mobile data connection: {}", e);
                self.show_toast(&format!("Failed to update mobile data: {}", e));
            }
        }
    }

    async fn toggle_mobile_radio(&self) {
        let status = match modem_manager::get_mobile_data_status().await {
            Ok(status) => status,
            Err(e) => {
                self.show_toast(&format!("Failed to read mobile data status: {}", e));
                return;
            }
        };

        if !status.modem_present {
            self.show_toast("No mobile modem detected");
            return;
        }

        let target = !status.radio_enabled;
        match modem_manager::set_radio_enabled(target).await {
            Ok(()) => {
                self.show_toast(if target {
                    "Mobile radio enabled"
                } else {
                    "Mobile radio disabled"
                });
                self.refresh_mobile_data().await;
            }
            Err(e) => {
                log::error!("Failed to toggle mobile radio: {}", e);
                self.show_toast(&format!("Failed to update radio state: {}", e));
            }
        }
    }

    fn update_empty_state_message(&self, hotspot_active: bool) {
        if hotspot_active {
            self.empty_state
                .set_title("Waiting for devices to connect...");
            self.empty_state
                .set_description(Some("Devices will appear here when they join your hotspot"));
        } else {
            self.empty_state.set_title("No devices found");
            self.empty_state
                .set_description(Some("Start a hotspot or refresh to detect nearby clients"));
        }
    }

    fn update_client_count(&self, count: usize, estimated: bool) {
        let qualifier = if estimated {
            "estimated"
        } else {
            "approximate"
        };
        let noun = if count == 1 { "client" } else { "clients" };
        self.client_count_label
            .set_text(&format!("{} {} ({})", count, noun, qualifier));
        if estimated {
            self.client_count_label.set_tooltip_text(Some("estimated"));
        } else {
            self.client_count_label.set_tooltip_text(None);
        }
    }

    async fn get_connected_devices(&self) -> Result<Vec<ConnectedDevice>> {
        Ok(hotspot::list_connected_clients()
            .await?
            .into_iter()
            .map(|device| ConnectedDevice {
                ip: device.ip,
                mac: device.mac,
                hostname: device.hostname,
                lease_expiry: device.lease_expiry,
            })
            .collect())
    }

    fn update_list(&self, devices: Vec<ConnectedDevice>) {
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        if devices.is_empty() {
            self.show_empty_state();
            return;
        }

        self.empty_state.set_visible(false);
        self.list_box.set_visible(true);
        let hotspot_config =
            config::load_config(&config::hotspot_config_path()).unwrap_or_default();
        let rule_map: HashMap<String, HotspotClientRule> = hotspot_config
            .client_rules
            .into_iter()
            .map(|rule| (rule.mac_address.clone(), rule))
            .collect();

        for device in &devices {
            let hostname = device
                .hostname
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty() && *name != device.ip);
            let title = hostname
                .map(str::to_string)
                .unwrap_or_else(|| device.ip.clone());

            let mut subtitle_parts = Vec::new();
            match hostname {
                Some(_) => subtitle_parts.push(format!("{} • {}", device.ip, device.mac)),
                None => subtitle_parts.push(device.mac.clone()),
            }

            if let Some(expiry) = device.lease_expiry {
                if let Some(lease_info) = format_lease_remaining(expiry) {
                    subtitle_parts.push(lease_info);
                }
            }
            if let Some(rule) = rule_map.get(&device.mac) {
                if let Some(summary) = rule_summary(rule) {
                    subtitle_parts.push(summary);
                }
            }

            let subtitle = subtitle_parts.join(" • ");

            let row = adw::ActionRow::builder()
                .title(&title)
                .subtitle(&subtitle)
                .build();
            row.add_css_class("fade-in");
            row.add_css_class("device-policy-row");
            row.set_activatable(true);

            let icon = gtk4::Image::from_icon_name(device_icon_name(device));
            row.add_prefix(&icon);

            let manage_button = gtk4::Button::builder()
                .label("Manage")
                .css_classes(vec!["flat".to_string(), "touch-target".to_string()])
                .build();
            let page_for_manage = self.clone();
            let device_for_manage = device.clone();
            manage_button.connect_clicked(move |_| {
                let page = page_for_manage.clone();
                let device = device_for_manage.clone();
                glib::spawn_future_local(async move {
                    page.manage_device_rule(device).await;
                });
            });
            row.add_suffix(&manage_button);

            let page = self.clone();
            let device_for_dialog = device.clone();
            row.connect_activated(move |_| {
                let page = page.clone();
                let device = device_for_dialog.clone();
                glib::spawn_future_local(async move {
                    page.show_device_details_dialog(device).await;
                });
            });

            self.add_device_context_menu(&row, device);
            self.list_box.append(&row);
        }
    }

    fn add_device_context_menu(&self, row: &adw::ActionRow, device: &ConnectedDevice) {
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(3);

        let row_for_menu = row.clone();
        let toast_overlay = self.toast_overlay.clone();
        let device_name = device.hostname.clone().unwrap_or_else(|| device.ip.clone());
        let device_ip = device.ip.clone();
        let device_mac = device.mac.clone();
        let page = self.clone();
        let device_mac_for_status = device.mac.clone();
        let currently_blocked = config::load_config(&config::hotspot_config_path())
            .ok()
            .and_then(|config| {
                config
                    .client_rules
                    .into_iter()
                    .find(|rule| rule.mac_address == device_mac_for_status)
            })
            .map(|rule| rule.blocked)
            .unwrap_or(false);

        gesture.connect_released(move |_gesture, _n_press, x, y| {
            let popover = gtk4::Popover::new();
            popover.set_position(gtk4::PositionType::Bottom);
            popover.set_has_arrow(false);
            popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));

            let menu_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            menu_box.add_css_class("menu");
            menu_box.set_margin_top(6);
            menu_box.set_margin_bottom(6);

            let details_btn = gtk4::Button::builder()
                .label("Device details")
                .css_classes(vec!["flat".to_string()])
                .build();
            let popover_details = popover.clone();
            let page_details = page.clone();
            let details_device = ConnectedDevice {
                ip: device_ip.clone(),
                mac: device_mac.clone(),
                hostname: Some(device_name.clone()),
                lease_expiry: None,
            };
            details_btn.connect_clicked(move |_| {
                popover_details.popdown();
                let page = page_details.clone();
                let device = details_device.clone();
                glib::spawn_future_local(async move {
                    page.show_device_details_dialog(device).await;
                });
            });
            menu_box.append(&details_btn);

            let manage_btn = gtk4::Button::builder()
                .label("Manage access")
                .css_classes(vec!["flat".to_string()])
                .build();
            let popover_manage = popover.clone();
            let page_manage = page.clone();
            let manage_device = ConnectedDevice {
                ip: device_ip.clone(),
                mac: device_mac.clone(),
                hostname: Some(device_name.clone()),
                lease_expiry: None,
            };
            manage_btn.connect_clicked(move |_| {
                popover_manage.popdown();
                let page = page_manage.clone();
                let device = manage_device.clone();
                glib::spawn_future_local(async move {
                    page.manage_device_rule(device).await;
                });
            });
            menu_box.append(&manage_btn);

            let copy_ip_btn = gtk4::Button::builder()
                .label("Copy IP")
                .css_classes(vec!["flat".to_string()])
                .build();
            let popover_copy_ip = popover.clone();
            let toast_overlay_copy_ip = toast_overlay.clone();
            let device_ip_copy = device_ip.clone();
            copy_ip_btn.connect_clicked(move |_| {
                popover_copy_ip.popdown();
                copy_to_clipboard(&device_ip_copy);
                common::show_toast(&toast_overlay_copy_ip, "IP copied");
            });
            menu_box.append(&copy_ip_btn);

            let copy_mac_btn = gtk4::Button::builder()
                .label("Copy MAC")
                .css_classes(vec!["flat".to_string()])
                .build();
            let popover_copy_mac = popover.clone();
            let toast_overlay_copy_mac = toast_overlay.clone();
            let device_mac_copy = device_mac.clone();
            copy_mac_btn.connect_clicked(move |_| {
                popover_copy_mac.popdown();
                copy_to_clipboard(&device_mac_copy);
                common::show_toast(&toast_overlay_copy_mac, "MAC copied");
            });
            menu_box.append(&copy_mac_btn);

            let block_btn = gtk4::Button::builder()
                .label(if currently_blocked {
                    "Unblock device"
                } else {
                    "Block device"
                })
                .css_classes(vec!["flat".to_string()])
                .build();

            let popover_block = popover.clone();
            let page_block = page.clone();
            let device_name = device_name.clone();
            let device_mac_for_block = device_mac.clone();
            block_btn.connect_clicked(move |_| {
                popover_block.popdown();
                let page = page_block.clone();
                let device_name = device_name.clone();
                let mac = device_mac_for_block.clone();
                glib::spawn_future_local(async move {
                    match page.set_device_blocked(&mac, !currently_blocked).await {
                        Ok(()) => {
                            let message = if currently_blocked {
                                format!("{} is no longer blocked", device_name)
                            } else {
                                format!("{} blocked", device_name)
                            };
                            page.show_toast(&message);
                        }
                        Err(e) => {
                            page.show_toast(&format!("Failed to update device policy: {}", e))
                        }
                    }
                    page.refresh_devices(false).await;
                });
            });

            menu_box.append(&block_btn);
            popover.set_child(Some(&menu_box));
            popover.set_parent(&row_for_menu);
            popover.popup();
        });

        row.add_controller(gesture);
    }

    async fn show_device_details_dialog(&self, device: ConnectedDevice) {
        let title = device
            .hostname
            .as_deref()
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .unwrap_or(device.ip.as_str());
        let body = format!("IP: {}\nMAC: {}", device.ip, device.mac);

        let dialog = adw::AlertDialog::builder()
            .heading(title)
            .body(&body)
            .default_response("close")
            .close_response("close")
            .build();
        dialog.add_responses(
            &[
                ("close", "Close"),
                ("copy-ip", "Copy IP"),
                ("copy-mac", "Copy MAC"),
            ][..],
        );

        let response = if let Some(parent) = self.widget.root().and_downcast_ref::<gtk4::Window>() {
            dialog.choose_future(Some(parent)).await
        } else {
            dialog.choose_future(None::<&gtk4::Window>).await
        };

        match response.as_str() {
            "copy-ip" => {
                copy_to_clipboard(&device.ip);
                self.show_toast("IP copied");
            }
            "copy-mac" => {
                copy_to_clipboard(&device.mac);
                self.show_toast("MAC copied");
            }
            _ => {}
        }
    }

    async fn manage_device_rule(&self, device: ConnectedDevice) {
        let existing_rule = config::load_config(&config::hotspot_config_path())
            .ok()
            .and_then(|config| {
                config
                    .client_rules
                    .into_iter()
                    .find(|rule| rule.mac_address == device.mac)
            });

        match self
            .show_device_policy_dialog(device.clone(), existing_rule)
            .await
        {
            Ok(Some(rule)) => {
                if let Err(e) = self.save_device_rule(&device.mac, rule).await {
                    self.show_toast(&format!("Failed to save device policy: {}", e));
                    return;
                }
                self.show_toast("Device policy updated");
                self.refresh_devices(false).await;
            }
            Ok(None) => {}
            Err(e) => self.show_toast(&format!("Failed to edit device policy: {}", e)),
        }
    }

    async fn save_device_rule(
        &self,
        mac_address: &str,
        rule: HotspotClientRule,
    ) -> anyhow::Result<()> {
        let normalized_mac = config::normalize_mac_address(mac_address)
            .ok_or_else(|| anyhow::anyhow!("Invalid MAC address"))?;
        let mut hotspot_config =
            config::load_config(&config::hotspot_config_path()).unwrap_or_default();
        let existing_index = hotspot_config
            .client_rules
            .iter()
            .position(|existing| existing.mac_address == normalized_mac);

        if rule_is_effectively_empty(&rule) {
            if let Some(index) = existing_index {
                hotspot_config.client_rules.remove(index);
            }
        } else if let Some(index) = existing_index {
            hotspot_config.client_rules[index] = rule;
        } else {
            hotspot_config.client_rules.push(rule);
        }

        config::save_config(&config::hotspot_config_path(), &hotspot_config)?;
        hotspot::sync_runtime_rules_from_disk().await.ok();
        Ok(())
    }

    async fn set_device_blocked(&self, mac_address: &str, blocked: bool) -> anyhow::Result<()> {
        let normalized_mac = config::normalize_mac_address(mac_address)
            .ok_or_else(|| anyhow::anyhow!("Invalid MAC address"))?;
        let mut hotspot_config =
            config::load_config(&config::hotspot_config_path()).unwrap_or_default();
        if let Some(rule) = hotspot_config
            .client_rules
            .iter_mut()
            .find(|rule| rule.mac_address == normalized_mac)
        {
            rule.blocked = blocked;
            if !blocked && rule_is_effectively_empty(rule) {
                hotspot_config
                    .client_rules
                    .retain(|rule| rule.mac_address != normalized_mac);
            }
        } else if blocked {
            hotspot_config.client_rules.push(HotspotClientRule {
                mac_address: normalized_mac.clone(),
                blocked: true,
                ..HotspotClientRule::default()
            });
        }

        config::save_config(&config::hotspot_config_path(), &hotspot_config)?;
        hotspot::sync_runtime_rules_from_disk().await.ok();
        Ok(())
    }

    async fn show_device_policy_dialog(
        &self,
        device: ConnectedDevice,
        existing_rule: Option<HotspotClientRule>,
    ) -> anyhow::Result<Option<HotspotClientRule>> {
        let display_name_entry = adw::EntryRow::builder().title("Device name").build();
        display_name_entry.set_text(
            existing_rule
                .as_ref()
                .and_then(|rule| rule.display_name.as_deref())
                .or(device.hostname.as_deref())
                .unwrap_or(""),
        );

        let mac_row = adw::ActionRow::builder()
            .title("MAC address")
            .subtitle(&device.mac)
            .build();
        let ip_row = adw::ActionRow::builder()
            .title("IP address")
            .subtitle(&device.ip)
            .build();
        let blocked_switch = adw::SwitchRow::builder()
            .title("Block this device")
            .subtitle("Immediately drops hotspot traffic for this MAC")
            .active(
                existing_rule
                    .as_ref()
                    .map(|rule| rule.blocked)
                    .unwrap_or(false),
            )
            .build();

        let download_row = adw::ActionRow::builder()
            .title("Download speed limit")
            .subtitle("kbit/s, optional")
            .build();
        let download_spin = gtk4::SpinButton::builder()
            .adjustment(&gtk4::Adjustment::new(
                0.0,
                0.0,
                1_000_000.0,
                100.0,
                1000.0,
                0.0,
            ))
            .numeric(true)
            .digits(0)
            .build();
        download_spin.set_value(
            existing_rule
                .as_ref()
                .and_then(|rule| rule.download_limit_kbps)
                .unwrap_or_default() as f64,
        );
        download_row.add_suffix(&download_spin);

        let upload_row = adw::ActionRow::builder()
            .title("Upload speed limit")
            .subtitle("kbit/s, optional")
            .build();
        let upload_spin = gtk4::SpinButton::builder()
            .adjustment(&gtk4::Adjustment::new(
                0.0,
                0.0,
                1_000_000.0,
                100.0,
                1000.0,
                0.0,
            ))
            .numeric(true)
            .digits(0)
            .build();
        upload_spin.set_value(
            existing_rule
                .as_ref()
                .and_then(|rule| rule.upload_limit_kbps)
                .unwrap_or_default() as f64,
        );
        upload_row.add_suffix(&upload_spin);

        let settings = config::load_app_settings(&config::app_settings_path()).unwrap_or_default();
        let quota_reset = match settings.hotspot_quota_reset_policy {
            config::HotspotQuotaResetPolicy::Never => "never reset automatically",
            config::HotspotQuotaResetPolicy::DailyMidnight => "reset daily at 00:00",
        };

        let time_row = adw::ActionRow::builder()
            .title("Connected time quota")
            .subtitle(format!("minutes, {}", quota_reset))
            .build();
        let time_spin = gtk4::SpinButton::builder()
            .adjustment(&gtk4::Adjustment::new(0.0, 0.0, 100_000.0, 5.0, 60.0, 0.0))
            .numeric(true)
            .digits(0)
            .build();
        time_spin.set_value(
            existing_rule
                .as_ref()
                .and_then(|rule| rule.time_limit_minutes)
                .unwrap_or_default() as f64,
        );
        time_row.add_suffix(&time_spin);

        let download_quota_row = adw::ActionRow::builder()
            .title("Download quota")
            .subtitle(format!("MB, {}", quota_reset))
            .build();
        let download_quota_spin = gtk4::SpinButton::builder()
            .adjustment(&gtk4::Adjustment::new(
                0.0,
                0.0,
                1_000_000.0,
                50.0,
                500.0,
                0.0,
            ))
            .numeric(true)
            .digits(0)
            .build();
        download_quota_spin.set_value(
            existing_rule
                .as_ref()
                .and_then(|rule| rule.download_quota_mb)
                .unwrap_or_default() as f64,
        );
        download_quota_row.add_suffix(&download_quota_spin);

        let upload_quota_row = adw::ActionRow::builder()
            .title("Upload quota")
            .subtitle(format!("MB, {}", quota_reset))
            .build();
        let upload_quota_spin = gtk4::SpinButton::builder()
            .adjustment(&gtk4::Adjustment::new(
                0.0,
                0.0,
                1_000_000.0,
                50.0,
                500.0,
                0.0,
            ))
            .numeric(true)
            .digits(0)
            .build();
        upload_quota_spin.set_value(
            existing_rule
                .as_ref()
                .and_then(|rule| rule.upload_quota_mb)
                .unwrap_or_default() as f64,
        );
        upload_quota_row.add_suffix(&upload_quota_spin);

        let blocked_domains_entry = adw::EntryRow::builder().title("Blocked sites").build();
        blocked_domains_entry.set_text(
            &existing_rule
                .as_ref()
                .map(|rule| rule.blocked_domains.join(", "))
                .unwrap_or_default(),
        );
        blocked_domains_entry.set_tooltip_text(Some(
            "Comma-separated domains, for example: youtube.com, instagram.com",
        ));

        let group = adw::PreferencesGroup::new();
        group.add(&display_name_entry);
        group.add(&mac_row);
        group.add(&ip_row);
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

        let title = device
            .hostname
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(device.ip.as_str())
            .to_string();
        let dialog = adw::AlertDialog::builder()
            .heading(&title)
            .body("Set per-device limits or leave the fields empty and save to remove the rule.")
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
            return Ok(None);
        }

        let blocked_domains = blocked_domains_entry
            .text()
            .split(',')
            .filter_map(config::normalize_blocked_domain)
            .collect::<Vec<_>>();

        Ok(Some(HotspotClientRule {
            mac_address: device.mac.clone(),
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

    fn show_empty_state(&self) {
        self.list_box.set_visible(false);
        self.empty_state.set_visible(true);
    }

    fn show_toast(&self, message: &str) {
        common::show_toast(&self.toast_overlay, message);
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

fn rule_summary(rule: &HotspotClientRule) -> Option<String> {
    let mut parts = Vec::new();
    if rule.blocked {
        parts.push("blocked".to_string());
    }
    if let Some(limit) = rule.download_limit_kbps {
        parts.push(format!("down {} kbit/s", limit));
    }
    if let Some(limit) = rule.upload_limit_kbps {
        parts.push(format!("up {} kbit/s", limit));
    }
    if let Some(limit) = rule.time_limit_minutes {
        parts.push(format!("{} min quota", limit));
    }
    if let Some(limit) = rule.download_quota_mb {
        parts.push(format!("{} MB down", limit));
    }
    if let Some(limit) = rule.upload_quota_mb {
        parts.push(format!("{} MB up", limit));
    }
    if !rule.blocked_domains.is_empty() {
        parts.push(format!("{} blocked site(s)", rule.blocked_domains.len()));
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("Policy: {}", parts.join(", ")))
    }
}

fn rule_is_effectively_empty(rule: &HotspotClientRule) -> bool {
    !rule.blocked
        && rule.upload_limit_kbps.is_none()
        && rule.download_limit_kbps.is_none()
        && rule.time_limit_minutes.is_none()
        && rule.upload_quota_mb.is_none()
        && rule.download_quota_mb.is_none()
        && rule.blocked_domains.is_empty()
}

fn copy_to_clipboard(value: &str) {
    if let Some(display) = gtk4::gdk::Display::default() {
        display.clipboard().set_text(value);
    }
}

fn device_icon_name(device: &ConnectedDevice) -> &'static str {
    match device_kind_for(device) {
        DeviceKind::Phone => icon_name(
            "phone-symbolic",
            &[
                "smartphone-symbolic",
                "phone-apple-iphone-symbolic",
                "multimedia-player-symbolic",
            ][..],
        ),
        DeviceKind::Tv => icon_name(
            "tv-symbolic",
            &[
                "display-symbolic",
                "video-display-symbolic",
                "computer-symbolic",
            ][..],
        ),
        DeviceKind::Computer => icon_name(
            "computer-symbolic",
            &["computer-apple-ipad-symbolic", "computer-old-symbolic"][..],
        ),
        DeviceKind::Iot => icon_name(
            "network-wireless-symbolic",
            &[
                "network-workgroup-symbolic",
                "network-transmit-receive-symbolic",
            ][..],
        ),
        DeviceKind::Unknown => icon_name(
            "network-wired-symbolic",
            &[
                "network-workgroup-symbolic",
                "network-transmit-receive-symbolic",
            ][..],
        ),
    }
}

fn device_kind_for(device: &ConnectedDevice) -> DeviceKind {
    if let Some(hostname) = device.hostname.as_deref() {
        if let Some(kind) = device_kind_from_hostname(hostname) {
            return kind;
        }
    }

    if let Some(vendor) = vendor_from_mac(&device.mac) {
        if let Some(kind) = device_kind_from_vendor(&vendor) {
            return kind;
        }
    }

    if is_locally_administered(&device.mac) {
        return DeviceKind::Phone;
    }

    DeviceKind::Unknown
}

fn device_kind_from_hostname(hostname: &str) -> Option<DeviceKind> {
    let lower = hostname.to_lowercase();

    if lower.contains("phone")
        || lower.contains("android")
        || lower.contains("iphone")
        || lower.contains("ipad")
        || lower.contains("pixel")
        || lower.contains("galaxy")
        || lower.contains("mobile")
        || lower.contains("tablet")
    {
        return Some(DeviceKind::Phone);
    }

    if lower.contains("tv")
        || lower.contains("roku")
        || lower.contains("chromecast")
        || lower.contains("firetv")
        || lower.contains("bravia")
        || lower.contains("hisense")
        || lower.contains("samsung")
        || lower.contains("lg")
        || lower.contains("philips")
        || lower.contains("tcl")
        || lower.contains("vizio")
    {
        return Some(DeviceKind::Tv);
    }

    if lower.contains("laptop")
        || lower.contains("desktop")
        || lower.contains("pc")
        || lower.contains("macbook")
        || lower.contains("thinkpad")
        || lower.contains("surface")
    {
        return Some(DeviceKind::Computer);
    }

    if lower.contains("speaker")
        || lower.contains("echo")
        || lower.contains("nest")
        || lower.contains("homepod")
        || lower.contains("sonos")
    {
        return Some(DeviceKind::Iot);
    }

    None
}

fn device_kind_from_vendor(vendor: &str) -> Option<DeviceKind> {
    let lower = vendor.to_lowercase();

    let tv_keywords = [
        "roku",
        "chromecast",
        "vizio",
        "hisense",
        "tcl",
        "panasonic",
        "philips",
        "sharp",
        "toshiba",
        "lg electronics",
    ];

    let phone_keywords = [
        "apple", "samsung", "huawei", "xiaomi", "oneplus", "oppo", "vivo", "google", "motorola",
        "nokia", "sony", "htc",
    ];

    let computer_keywords = [
        "dell",
        "lenovo",
        "asus",
        "acer",
        "hewlett",
        "hp",
        "intel",
        "microsoft",
        "msi",
        "gigabyte",
        "framework",
        "system76",
    ];

    let iot_keywords = ["amazon", "ring", "nest", "sonos", "bose", "ubiquiti"];

    if tv_keywords.iter().any(|kw| lower.contains(kw)) {
        return Some(DeviceKind::Tv);
    }
    if phone_keywords.iter().any(|kw| lower.contains(kw)) {
        return Some(DeviceKind::Phone);
    }
    if computer_keywords.iter().any(|kw| lower.contains(kw)) {
        return Some(DeviceKind::Computer);
    }
    if iot_keywords.iter().any(|kw| lower.contains(kw)) {
        return Some(DeviceKind::Iot);
    }

    None
}

fn vendor_from_mac(mac: &str) -> Option<String> {
    let oui = normalize_mac_prefix(mac)?;
    oui_map().get(&oui).cloned()
}

fn oui_map() -> &'static HashMap<String, String> {
    static OUI_MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
    OUI_MAP.get_or_init(load_oui_map)
}

fn load_oui_map() -> HashMap<String, String> {
    let mut map = HashMap::new();
    let paths = [
        "/usr/share/hwdata/oui.txt",
        "/usr/share/misc/oui.txt",
        "/usr/share/ieee-data/oui.txt",
        "/var/lib/ieee-data/oui.txt",
    ];

    for path in &paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            parse_oui_content(&content, &mut map);
            if !map.is_empty() {
                break;
            }
        }
    }

    map
}

fn parse_oui_content(content: &str, map: &mut HashMap<String, String>) {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let (prefix, vendor) = if let Some((left, right)) = line.split_once("(hex)") {
            (left, right)
        } else if let Some((left, right)) = line.split_once("(base 16)") {
            (left, right)
        } else {
            continue;
        };

        let oui = match normalize_oui(prefix) {
            Some(oui) => oui,
            std::prelude::v1::None => continue,
        };
        let vendor = vendor.trim();
        if !vendor.is_empty() {
            map.insert(oui, vendor.to_string());
        }
    }
}

fn normalize_oui(prefix: &str) -> Option<String> {
    let hex: String = prefix.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() < 6 {
        return None;
    }
    Some(hex[..6].to_uppercase())
}

fn normalize_mac_prefix(mac: &str) -> Option<String> {
    let hex: String = mac.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() < 6 {
        return None;
    }
    Some(hex[..6].to_uppercase())
}

fn is_locally_administered(mac: &str) -> bool {
    let hex: String = mac.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() < 2 {
        return false;
    }
    let first_octet = u8::from_str_radix(&hex[..2], 16).unwrap_or(0);
    first_octet & 0x02 != 0
}

fn format_lease_remaining(expiry: i64) -> Option<String> {
    let now = Utc::now().timestamp();
    let remaining = expiry - now;
    if remaining <= 0 {
        return Some("Lease expired".to_string());
    }

    let minutes = (remaining + 59) / 60;
    if minutes < 60 {
        return Some(format!("Lease expires in {}m", minutes));
    }

    let hours = minutes / 60;
    if hours < 24 {
        return Some(format!("Lease expires in {}h {}m", hours, minutes % 60));
    }

    let days = hours / 24;
    let hours_rem = hours % 24;
    Some(format!("Lease expires in {}d {}h", days, hours_rem))
}
