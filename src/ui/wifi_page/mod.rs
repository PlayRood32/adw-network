// * Credits and inspirations:
// * airctl (pshycodr) for network-focused UI patterns.
// * GNOME Settings Network panel.

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::{self as adw, prelude::*};
use std::cmp::Ordering;
use std::collections::HashSet;

use crate::nm::{self, WifiNetwork};
use crate::qr_dialog;
use crate::state::{AppState, PageKind, WifiFilterState};
use crate::ui::{common, icon_name};

mod actions;
mod details;
mod dialogs;
use actions::BusyGuard;
use details::{get_signal_icon, get_signal_strength_text, invalid_ip_entries};
use dialogs::parse_entry_list;

#[derive(Clone)]
pub struct WifiPage {
    pub widget: gtk4::Box,
    toast_overlay: adw::ToastOverlay,
    wifi_switch: adw::SwitchRow,
    #[allow(dead_code)]
    search_entry: gtk4::SearchEntry,
    hidden_network_button: gtk4::Button,
    refresh_button: gtk4::Button,
    spinner: gtk4::Spinner,
    operation_status_label: gtk4::Label,
    connected_card: gtk4::Box,
    connected_ssid: gtk4::Label,
    connected_subtitle: gtk4::Label,
    connected_details_revealer: gtk4::Revealer,
    connected_details_ip: gtk4::Label,
    connected_details_dns: gtk4::Label,
    connected_details_speed: gtk4::Label,
    known_header: gtk4::Label,
    known_list: gtk4::ListBox,
    other_header: gtk4::Label,
    other_list: gtk4::ListBox,
    empty_state: adw::StatusPage,
    empty_action: gtk4::Button,
    filter_all: gtk4::ToggleButton,
    filter_24: gtk4::ToggleButton,
    filter_5: gtk4::ToggleButton,
    filter_saved: gtk4::ToggleButton,
    app_state: AppState,
}

impl WifiPage {
    pub fn new(app_state: AppState) -> Self {
        let widget = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let toast_overlay = adw::ToastOverlay::new();

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .build();
        let clamp = adw::Clamp::builder()
            .maximum_size(920)
            .tightening_threshold(560)
            .build();

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(16);
        content.set_margin_end(16);

        // WiFi Toggle
        let wifi_switch = adw::SwitchRow::builder().title("Use Wi-Fi").build();

        let switch_group = adw::PreferencesGroup::new();
        switch_group.add(&wifi_switch);
        content.append(&switch_group);

        // Search Bar
        let search_filter_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        search_filter_box.set_margin_top(12);

        // Search Entry
        let search_entry = gtk4::SearchEntry::builder()
            .placeholder_text("Search networks...")
            .build();
        search_entry.add_css_class("search-entry");

        search_filter_box.append(&search_entry);

        // Filter row
        let filter_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        filter_row.set_halign(gtk4::Align::Start);
        filter_row.add_css_class("filter-row");

        let filter_all = gtk4::ToggleButton::builder().label("All").build();
        filter_all.add_css_class("toggle");
        let filter_24 = gtk4::ToggleButton::builder().label("2.4 GHz").build();
        filter_24.add_css_class("toggle");
        let filter_5 = gtk4::ToggleButton::builder().label("5 GHz").build();
        filter_5.add_css_class("toggle");
        let filter_saved = gtk4::ToggleButton::builder().label("Saved").build();
        filter_saved.add_css_class("toggle");

        filter_24.set_group(Some(&filter_all));
        filter_5.set_group(Some(&filter_all));
        filter_saved.set_group(Some(&filter_all));
        filter_all.set_active(true);

        filter_row.append(&filter_all);
        filter_row.append(&filter_24);
        filter_row.append(&filter_5);
        filter_row.append(&filter_saved);

        search_filter_box.append(&filter_row);
        content.append(&search_filter_box);

        // Networks Header with Refresh Button
        let header_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        header_box.set_margin_top(12);

        let networks_label = gtk4::Label::builder()
            .label("Networks")
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .build();
        networks_label.add_css_class("title-4");

        let spinner = gtk4::Spinner::new();
        spinner.add_css_class("big-spinner");
        spinner.set_size_request(28, 28);
        spinner.set_visible(false);

        let operation_status_label = gtk4::Label::new(None);
        operation_status_label.set_halign(gtk4::Align::Start);
        operation_status_label.set_opacity(0.7);
        operation_status_label.set_visible(false);

        let hidden_network_button = gtk4::Button::builder()
            .label("Hidden Network")
            .tooltip_text("Connect to a hidden Wi-Fi network")
            .css_classes(vec!["flat".to_string(), "touch-target".to_string()])
            .build();

        let refresh_button = gtk4::Button::builder()
            .icon_name(icon_name(
                "view-refresh-symbolic",
                &["view-refresh", "reload-symbolic"][..],
            ))
            .tooltip_text("Refresh networks")
            .css_classes(vec![
                "flat".to_string(),
                "circular".to_string(),
                "touch-target".to_string(),
            ])
            .build();

        header_box.append(&networks_label);
        header_box.append(&spinner);
        header_box.append(&hidden_network_button);
        header_box.append(&refresh_button);
        content.append(&header_box);
        content.append(&operation_status_label);

        // Connected network card
        let connected_card = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        connected_card.add_css_class("connected-card");
        connected_card.set_margin_top(8);
        connected_card.set_visible(false);
        connected_card.set_can_target(true);

        let connected_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let connected_ssid = gtk4::Label::new(None);
        connected_ssid.add_css_class("connected-ssid");
        connected_ssid.set_xalign(0.0);
        connected_ssid.set_hexpand(true);

        let details_button = gtk4::Button::builder()
            .label("Details")
            .tooltip_text("Connection details")
            .css_classes(vec!["flat".to_string()])
            .build();

        connected_header.append(&connected_ssid);
        connected_header.append(&details_button);
        connected_card.append(&connected_header);

        let connected_subtitle = gtk4::Label::new(None);
        connected_subtitle.set_xalign(0.0);
        connected_subtitle.set_wrap(true);
        connected_subtitle.add_css_class("connected-subtitle");
        connected_card.append(&connected_subtitle);

        let details_revealer = gtk4::Revealer::new();
        details_revealer.set_transition_type(gtk4::RevealerTransitionType::Crossfade);
        let expand_connected_details = app_state.expand_connected_details();
        details_revealer.set_reveal_child(expand_connected_details);

        let details_clamp = adw::Clamp::builder()
            .maximum_size(520)
            .tightening_threshold(380)
            .build();
        let details_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);

        let details_ip = gtk4::Label::new(Some("IP: —"));
        details_ip.set_xalign(0.0);
        details_ip.add_css_class("detail-label");
        details_ip.add_css_class("detail-ip");
        let details_dns = gtk4::Label::new(Some("DNS: —"));
        details_dns.set_xalign(0.0);
        details_dns.add_css_class("detail-label");
        let details_speed = gtk4::Label::new(Some("Speed: —"));
        details_speed.set_xalign(0.0);
        details_speed.add_css_class("detail-label");

        details_box.append(&details_ip);
        details_box.append(&details_dns);
        details_box.append(&details_speed);

        details_clamp.set_child(Some(&details_box));
        details_revealer.set_child(Some(&details_clamp));
        connected_card.append(&details_revealer);

        content.append(&connected_card);

        // Known Networks section
        let known_header = gtk4::Label::new(Some("Known Networks"));
        known_header.add_css_class("section-header");
        known_header.set_xalign(0.0);
        known_header.set_margin_top(8);
        known_header.set_visible(false);

        let known_list = gtk4::ListBox::builder()
            .css_classes(vec!["boxed-list".to_string()])
            .selection_mode(gtk4::SelectionMode::None)
            .margin_top(4)
            .build();
        known_list.set_visible(false);

        // Other Networks section
        let other_header = gtk4::Label::new(Some("Other Networks"));
        other_header.add_css_class("section-header");
        other_header.set_xalign(0.0);
        other_header.set_margin_top(8);
        other_header.set_visible(false);

        let other_list = gtk4::ListBox::builder()
            .css_classes(vec!["boxed-list".to_string()])
            .selection_mode(gtk4::SelectionMode::None)
            .margin_top(4)
            .build();
        other_list.set_visible(false);

        // Empty State
        let empty_state = adw::StatusPage::builder()
            .icon_name(icon_name(
                "network-wireless-disabled-symbolic",
                &[
                    "network-wireless-symbolic",
                    "network-wireless-offline-symbolic",
                ][..],
            ))
            .title("No Networks Found")
            .description("Turn on Wi-Fi or refresh to scan for networks")
            .build();
        empty_state.set_visible(false);

        let empty_action = gtk4::Button::builder()
            .label("Turn On Wi-Fi")
            .css_classes(vec!["suggested-action".to_string()])
            .build();
        empty_action.set_visible(false);

        content.append(&known_header);
        content.append(&known_list);
        content.append(&other_header);
        content.append(&other_list);
        content.append(&empty_state);
        content.append(&empty_action);

        clamp.set_child(Some(&content));
        scrolled.set_child(Some(&clamp));
        toast_overlay.set_child(Some(&scrolled));
        widget.append(&toast_overlay);

        let page = Self {
            widget,
            toast_overlay,
            wifi_switch: wifi_switch.clone(),
            search_entry: search_entry.clone(),
            hidden_network_button: hidden_network_button.clone(),
            refresh_button: refresh_button.clone(),
            spinner: spinner.clone(),
            operation_status_label: operation_status_label.clone(),
            connected_card: connected_card.clone(),
            connected_ssid: connected_ssid.clone(),
            connected_subtitle: connected_subtitle.clone(),
            connected_details_revealer: details_revealer.clone(),
            connected_details_ip: details_ip.clone(),
            connected_details_dns: details_dns.clone(),
            connected_details_speed: details_speed.clone(),
            known_header: known_header.clone(),
            known_list: known_list.clone(),
            other_header: other_header.clone(),
            other_list: other_list.clone(),
            empty_state: empty_state.clone(),
            empty_action: empty_action.clone(),
            filter_all: filter_all.clone(),
            filter_24: filter_24.clone(),
            filter_5: filter_5.clone(),
            filter_saved: filter_saved.clone(),
            app_state: app_state.clone(),
        };

        page.apply_expand_details_setting(expand_connected_details);

        // Connected details toggle
        let page_ref = page.clone();
        details_button.connect_clicked(move |_| {
            let page = page_ref.clone();
            let reveal = !page.connected_details_revealer.reveals_child();
            page.connected_details_revealer.set_reveal_child(reveal);
            if reveal {
                page.refresh_connected_details();
            }
        });

        // Context menu for connected card
        let page_ref = page.clone();
        let connected_card_widget = page.connected_card.clone().upcast::<gtk4::Widget>();
        let connected_card = page.connected_card.clone();
        let connected_menu_gesture = gtk4::GestureClick::new();
        connected_menu_gesture.set_button(3);
        connected_menu_gesture.connect_released(move |_gesture, _n_press, x, y| {
            if let Some(network) = page_ref.app_state.wifi_connected_network() {
                page_ref.show_context_menu(&connected_card_widget, &network, x, y);
            }
        });
        connected_card.add_controller(connected_menu_gesture);

        // Filter buttons
        let page_ref = page.clone();
        filter_all.connect_toggled(move |btn| {
            if btn.is_active() {
                page_ref
                    .app_state
                    .set_wifi_filter_state(WifiFilterState::All);
                page_ref.update_filtered_networks();
            }
        });
        let page_ref = page.clone();
        filter_24.connect_toggled(move |btn| {
            if btn.is_active() {
                page_ref
                    .app_state
                    .set_wifi_filter_state(WifiFilterState::Band24);
                page_ref.update_filtered_networks();
            }
        });
        let page_ref = page.clone();
        filter_5.connect_toggled(move |btn| {
            if btn.is_active() {
                page_ref
                    .app_state
                    .set_wifi_filter_state(WifiFilterState::Band5);
                page_ref.update_filtered_networks();
            }
        });
        let page_ref = page.clone();
        filter_saved.connect_toggled(move |btn| {
            if btn.is_active() {
                page_ref
                    .app_state
                    .set_wifi_filter_state(WifiFilterState::Saved);
                page_ref.update_filtered_networks();
            }
        });

        // Check initial WiFi state
        let page_ref = page.clone();
        glib::spawn_future_local(async move {
            match nm::is_wifi_enabled().await {
                Ok(enabled) => {
                    page_ref.wifi_switch.set_active(enabled);
                    page_ref.update_filter_controls(enabled);
                    if enabled {
                        page_ref.refresh_networks(false).await;
                    } else {
                        page_ref.load_saved_connections().await;
                        page_ref.update_filtered_networks();
                    }
                }
                Err(e) => {
                    log::error!("Failed to check WiFi state: {}", e);
                }
            }
        });

        // WiFi switch handler
        let page_ref = page.clone();
        wifi_switch.connect_active_notify(move |switch| {
            let enabled = switch.is_active();
            let page = page_ref.clone();

            page.update_filter_controls(enabled);
            if enabled && page.filter_saved.is_active() {
                page.filter_all.set_active(true);
            }

            glib::spawn_future_local(async move {
                match nm::set_wifi_enabled(enabled).await {
                    Ok(_) => {
                        if enabled {
                            page.refresh_networks(false).await;
                        } else {
                            page.app_state.clear_wifi_all_networks();
                            page.load_saved_connections().await;
                            page.update_filtered_networks();
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to toggle WiFi: {}", e);
                        page.show_toast(&format!("Failed to toggle WiFi: {}", e));
                    }
                }
            });
        });

        // Search handler
        let page_ref = page.clone();
        search_entry.connect_search_changed(move |entry| {
            let text = entry.text().to_string();
            page_ref.app_state.set_wifi_search_text(text.to_lowercase());
            if let Some(source) = page_ref.app_state.take_wifi_search_debounce_source() {
                source.remove();
            }
            let page = page_ref.clone();
            let source =
                glib::timeout_add_local(std::time::Duration::from_millis(300), move || {
                    page.update_filtered_networks();
                    glib::ControlFlow::Break
                });
            page_ref
                .app_state
                .set_wifi_search_debounce_source(Some(source));
        });

        // Refresh button handler
        let page_ref = page.clone();
        refresh_button.connect_clicked(move |_| {
            let page = page_ref.clone();
            glib::spawn_future_local(async move {
                page.refresh_networks(true).await;
            });
        });

        let page_ref = page.clone();
        hidden_network_button.connect_clicked(move |_| {
            let page = page_ref.clone();
            glib::spawn_future_local(async move {
                page.show_hidden_network_dialog().await;
            });
        });

        let page_ref = page.clone();
        empty_action.connect_clicked(move |_| {
            let page = page_ref.clone();
            page.wifi_switch.set_active(true);
        });

        page.set_page_visible(false);
        page
    }

    pub fn set_page_visible(&self, visible: bool) {
        self.app_state.set_page_visible(PageKind::Wifi, visible);
        if visible {
            self.start_auto_refresh();
            let page = self.clone();
            glib::spawn_future_local(async move {
                page.refresh_networks(false).await;
            });
        } else {
            self.stop_auto_refresh();
        }
    }

    fn start_auto_refresh(&self) {
        if self.app_state.wifi_has_refresh_source() {
            return;
        }

        let page_ref = self.clone();
        let source = glib::timeout_add_seconds_local(15, move || {
            let page = page_ref.clone();
            if page.wifi_switch.is_active()
                && page.app_state.auto_scan_enabled()
                && page.app_state.is_page_visible(PageKind::Wifi)
            {
                glib::spawn_future_local(async move {
                    page.refresh_networks(false).await;
                });
            }
            glib::ControlFlow::Continue
        });

        self.app_state.set_wifi_refresh_source(Some(source));
    }

    fn stop_auto_refresh(&self) {
        if let Some(source) = self.app_state.take_wifi_refresh_source() {
            source.remove();
        }
    }

    fn set_busy(&self, busy: bool, status_message: Option<&str>) {
        let next = if busy {
            self.app_state.wifi_busy_count_inc()
        } else {
            self.app_state.wifi_busy_count_dec()
        };
        if busy {
            if next == 1 {
                common::set_busy(
                    &self.spinner,
                    &self.operation_status_label,
                    Some(&self.refresh_button),
                    true,
                    status_message,
                );
                self.hidden_network_button.set_sensitive(false);
            } else if let Some(message) = status_message {
                self.operation_status_label.set_text(message);
            }
            return;
        }

        if next == 0 {
            common::set_busy(
                &self.spinner,
                &self.operation_status_label,
                Some(&self.refresh_button),
                false,
                None,
            );
            self.refresh_button
                .set_sensitive(self.wifi_switch.is_active());
            self.hidden_network_button
                .set_sensitive(self.wifi_switch.is_active());
        }
    }

    fn busy_guard(&self, status_message: &'static str) -> BusyGuard {
        self.set_busy(true, Some(status_message));
        BusyGuard { page: self.clone() }
    }

    fn update_filter_controls(&self, wifi_enabled: bool) {
        self.filter_saved.set_visible(true);
        self.filter_all.set_sensitive(wifi_enabled);
        self.filter_24.set_sensitive(wifi_enabled);
        self.filter_5.set_sensitive(wifi_enabled);
        self.filter_saved.set_sensitive(true);
        self.hidden_network_button.set_sensitive(wifi_enabled);

        if !wifi_enabled {
            self.filter_saved.set_active(true);
        }
    }

    fn is_band_24(network: &WifiNetwork) -> bool {
        let freq_mhz = if network.freq_mhz >= 1_000_000_000 {
            network.freq_mhz / 1_000_000
        } else if network.freq_mhz >= 1_000_000 {
            network.freq_mhz / 1_000
        } else {
            network.freq_mhz
        };

        if (2400..=2500).contains(&freq_mhz) {
            return true;
        }
        if (1..=14).contains(&network.channel) {
            return true;
        }
        let band = network
            .band
            .to_lowercase()
            .replace(' ', "")
            .replace(',', ".");
        band.contains("2.4") || band.contains("2g")
    }

    fn is_band_5(network: &WifiNetwork) -> bool {
        let freq_mhz = if network.freq_mhz >= 1_000_000_000 {
            network.freq_mhz / 1_000_000
        } else if network.freq_mhz >= 1_000_000 {
            network.freq_mhz / 1_000
        } else {
            network.freq_mhz
        };

        if (4900..5925).contains(&freq_mhz) {
            return true;
        }
        if (36..=177).contains(&network.channel) {
            return true;
        }
        let band = network
            .band
            .to_lowercase()
            .replace(' ', "")
            .replace(',', ".");
        band.contains("5") && !band.contains("2.4") && !band.contains("6")
    }

    fn sort_networks_stably(networks: &mut [WifiNetwork]) {
        networks.sort_by(Self::compare_network_rows);
    }

    fn compare_network_rows(a: &WifiNetwork, b: &WifiNetwork) -> Ordering {
        let ssid_cmp = a.ssid.to_lowercase().cmp(&b.ssid.to_lowercase());
        if ssid_cmp != Ordering::Equal {
            return ssid_cmp;
        }

        let band_cmp =
            Self::network_band_sort_key(&a.band).cmp(&Self::network_band_sort_key(&b.band));
        if band_cmp != Ordering::Equal {
            return band_cmp;
        }

        let security_cmp = Self::network_security_sort_key(&a.security_type)
            .cmp(&Self::network_security_sort_key(&b.security_type));
        if security_cmp != Ordering::Equal {
            return security_cmp;
        }

        b.signal
            .cmp(&a.signal)
            .then_with(|| a.channel.cmp(&b.channel))
            .then_with(|| a.band.cmp(&b.band))
            .then_with(|| a.security_type.cmp(&b.security_type))
    }

    fn network_band_sort_key(band: &str) -> u8 {
        let normalized = band.to_lowercase();
        if normalized == "saved" {
            0
        } else if normalized.contains("2.4") {
            1
        } else if normalized.contains("5")
            && !normalized.contains("2.4")
            && !normalized.contains("6")
        {
            2
        } else if normalized.contains('6') {
            3
        } else {
            4
        }
    }

    fn network_security_sort_key(security: &str) -> u8 {
        let normalized = security.to_lowercase();
        if normalized == "saved" {
            0
        } else if normalized.contains("open") {
            1
        } else if normalized.contains("wep") {
            2
        } else if normalized.contains("wpa") {
            3
        } else {
            4
        }
    }

    async fn refresh_networks(&self, show_feedback: bool) {
        let _busy = self.busy_guard("Refreshing...");
        if show_feedback {
            self.known_list.add_css_class("list-loading");
            self.other_list.add_css_class("list-loading");
        }

        self.load_saved_connections().await;

        match nm::scan_networks().await {
            Ok(networks) => {
                self.app_state.set_wifi_all_networks(networks);
                self.update_filtered_networks();
            }
            Err(e) => {
                log::error!("Failed to scan networks: {}", e);
                if nm::is_nmcli_retrieval_error(&e.to_string()) {
                    self.show_toast(nm::NMCLI_RETRIEVAL_TOAST);
                } else {
                    self.show_toast(&format!("Failed to scan: {}", e));
                }
                self.update_filtered_networks();
            }
        }

        if show_feedback {
            self.known_list.remove_css_class("list-loading");
            self.other_list.remove_css_class("list-loading");
        }
    }

    async fn load_saved_connections(&self) {
        match nm::get_saved_connections().await {
            Ok(saved) => {
                let mut set = HashSet::new();
                for conn in saved {
                    set.insert(conn.ssid);
                }
                self.app_state.set_wifi_saved_ssids(set);
            }
            Err(e) => {
                log::warn!("Failed to load saved networks: {}", e);
                self.app_state.clear_wifi_saved_ssids();
            }
        }
    }

    fn update_filtered_networks(&self) {
        let all_nets = self.app_state.wifi_all_networks();
        let search = self.app_state.wifi_search_text();
        let saved = self.app_state.wifi_saved_ssids();
        let filter_state = self.app_state.wifi_filter_state();
        let wifi_enabled = self.wifi_switch.is_active();
        let connected = all_nets.iter().find(|n| n.connected).cloned();

        let filtered: Vec<WifiNetwork> = match filter_state {
            WifiFilterState::Saved => {
                let mut list = Vec::new();
                let mut seen_saved: HashSet<String> = HashSet::new();

                for net in all_nets.iter().filter(|net| saved.contains(&net.ssid)) {
                    let search_match = if search.is_empty() {
                        true
                    } else {
                        net.ssid.to_lowercase().contains(&search)
                    };
                    if search_match {
                        list.push(net.clone());
                    }
                    seen_saved.insert(net.ssid.clone());
                }

                let mut missing_saved: Vec<String> = saved.iter().cloned().collect();
                missing_saved.sort_by_key(|ssid| ssid.to_lowercase());

                for ssid in missing_saved {
                    if seen_saved.contains(&ssid) {
                        continue;
                    }
                    let search_match = if search.is_empty() {
                        true
                    } else {
                        ssid.to_lowercase().contains(&search)
                    };
                    if !search_match {
                        continue;
                    }
                    list.push(WifiNetwork {
                        ssid,
                        signal: 0,
                        secured: true,
                        connected: false,
                        band: "Saved".to_string(),
                        channel: 0,
                        freq_mhz: 0,
                        security_type: "Saved".to_string(),
                    });
                }

                Self::sort_networks_stably(&mut list);
                list
            }
            _ => {
                if !wifi_enabled {
                    Vec::new()
                } else {
                    all_nets
                        .iter()
                        .filter(|net| {
                            // Search filter
                            let search_match = if search.is_empty() {
                                true
                            } else {
                                net.ssid.to_lowercase().contains(&search)
                            };

                            let filter_match = match filter_state {
                                WifiFilterState::All => true,
                                WifiFilterState::Band24 => Self::is_band_24(net),
                                WifiFilterState::Band5 => Self::is_band_5(net),
                                WifiFilterState::Saved => saved.contains(&net.ssid),
                            };

                            search_match && filter_match
                        })
                        .cloned()
                        .collect()
                }
            }
        };

        self.populate_networks(filtered, connected);
    }

    fn populate_networks(&self, networks: Vec<WifiNetwork>, connected: Option<WifiNetwork>) {
        self.clear_networks();

        if let Some(ref network) = connected {
            self.app_state
                .set_wifi_connected_network(Some(network.clone()));
            self.update_connected_card(network);
            self.connected_card.set_visible(true);
            self.connected_card.add_css_class("fade-in");
            if self.connected_details_revealer.reveals_child() {
                self.refresh_connected_details();
            }
        }

        self.empty_state.set_visible(false);
        self.empty_action.set_visible(false);

        let saved = self.app_state.wifi_saved_ssids();
        let mut known = Vec::new();
        let mut other = Vec::new();

        for network in networks {
            if connected
                .as_ref()
                .map(|c| {
                    c.connected
                        && c.ssid == network.ssid
                        && c.band == network.band
                        && c.security_type == network.security_type
                })
                .unwrap_or(false)
            {
                continue;
            }

            if saved.contains(&network.ssid) {
                known.push(network);
            } else {
                other.push(network);
            }
        }

        Self::sort_networks_stably(&mut known);
        Self::sort_networks_stably(&mut other);

        for network in known {
            let row = self.create_network_row(&network);
            self.known_list.append(&row);
        }

        for network in other {
            let row = self.create_network_row(&network);
            self.other_list.append(&row);
        }

        let show_known = self.known_list.first_child().is_some();
        let show_other = self.other_list.first_child().is_some();

        self.known_header.set_visible(show_known);
        self.known_list.set_visible(show_known);
        self.other_header.set_visible(show_other);
        self.other_list.set_visible(show_other);

        if !show_known && !show_other && connected.is_none() {
            let wifi_enabled = self.wifi_switch.is_active();
            if wifi_enabled {
                self.empty_state.set_title("No Networks Found");
                self.empty_state
                    .set_description(Some("Refresh or wait for networks to appear"));
                self.empty_action.set_visible(false);
            } else {
                self.empty_state.set_title("Wi-Fi is Off");
                self.empty_state
                    .set_description(Some("Turn on Wi-Fi to scan for networks"));
                self.empty_action.set_visible(true);
            }
            self.empty_state.set_visible(true);
        }
    }

    fn update_connected_card(&self, network: &WifiNetwork) {
        self.connected_ssid.set_text(&network.ssid);
        let signal_text = get_signal_strength_text(network.signal);
        let subtitle = format!(
            "Connected • {} • {} • Channel {}",
            signal_text, network.band, network.channel
        );
        self.connected_subtitle.set_text(&subtitle);
        if self.app_state.expand_connected_details() {
            self.apply_expand_details_setting(true);
        }
    }

    fn refresh_connected_details(&self) {
        let network = self.app_state.wifi_connected_network();
        let details_ip = self.connected_details_ip.clone();
        let details_dns = self.connected_details_dns.clone();
        let details_speed = self.connected_details_speed.clone();

        if let Some(net) = network {
            glib::spawn_future_local(async move {
                let info = nm::get_network_info(&net.ssid).await.ok();

                let ip = info
                    .as_ref()
                    .and_then(|i| i.ip_address.clone())
                    .unwrap_or_else(|| "—".to_string());

                let dns = info
                    .as_ref()
                    .map(|i| {
                        if i.dns.is_empty() {
                            "—".to_string()
                        } else {
                            i.dns.join(", ")
                        }
                    })
                    .unwrap_or_else(|| "—".to_string());

                let speed = info
                    .as_ref()
                    .and_then(|i| i.link_speed_mbps)
                    .map(|s| format!("{} Mbps", s))
                    .unwrap_or_else(|| "—".to_string());

                details_ip.set_text(&format!("IP: {}", ip));
                details_dns.set_text(&format!("DNS: {}", dns));
                details_speed.set_text(&format!("Speed: {}", speed));
            });
        } else {
            self.connected_details_ip.set_text("IP: —");
            self.connected_details_dns.set_text("DNS: —");
            self.connected_details_speed.set_text("Speed: —");
        }
    }

    pub fn apply_expand_details_setting(&self, enabled: bool) {
        self.connected_details_revealer.set_reveal_child(enabled);
        if enabled {
            self.refresh_connected_details();
        }
    }

    fn create_network_row(&self, network: &WifiNetwork) -> adw::ActionRow {
        let row = adw::ActionRow::new();
        row.set_title(&network.ssid);

        // Subtitle with details
        let subtitle = if network.band == "Saved" {
            "Saved network".to_string()
        } else {
            let signal_text = get_signal_strength_text(network.signal);
            let channel_text = if network.channel == 0 {
                "Channel —".to_string()
            } else {
                format!("Channel {}", network.channel)
            };
            if network.connected {
                format!(
                    "Connected • {} • {} • {}",
                    signal_text, network.band, channel_text
                )
            } else {
                format!("{} • {} • {}", signal_text, network.band, channel_text)
            }
        };
        row.set_subtitle(&subtitle[..]);

        // Signal icon
        let signal_icon = gtk4::Image::new();
        let signal_icon_name = get_signal_icon(network.signal);
        signal_icon.set_icon_name(Some(icon_name(
            signal_icon_name,
            &["network-wireless-symbolic", "network-wireless"][..],
        )));
        signal_icon.set_pixel_size(24);
        row.add_prefix(&signal_icon);

        row.add_css_class("fade-in");

        // Security icon
        if network.secured {
            let security_icon = gtk4::Image::new();
            security_icon.set_icon_name(Some(icon_name(
                "changes-prevent-symbolic",
                &["emblem-readonly-symbolic", "changes-allow-symbolic"][..],
            )));
            security_icon.set_pixel_size(16);
            security_icon.set_opacity(0.7);
            row.add_suffix(&security_icon);
        }

        // Connected indicator
        if network.connected {
            let connected_icon = gtk4::Image::new();
            connected_icon.set_icon_name(Some(icon_name(
                "emblem-ok-symbolic",
                &["emblem-default-symbolic", "object-select-symbolic"][..],
            )));
            connected_icon.set_pixel_size(16);
            row.add_suffix(&connected_icon);
        }

        // Right-click menu
        self.add_context_menu(&row.clone().upcast::<gtk4::Widget>(), network);

        // Click handler
        let page = self.clone();
        let network = network.clone();
        row.set_activatable(true);
        row.connect_activated(move |_| {
            let page = page.clone();
            let network = network.clone();

            glib::spawn_future_local(async move {
                if network.connected {
                    page.show_network_info_dialog(&network).await;
                } else {
                    page.handle_network_click(&network).await;
                }
            });
        });

        row
    }

    fn add_context_menu(&self, widget: &gtk4::Widget, network: &WifiNetwork) {
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(3); // Right click

        let network_for_menu = network.clone();
        let page_for_menu = self.clone();
        let widget_for_menu = widget.clone();

        gesture.connect_released(move |_gesture, _n_press, x, y| {
            page_for_menu.show_context_menu(&widget_for_menu, &network_for_menu, x, y);
        });

        widget.add_controller(gesture);
    }

    fn show_context_menu(&self, widget: &gtk4::Widget, network: &WifiNetwork, x: f64, y: f64) {
        let popover = gtk4::Popover::new();
        popover.set_position(gtk4::PositionType::Bottom);
        popover.set_has_arrow(false);
        popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));

        let menu_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        menu_box.add_css_class("menu");
        menu_box.set_margin_top(6);
        menu_box.set_margin_bottom(6);

        if network.connected {
            // Disconnect button
            let disconnect_btn = gtk4::Button::builder()
                .label("Disconnect")
                .css_classes(vec!["flat".to_string()])
                .build();

            let page_disc = self.clone();
            let popover_disc = popover.clone();

            disconnect_btn.connect_clicked(move |_| {
                let page = page_disc.clone();
                popover_disc.popdown();

                glib::spawn_future_local(async move {
                    page.disconnect_network().await;
                });
            });

            menu_box.append(&disconnect_btn);
        } else {
            // Connect button
            let connect_btn = gtk4::Button::builder()
                .label("Connect")
                .css_classes(vec!["flat".to_string()])
                .build();

            let page_conn = self.clone();
            let network_conn = network.clone();
            let popover_conn = popover.clone();

            connect_btn.connect_clicked(move |_| {
                let page = page_conn.clone();
                let network = network_conn.clone();
                popover_conn.popdown();

                glib::spawn_future_local(async move {
                    page.handle_network_click(&network).await;
                });
            });

            menu_box.append(&connect_btn);
        }

        // Show QR code (only if a saved password exists)
        let qr_btn = gtk4::Button::builder()
            .label("Show QR Code")
            .css_classes(vec!["flat".to_string()])
            .build();
        qr_btn.set_visible(false);

        let page_qr = self.clone();
        let network_qr = network.clone();
        let popover_qr = popover.clone();

        qr_btn.connect_clicked(move |_| {
            let page = page_qr.clone();
            let network = network_qr.clone();
            popover_qr.popdown();

            glib::spawn_future_local(async move {
                page.show_qr_code(&network).await;
            });
        });

        let qr_btn_state = qr_btn.clone();
        let ssid_check = network.ssid.clone();
        glib::spawn_future_local(async move {
            let is_saved = nm::is_network_saved(&ssid_check).await.unwrap_or(false);
            if is_saved {
                qr_btn_state.set_visible(true);
            }
        });

        menu_box.append(&qr_btn);

        // Show Network Info button
        let info_btn = gtk4::Button::builder()
            .label("Network Details")
            .css_classes(vec!["flat".to_string()])
            .build();

        let page_info = self.clone();
        let network_info = network.clone();
        let popover_info = popover.clone();

        info_btn.connect_clicked(move |_| {
            let page = page_info.clone();
            let network = network_info.clone();
            popover_info.popdown();

            glib::spawn_future_local(async move {
                page.show_network_info_dialog(&network).await;
            });
        });

        menu_box.append(&info_btn);

        // Forget button
        let forget_btn = gtk4::Button::builder()
            .css_classes(vec!["flat".to_string(), "destructive-action".to_string()])
            .build();
        forget_btn.set_sensitive(false);
        forget_btn.set_visible(false);

        let forget_content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let forget_icon = gtk4::Image::from_icon_name(icon_name(
            "user-trash-symbolic",
            &["user-trash", "edit-delete-symbolic"][..],
        ));
        forget_icon.set_pixel_size(22);
        forget_icon.add_css_class("forget-icon");
        let forget_label = gtk4::Label::new(Some("Forget Network"));
        forget_label.set_xalign(0.0);
        forget_content.append(&forget_icon);
        forget_content.append(&forget_label);
        forget_btn.set_child(Some(&forget_content));

        let page_forget = self.clone();
        let ssid_forget = network.ssid.clone();
        let popover_forget = popover.clone();

        forget_btn.connect_clicked(move |_| {
            let page = page_forget.clone();
            let ssid = ssid_forget.clone();
            popover_forget.popdown();

            glib::spawn_future_local(async move {
                page.forget_network(&ssid).await;
            });
        });

        let forget_btn_state = forget_btn.clone();
        let ssid_check = network.ssid.clone();
        glib::spawn_future_local(async move {
            let is_saved = nm::is_network_saved(&ssid_check).await.unwrap_or(false);
            if is_saved {
                forget_btn_state.set_visible(true);
                forget_btn_state.set_sensitive(true);
            }
        });

        menu_box.append(&forget_btn);

        popover.set_child(Some(&menu_box));
        popover.set_parent(widget);
        popover.popup();
    }

    async fn handle_network_click(&self, network: &WifiNetwork) {
        if !network.secured {
            self.connect_open_network(&network.ssid).await;
        } else {
            // Check if network is saved
            match nm::is_network_saved(&network.ssid).await {
                Ok(true) => {
                    // Network is saved, connect directly
                    self.connect_saved_network(&network.ssid).await;
                }
                Ok(false) => {
                    // Network is not saved, show password dialog
                    self.show_password_dialog(network).await;
                }
                Err(e) => {
                    log::error!("Failed to check if network is saved: {}", e);
                    // Show password dialog as fallback
                    self.show_password_dialog(network).await;
                }
            }
        }
    }

    async fn show_password_dialog(&self, network: &WifiNetwork) {
        self.show_password_dialog_for_ssid(&network.ssid, Some(&network.security_type))
            .await;
    }

    async fn show_hidden_network_dialog(&self) {
        let ssid_entry = adw::EntryRow::builder()
            .title("Network Name (SSID)")
            .activates_default(true)
            .build();

        let security_model =
            gtk4::StringList::new(&["WPA/WPA2 Personal", "WPA3 Personal", "WEP", "Open"][..]);
        let security_row = adw::ComboRow::builder()
            .title("Security")
            .model(&security_model)
            .selected(0)
            .build();

        let password_entry = adw::PasswordEntryRow::builder()
            .title("Password")
            .activates_default(true)
            .build();

        let helper_label = gtk4::Label::new(Some(
            "Enter the exact hidden SSID and matching security settings",
        ));
        helper_label.set_wrap(true);
        helper_label.set_xalign(0.0);
        helper_label.add_css_class("dim-label");

        let content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        content_box.set_margin_top(12);
        content_box.set_margin_bottom(12);
        content_box.set_margin_start(12);
        content_box.set_margin_end(12);
        content_box.append(&helper_label);
        content_box.append(&ssid_entry);
        content_box.append(&security_row);
        content_box.append(&password_entry);

        let password_entry_visibility = password_entry.clone();
        security_row.connect_selected_notify(move |row| {
            let password_required = row.selected() != 3;
            password_entry_visibility.set_visible(password_required);
            if !password_required {
                password_entry_visibility.set_text("");
            }
        });

        let dialog = adw::AlertDialog::builder()
            .heading("Connect to Hidden Network")
            .body("Hidden networks do not appear in the scan list until you connect")
            .extra_child(&content_box)
            .default_response("connect")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("connect", "Connect")][..]);
        dialog.set_response_appearance("connect", adw::ResponseAppearance::Suggested);

        let response = if let Some(parent) = self.widget.root().and_downcast_ref::<gtk4::Window>() {
            dialog.choose_future(Some(parent)).await
        } else {
            dialog.choose_future(None::<&gtk4::Window>).await
        };

        if response.as_str() != "connect" {
            return;
        }

        let ssid = ssid_entry.text().to_string();
        if ssid.trim().is_empty() {
            self.show_toast("Please enter the hidden network name");
            return;
        }

        let security_type = match security_row.selected() {
            1 => Some("WPA3"),
            2 => Some("WEP"),
            3 => None,
            _ => Some("WPA/WPA2"),
        };

        let password_text = password_entry.text().to_string();
        if security_type.is_some() && password_text.is_empty() {
            self.show_toast("Please enter the network password");
            return;
        }

        let password = if password_text.is_empty() {
            None
        } else {
            Some(password_text.as_str())
        };

        self.connect_hidden_network(&ssid, password, security_type)
            .await;
    }

    async fn show_password_dialog_for_ssid(&self, ssid: &str, security_type: Option<&str>) {
        let password_entry = adw::PasswordEntryRow::builder()
            .title("Password")
            .activates_default(true)
            .build();

        let content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        content_box.set_margin_top(12);
        content_box.set_margin_bottom(12);
        content_box.set_margin_start(12);
        content_box.set_margin_end(12);
        content_box.append(&password_entry);

        let dialog = adw::AlertDialog::builder()
            .heading(format!("Connect to {}", ssid))
            .body("Enter the network password")
            .extra_child(&content_box)
            .default_response("connect")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("connect", "Connect")][..]);
        dialog.set_response_appearance("connect", adw::ResponseAppearance::Suggested);

        let response = if let Some(parent) = self.widget.root().and_downcast_ref::<gtk4::Window>() {
            dialog.choose_future(Some(parent)).await
        } else {
            dialog.choose_future(None::<&gtk4::Window>).await
        };

        if response.as_str() != "connect" {
            return;
        }

        let password = password_entry.text().to_string();
        if password.is_empty() {
            self.show_toast("Please enter a password");
            return;
        }

        self.connect_secured_network(ssid, &password, security_type)
            .await;
    }

    async fn connect_open_network(&self, ssid: &str) {
        let _busy = self.busy_guard("Connecting...");
        self.show_toast("Connecting...");

        match nm::connect_open_network(ssid).await {
            Ok(nm::ConnectStatus::Connected) => {
                self.show_toast(&format!("Connected to {}", ssid));
                self.refresh_networks(false).await;
            }
            Err(e) => {
                log::error!("Connection failed: {}", e);
                self.show_toast(&format!("Failed to connect: {}", e));
            }
        }
    }

    async fn connect_secured_network(
        &self,
        ssid: &str,
        password: &str,
        security_type: Option<&str>,
    ) {
        let _busy = self.busy_guard("Connecting...");
        self.show_toast("Connecting...");

        match nm::connect_secured_network(ssid, password, security_type).await {
            Ok(nm::ConnectStatus::Connected) => {
                self.show_toast(&format!("Connected to {}", ssid));
                self.refresh_networks(false).await;
            }
            Err(e) => {
                log::error!("Connection failed: {}", e);
                self.show_toast(&format!("Failed to connect: {}", e));
            }
        }
    }

    async fn connect_hidden_network(
        &self,
        ssid: &str,
        password: Option<&str>,
        security_type: Option<&str>,
    ) {
        let _busy = self.busy_guard("Connecting...");
        self.show_toast("Connecting to hidden network...");

        match nm::connect_hidden_network(ssid, password, security_type).await {
            Ok(nm::ConnectStatus::Connected) => {
                self.show_toast(&format!("Connected to {}", ssid));
                self.refresh_networks(false).await;
            }
            Err(e) => {
                log::error!("Hidden network connection failed: {}", e);
                self.show_toast(&format!("Failed to connect: {}", e));
            }
        }
    }

    async fn connect_saved_network(&self, ssid: &str) {
        let _busy = self.busy_guard("Connecting...");
        self.show_toast("Connecting...");
        let activation_result = nm::activate_saved_connection(ssid).await;
        match activation_result {
            Ok(nm::ConnectStatus::Connected) => {
                self.show_toast(&format!("Connected to {}", ssid));
                self.refresh_networks(false).await;
            }
            Err(e) => {
                let err_text = e.to_string();
                if nm::is_network_not_found_error(&err_text) {
                    // Fallback: ask for password explicitly (saved secret reads are disabled).
                    let security_type = self
                        .app_state
                        .wifi_all_networks()
                        .iter()
                        .find(|n| n.ssid == ssid)
                        .map(|n| n.security_type.clone());
                    let security_type = security_type.filter(|s| s != "Saved");
                    self.show_toast("Password required to connect");
                    self.show_password_dialog_for_ssid(ssid, security_type.as_deref())
                        .await;
                    return;
                }

                log::error!("Connection failed: {}", e);
                self.show_toast(&format!("Failed to connect: {}", e));
            }
        }
    }

    async fn disconnect_network(&self) {
        let _busy = self.busy_guard("Disconnecting...");
        // Get current connection
        let networks = self.app_state.wifi_all_networks();
        let connected = networks.iter().find(|n| n.connected);

        if let Some(network) = connected {
            let ssid = network.ssid.clone();
            drop(networks); // Release borrow

            match nm::disconnect_network(&ssid).await {
                Ok(_) => {
                    self.show_toast("Disconnected");
                    self.refresh_networks(false).await;
                }
                Err(e) => {
                    log::error!("Disconnect failed: {}", e);
                    self.show_toast(&format!("Failed to disconnect: {}", e));
                }
            }
        }
    }

    async fn forget_network(&self, ssid: &str) {
        let dialog = adw::AlertDialog::builder()
            .heading("Forget Network?")
            .body(format!("This will remove {} from saved networks.", ssid))
            .default_response("forget")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("forget", "Forget")][..]);
        dialog.set_response_appearance("forget", adw::ResponseAppearance::Destructive);

        let response = if let Some(parent) = self.widget.root().and_downcast_ref::<gtk4::Window>() {
            dialog.choose_future(Some(parent)).await
        } else {
            dialog.choose_future(None::<&gtk4::Window>).await
        };

        if response.as_str() != "forget" {
            return;
        }

        match nm::delete_connection_by_ssid(ssid).await {
            Ok(_) => {
                self.show_toast(&format!("Removed {}", ssid));
                self.refresh_networks(false).await;
            }
            Err(e) => {
                log::error!("Failed to forget network: {}", e);
                self.show_toast(&format!("Failed to remove: {}", e));
            }
        }
    }

    async fn show_qr_code(&self, network: &WifiNetwork) {
        let password = if network.secured {
            match self.prompt_qr_password(&network.ssid).await {
                Some(password) => password,
                None => return,
            }
        } else {
            String::new()
        };

        qr_dialog::show_qr_dialog(
            &network.ssid,
            &password,
            Some(network.security_type.as_str()),
            300,
            &self.toast_overlay,
        )
        .await;
    }

    async fn prompt_qr_password(&self, ssid: &str) -> Option<String> {
        if !nm::is_network_saved(ssid).await.unwrap_or(false) {
            self.show_toast("Save and connect to the network before generating a QR code");
            return None;
        }
        let password_entry = adw::PasswordEntryRow::builder()
            .title("Password")
            .activates_default(true)
            .build();

        let content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        content_box.set_margin_top(12);
        content_box.set_margin_bottom(12);
        content_box.set_margin_start(12);
        content_box.set_margin_end(12);
        content_box.append(&password_entry);

        let dialog = adw::AlertDialog::builder()
            .heading(format!("QR Code for {}", ssid))
            .body("Enter the network password to generate a QR code")
            .extra_child(&content_box)
            .default_response("generate")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("generate", "Generate")][..]);
        dialog.set_response_appearance("generate", adw::ResponseAppearance::Suggested);

        let response = if let Some(parent) = self.widget.root().and_downcast_ref::<gtk4::Window>() {
            dialog.choose_future(Some(parent)).await
        } else {
            dialog.choose_future(None::<&gtk4::Window>).await
        };

        if response.as_str() != "generate" {
            return None;
        }

        let password = password_entry.text().to_string();
        if password.is_empty() {
            return None;
        }

        Some(password)
    }

    async fn show_network_info_dialog(&self, network: &WifiNetwork) {
        let info = nm::get_network_info(&network.ssid).await.ok();
        let is_saved = nm::is_network_saved(&network.ssid).await.unwrap_or(false);

        let dialog = adw::Dialog::builder()
            .title("Network Details")
            .content_width(520)
            .content_height(700)
            .build();
        let parent_window = self
            .widget
            .root()
            .and_then(|root| root.downcast::<gtk4::Window>().ok());
        // * Make the network details dialog resize with the main window.
        common::make_dialog_responsive(&dialog, parent_window.as_ref(), 520, 700);

        let main_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        let nav_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        nav_box.set_margin_top(8);
        nav_box.set_margin_bottom(8);
        nav_box.set_margin_start(8);
        nav_box.set_margin_end(8);

        let back_button = gtk4::Button::builder()
            .icon_name(icon_name(
                "go-previous-symbolic",
                &["go-previous", "go-back-symbolic"][..],
            ))
            .tooltip_text("Back")
            .css_classes(vec!["flat".to_string()])
            .build();
        let dialog_close = dialog.clone();
        back_button.connect_clicked(move |_| {
            dialog_close.close();
        });

        nav_box.append(&back_button);
        main_box.append(&nav_box);

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .build();

        let info_box = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
        info_box.set_margin_top(16);
        info_box.set_margin_bottom(16);
        info_box.set_margin_start(16);
        info_box.set_margin_end(16);

        // Header section (icon, SSID, status)
        let header_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        header_box.set_halign(gtk4::Align::Center);

        let wifi_icon = gtk4::Image::new();
        wifi_icon.set_icon_name(Some(icon_name(
            get_signal_icon(network.signal),
            &["network-wireless-symbolic", "network-wireless"][..],
        )));
        wifi_icon.set_pixel_size(64);

        let ssid_label = gtk4::Label::new(Some(&network.ssid));
        ssid_label.add_css_class("title-2");

        let status_text = if network.connected {
            "Connected"
        } else {
            "Not connected"
        };
        let status_label = gtk4::Label::new(Some(status_text));
        status_label.set_opacity(0.7);

        header_box.append(&wifi_icon);
        header_box.append(&ssid_label);
        header_box.append(&status_label);
        info_box.append(&header_box);

        // Action buttons (forget / disconnect)
        let button_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 16);
        button_box.set_halign(gtk4::Align::Center);
        button_box.set_margin_top(8);
        button_box.set_margin_bottom(8);

        let build_action_button = |icon_primary: &'static str,
                                   icon_fallbacks: &'static [&'static str],
                                   label: &str,
                                   classes: &[&str]| {
            let button = gtk4::Button::builder()
                .css_classes(
                    classes
                        .iter()
                        .map(|class_name| (*class_name).to_string())
                        .collect::<Vec<String>>(),
                )
                .build();

            let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            let icon = gtk4::Image::from_icon_name(icon_name(icon_primary, icon_fallbacks));
            icon.set_pixel_size(18);
            let text = gtk4::Label::new(Some(label));
            text.set_xalign(0.0);

            content.append(&icon);
            content.append(&text);
            button.set_child(Some(&content));

            button
        };

        if is_saved {
            let forget_button = build_action_button(
                "user-trash-symbolic",
                &["user-trash", "edit-delete-symbolic"][..],
                "Forget",
                &["action-pill", "forget"][..],
            );

            let page_forget = self.clone();
            let ssid_forget = network.ssid.clone();
            forget_button.connect_clicked(move |_| {
                let page = page_forget.clone();
                let ssid = ssid_forget.clone();
                glib::spawn_future_local(async move {
                    page.forget_network(&ssid).await;
                });
            });

            button_box.append(&forget_button);
        }

        if network.connected {
            let disconnect_button = build_action_button(
                "network-offline-symbolic",
                &["window-close-symbolic", "process-stop-symbolic"][..],
                "Disconnect",
                &["action-pill", "disconnect"][..],
            );

            let page_disc = self.clone();
            disconnect_button.connect_clicked(move |_| {
                let page = page_disc.clone();
                glib::spawn_future_local(async move {
                    page.disconnect_network().await;
                });
            });

            button_box.append(&disconnect_button);
        }

        if button_box.first_child().is_some() {
            info_box.append(&button_box);
        }

        // Auto-connect (only for saved networks)
        if is_saved {
            let auto_group = adw::PreferencesGroup::builder().title("Connection").build();

            let auto_row = adw::SwitchRow::builder()
                .title("Connect automatically")
                .subtitle("Automatically connect to this network when available")
                .build();

            // Initialize state
            let current_auto = nm::get_autoconnect_for_ssid(&network.ssid)
                .await
                .unwrap_or(false);
            auto_row.set_active(current_auto);

            let page_auto = self.clone();
            let ssid_auto = network.ssid.clone();
            auto_row.connect_active_notify(move |row| {
                let page = page_auto.clone();
                let ssid = ssid_auto.clone();
                let enabled = row.is_active();

                glib::spawn_future_local(async move {
                    if let Err(e) = nm::set_autoconnect_for_ssid(&ssid, enabled).await {
                        log::error!("Failed to set autoconnect: {}", e);
                        page.show_toast(&format!("Failed to update auto-connect: {}", e));
                    }
                });
            });

            auto_group.add(&auto_row);
            info_box.append(&auto_group);
        }

        // Custom DNS (active connection only)
        let dns_group = adw::PreferencesGroup::builder().title("Custom DNS").build();

        let dns_entry = adw::EntryRow::builder().title("DNS servers").build();

        if let Some(i) = info.as_ref() {
            if !i.dns.is_empty() {
                dns_entry.set_text(&i.dns.join(", "));
            }
        }

        let search_entry = adw::EntryRow::builder().title("Search domains").build();

        let apply_button = gtk4::Button::builder()
            .label("Apply")
            .css_classes(vec!["suggested-action".to_string()])
            .build();
        apply_button.set_sensitive(network.connected);

        let apply_row = adw::ActionRow::builder()
            .title("Apply to active connection")
            .subtitle(if network.connected {
                "Reapply the connection to use custom DNS"
            } else {
                "Connect to this network to apply changes"
            })
            .build();
        apply_row.add_suffix(&apply_button);
        apply_row.set_activatable_widget(Some(&apply_button));

        let page_apply = self.clone();
        let ssid_apply = network.ssid.clone();
        let connected_apply = network.connected;
        let dns_entry_apply = dns_entry.clone();
        let search_entry_apply = search_entry.clone();
        apply_button.connect_clicked(move |_| {
            if !connected_apply {
                page_apply.show_toast("Connect to this network to apply DNS");
                return;
            }

            let dns_text = dns_entry_apply.text().to_string();
            let search_text = search_entry_apply.text().to_string();
            let dns_servers = parse_entry_list(&dns_text);
            if dns_servers.is_empty() {
                page_apply.show_toast("Enter at least one DNS server");
                return;
            }

            let invalid = invalid_ip_entries(&dns_servers);
            if !invalid.is_empty() {
                page_apply.show_toast(&format!("Invalid DNS IP: {}", invalid.join(", ")));
                return;
            }

            let search_domains = parse_entry_list(&search_text);
            let page = page_apply.clone();
            let ssid = ssid_apply.clone();

            glib::spawn_future_local(async move {
                match nm::get_active_connection_name().await {
                    Ok(Some(active)) => {
                        if active != ssid {
                            page.show_toast("Active connection does not match this network");
                            return;
                        }
                        if let Err(e) = nm::set_custom_ipv4_dns_for_connection(
                            &active,
                            &dns_servers,
                            &search_domains,
                        )
                        .await
                        {
                            page.show_toast(&format!("Failed to set DNS: {}", e));
                            return;
                        }
                        if let Err(e) = nm::reapply_connection(&active).await {
                            page.show_toast(&format!("Failed to apply connection: {}", e));
                            return;
                        }
                        page.show_toast("Custom DNS applied");
                    }
                    Ok(std::prelude::v1::None) => {
                        page.show_toast("No active connection found");
                    }
                    Err(e) => {
                        page.show_toast(&format!("Failed to get active connection: {}", e));
                    }
                }
            });
        });

        dns_group.add(&dns_entry);
        dns_group.add(&search_entry);
        dns_group.add(&apply_row);
        info_box.append(&dns_group);

        // Info items section
        let info_section = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        let items: Vec<(&'static str, String, String)> = vec![
            (
                get_signal_icon(network.signal),
                "Signal strength".to_string(),
                get_signal_strength_text(network.signal),
            ),
            (
                "network-wireless-symbolic",
                "Frequency".to_string(),
                network.band.clone(),
            ),
            (
                "network-wired-symbolic",
                "Channel".to_string(),
                network.channel.to_string(),
            ),
            (
                "security-high-symbolic",
                "Security".to_string(),
                network.security_type.clone(),
            ),
        ];

        let items_len = items.len();
        for (idx, (icon, title, subtitle)) in items.into_iter().enumerate() {
            let item_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
            item_box.set_margin_top(12);
            item_box.set_margin_bottom(12);
            item_box.set_margin_start(12);
            item_box.set_margin_end(12);

            let icon_widget = gtk4::Image::new();
            icon_widget.set_icon_name(Some(icon_name(
                icon,
                &["network-wireless-symbolic", "network-wireless"][..],
            )));
            icon_widget.set_pixel_size(24);
            icon_widget.set_opacity(0.7);

            let text_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
            text_box.set_hexpand(true);

            let title_label = gtk4::Label::new(Some(&title));
            title_label.set_xalign(0.0);
            let subtitle_label = gtk4::Label::new(Some(&subtitle));
            subtitle_label.set_xalign(0.0);
            subtitle_label.set_opacity(0.7);

            text_box.append(&title_label);
            text_box.append(&subtitle_label);

            item_box.append(&icon_widget);
            item_box.append(&text_box);

            info_section.append(&item_box);

            if idx + 1 < items_len {
                let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
                info_section.append(&separator);
            }
        }

        info_box.append(&info_section);

        // Network details section
        let details_header = gtk4::Label::new(Some("Network details"));
        details_header.set_xalign(0.0);
        details_header.set_margin_top(8);
        details_header.set_margin_bottom(8);
        info_box.append(&details_header);

        let details_card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        let mut details: Vec<(String, String)> = Vec::new();
        if let Some(i) = info.as_ref() {
            if let Some(v) = i.connection_type.as_deref() {
                details.push(("Type".to_string(), v.to_string()));
            }
            if let Some(v) = i.mac_address.as_deref() {
                details.push(("MAC address".to_string(), v.to_string()));
            }
            if let Some(v) = i.ip_address.as_deref() {
                details.push(("IP address".to_string(), v.to_string()));
            }
            if let Some(v) = i.gateway.as_deref() {
                details.push(("Gateway".to_string(), v.to_string()));
            }
            if let Some(v) = i.subnet_mask.as_deref() {
                details.push(("Subnet mask".to_string(), v.to_string()));
            }
            for (idx, dns) in i.dns.iter().enumerate() {
                let label = if idx == 0 {
                    "DNS".to_string()
                } else {
                    format!("DNS {}", idx + 1)
                };
                details.push((label, dns.to_string()));
            }
            if let Some(v) = i.ipv6_address.as_deref() {
                details.push(("IPv6 address".to_string(), v.to_string()));
            }
            if let Some(v) = i.interface.as_deref() {
                details.push(("Interface".to_string(), v.to_string()));
            }
            if let Some(v) = i.uuid.as_deref() {
                details.push(("UUID".to_string(), v.to_string()));
            }
            if let Some(seconds) = i.dhcp_lease_time_seconds {
                details.push((
                    "DHCP lease time".to_string(),
                    format!("{} seconds", seconds),
                ));
            }
        }

        if details.is_empty() {
            details.push((
                "No additional details available".to_string(),
                "Connect to this network to see IP and DNS information".to_string(),
            ));
        }

        let details_len = details.len();
        for (idx, (label, value)) in details.into_iter().enumerate() {
            let row_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            row_box.set_margin_top(12);
            row_box.set_margin_bottom(12);
            row_box.set_margin_start(12);
            row_box.set_margin_end(12);

            let label_widget = gtk4::Label::new(Some(&label));
            label_widget.set_xalign(0.0);

            let value_widget = gtk4::Label::new(Some(&value));
            value_widget.set_xalign(0.0);
            value_widget.set_opacity(0.7);
            value_widget.set_selectable(true);
            value_widget.set_wrap(true);
            value_widget.set_max_width_chars(50);

            row_box.append(&label_widget);
            row_box.append(&value_widget);
            details_card.append(&row_box);

            if idx + 1 < details_len {
                let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
                details_card.append(&separator);
            }
        }

        info_box.append(&details_card);

        scrolled.set_child(Some(&info_box));
        main_box.append(&scrolled);
        dialog.set_child(Some(&main_box));

        if let Some(parent) = parent_window.as_ref() {
            dialog.present(Some(parent));
        } else {
            dialog.present(None::<&gtk4::Window>);
        }
    }

    fn clear_networks(&self) {
        while let Some(child) = self.known_list.first_child() {
            self.known_list.remove(&child);
        }
        while let Some(child) = self.other_list.first_child() {
            self.other_list.remove(&child);
        }

        self.connected_card.set_visible(false);
        self.app_state.set_wifi_connected_network(None);
        self.known_header.set_visible(false);
        self.known_list.set_visible(false);
        self.other_header.set_visible(false);
        self.other_list.set_visible(false);
        self.empty_state.set_visible(true);
    }

    fn show_toast(&self, message: &str) {
        common::show_toast(&self.toast_overlay, message);
    }
}
