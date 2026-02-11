// File: ethernet_page.rs
// Location: /src/ui/ethernet_page.rs
//
// Credits & Inspirations:
// - GNOME Settings Network panel for UI/UX patterns

use gtk4::prelude::*;
use gtk4::glib;
use libadwaita::{self as adw, prelude::*};
use std::cell::RefCell;
use std::rc::Rc;

use crate::nm::{self, Connection, DeviceType, NetworkManager};
use crate::ui::icon_name;

pub struct EthernetPage {
    pub widget: gtk4::Box,
    toast_overlay: adw::ToastOverlay,
    ethernet_switch: adw::SwitchRow,
    refresh_button: gtk4::Button,
    spinner: gtk4::Spinner,
    connected_card: gtk4::Box,
    connected_title: gtk4::Label,
    connected_subtitle: gtk4::Label,
    list: gtk4::ListBox,
    empty_state: adw::StatusPage,
    connections: Rc<RefCell<Vec<Connection>>>,
    connected_connection: Rc<RefCell<Option<Connection>>>,
    ethernet_devices: Rc<RefCell<Vec<String>>>,
}

impl EthernetPage {
    pub fn new() -> Self {
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

        // Ethernet Toggle
        let ethernet_switch = adw::SwitchRow::builder()
            .title("Use Ethernet")
            .build();

        let switch_group = adw::PreferencesGroup::new();
        switch_group.add(&ethernet_switch);
        content.append(&switch_group);

        // Header with refresh button
        let header_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        header_box.set_margin_top(12);

        let title = gtk4::Label::builder()
            .label("Ethernet")
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .build();
        title.add_css_class("title-4");

        let spinner = gtk4::Spinner::new();
        spinner.add_css_class("big-spinner");

        let refresh_button = gtk4::Button::builder()
            .icon_name(icon_name(
                "view-refresh-symbolic",
                &["view-refresh", "reload-symbolic"][..],
            ))
            .tooltip_text("Refresh wired connections")
            .css_classes(vec![
                "flat".to_string(),
                "circular".to_string(),
                "touch-target".to_string(),
            ])
            .build();

        header_box.append(&title);
        header_box.append(&spinner);
        header_box.append(&refresh_button);
        content.append(&header_box);

        // Connected card
        let connected_card = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        connected_card.add_css_class("connected-card");
        connected_card.set_margin_top(8);
        connected_card.set_visible(false);
        connected_card.set_can_target(true);

        let connected_title = gtk4::Label::new(None);
        connected_title.add_css_class("connected-ssid");
        connected_title.set_xalign(0.0);

        let connected_subtitle = gtk4::Label::new(None);
        connected_subtitle.set_xalign(0.0);
        connected_subtitle.set_wrap(true);
        connected_subtitle.add_css_class("connected-subtitle");

        connected_card.append(&connected_title);
        connected_card.append(&connected_subtitle);

        content.append(&connected_card);

        let list = gtk4::ListBox::builder()
            .css_classes(vec!["boxed-list".to_string()])
            .selection_mode(gtk4::SelectionMode::None)
            .margin_top(8)
            .build();
        list.set_visible(false);

        let empty_state = adw::StatusPage::builder()
            .icon_name(icon_name(
                "network-wired-symbolic",
                &["network-wired", "network-transmit-receive-symbolic"][..],
            ))
            .title("No Wired Connections")
            .description("Connect an ethernet cable or create a wired profile")
            .build();
        empty_state.set_visible(false);

        content.append(&list);
        content.append(&empty_state);

        scrolled.set_child(Some(&content));
        toast_overlay.set_child(Some(&scrolled));
        widget.append(&toast_overlay);

        let connections = Rc::new(RefCell::new(Vec::new()));
        let connected_connection = Rc::new(RefCell::new(None));
        let ethernet_devices = Rc::new(RefCell::new(Vec::new()));

        let page = Self {
            widget,
            toast_overlay,
            ethernet_switch: ethernet_switch.clone(),
            refresh_button: refresh_button.clone(),
            spinner: spinner.clone(),
            connected_card: connected_card.clone(),
            connected_title: connected_title.clone(),
            connected_subtitle: connected_subtitle.clone(),
            list: list.clone(),
            empty_state: empty_state.clone(),
            connections,
            connected_connection,
            ethernet_devices,
        };

        // Connected card context menu
        let page_ref = page.clone_ref();
        let connected_card_widget = page.connected_card.clone().upcast::<gtk4::Widget>();
        let connected_card = page.connected_card.clone();
        let connected_menu_gesture = gtk4::GestureClick::new();
        connected_menu_gesture.set_button(3);
        connected_menu_gesture.connect_released(move |_gesture, _n_press, x, y| {
            if let Some(connection) = page_ref.connected_connection.borrow().clone() {
                page_ref.show_context_menu(&connected_card_widget, &connection, x, y);
            }
        });
        connected_card.add_controller(connected_menu_gesture);

        // Initial state
        let page_ref = page.clone_ref();
        glib::spawn_future_local(async move {
            match nm::is_ethernet_enabled().await {
                Ok(enabled) => {
                    page_ref.ethernet_switch.set_active(enabled);
                    page_ref.update_enabled_state(enabled);
                    if enabled {
                        page_ref.refresh_connections().await;
                    }
                }
                Err(e) => {
                    log::error!("Failed to check ethernet state: {}", e);
                }
            }
        });

        // Ethernet switch handler
        let page_ref = page.clone_ref();
        ethernet_switch.connect_active_notify(move |switch| {
            let enabled = switch.is_active();
            let page = page_ref.clone_ref();
            page.update_enabled_state(enabled);

            glib::spawn_future_local(async move {
                match nm::set_ethernet_enabled(enabled).await {
                    Ok(_) => {
                        if enabled {
                            page.refresh_connections().await;
                        } else {
                            page.clear_connections();
                            page.show_disabled_state();
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to toggle ethernet: {}", e);
                        page.show_toast(&format!("Failed to toggle ethernet: {}", e));
                    }
                }
            });
        });

        // Refresh handler
        let page_ref = page.clone_ref();
        refresh_button.connect_clicked(move |_| {
            let page = page_ref.clone_ref();
            glib::spawn_future_local(async move {
                page.refresh_connections().await;
            });
        });

        page
    }

    pub fn clone_ref(&self) -> Self {
        Self {
            widget: self.widget.clone(),
            toast_overlay: self.toast_overlay.clone(),
            ethernet_switch: self.ethernet_switch.clone(),
            refresh_button: self.refresh_button.clone(),
            spinner: self.spinner.clone(),
            connected_card: self.connected_card.clone(),
            connected_title: self.connected_title.clone(),
            connected_subtitle: self.connected_subtitle.clone(),
            list: self.list.clone(),
            empty_state: self.empty_state.clone(),
            connections: self.connections.clone(),
            connected_connection: self.connected_connection.clone(),
            ethernet_devices: self.ethernet_devices.clone(),
        }
    }

    async fn refresh_connections(&self) {
        if !self.ethernet_switch.is_active() {
            self.show_disabled_state();
            return;
        }

        self.spinner.set_visible(true);
        self.spinner.start();
        self.refresh_button.set_sensitive(false);
        self.list.add_css_class("list-loading");

        match NetworkManager::get_devices().await {
            Ok(devices) => {
                let ethernet = devices
                    .into_iter()
                    .filter(|d| d.device_type == DeviceType::Ethernet)
                    .map(|d| d.name)
                    .collect::<Vec<_>>();
                *self.ethernet_devices.borrow_mut() = ethernet;
            }
            Err(e) => {
                log::warn!("Failed to get devices: {}", e);
                self.ethernet_devices.borrow_mut().clear();
            }
        }

        match NetworkManager::get_connections().await {
            Ok(connections) => {
                let mut wired: Vec<Connection> = connections
                    .into_iter()
                    .filter(|conn| conn.is_ethernet())
                    .collect();
                wired.sort_by(|a, b| {
                    if a.active && !b.active {
                        std::cmp::Ordering::Less
                    } else if !a.active && b.active {
                        std::cmp::Ordering::Greater
                    } else {
                        a.name.cmp(&b.name)
                    }
                });
                *self.connections.borrow_mut() = wired.clone();
                self.populate_connections(wired);
            }
            Err(e) => {
                log::error!("Failed to get connections: {}", e);
                self.show_toast(&format!("Failed to refresh: {}", e));
                self.populate_connections(Vec::new());
            }
        }

        self.spinner.stop();
        self.spinner.set_visible(false);
        self.refresh_button.set_sensitive(true);
        self.list.remove_css_class("list-loading");
    }

    fn update_enabled_state(&self, enabled: bool) {
        self.refresh_button.set_sensitive(enabled);
        self.list.set_sensitive(enabled);
        if !enabled {
            self.show_disabled_state();
        }
    }

    fn show_disabled_state(&self) {
        self.clear_connections();
        self.empty_state.set_visible(true);
        self.empty_state.set_title("Ethernet is off");
        self.empty_state
            .set_description(Some("Turn on Ethernet to manage wired connections"));
    }

    fn populate_connections(&self, connections: Vec<Connection>) {
        self.clear_connections();

        let connected = connections.iter().find(|c| c.active).cloned();
        if let Some(ref conn) = connected {
            *self.connected_connection.borrow_mut() = Some(conn.clone());
            self.update_connected_card(conn);
            self.connected_card.set_visible(true);
            self.connected_card.add_css_class("fade-in");
        }

        self.empty_state.set_visible(false);

        for connection in connections {
            if connected
                .as_ref()
                .map(|c| c.active && c.name == connection.name)
                .unwrap_or(false)
            {
                continue;
            }
            let row = self.create_connection_row(&connection);
            self.list.append(&row);
        }

        let show_list = self.list.first_child().is_some();
        self.list.set_visible(show_list);

        if !show_list && connected.is_none() {
            self.empty_state.set_visible(true);
            self.empty_state.set_title("No Wired Connections");
            self.empty_state.set_description(Some(
                "Connect an ethernet cable or create a wired profile",
            ));
        }
    }

    fn clear_connections(&self) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        self.connected_card.set_visible(false);
        self.connected_connection.borrow_mut().take();
        self.empty_state.set_visible(true);
    }

    fn update_connected_card(&self, connection: &Connection) {
        self.connected_title.set_text(&connection.name);
        let device = connection
            .device
            .clone()
            .or_else(|| self.ethernet_devices.borrow().first().cloned())
            .unwrap_or_else(|| "Unknown device".to_string());
        let subtitle = format!("Connected • {}", device);
        self.connected_subtitle.set_text(&subtitle);
    }

    fn create_connection_row(&self, connection: &Connection) -> adw::ActionRow {
        let row = adw::ActionRow::new();
        row.set_title(&connection.name);

        let subtitle = if connection.active {
            connection
                .device
                .clone()
                .map(|dev| format!("Connected • {}", dev))
                .unwrap_or_else(|| "Connected".to_string())
        } else {
            "Not connected".to_string()
        };
        row.set_subtitle(&subtitle);

        let icon = gtk4::Image::new();
        icon.set_icon_name(Some(icon_name(
            "network-wired-symbolic",
            &["network-wired", "network-transmit-receive-symbolic"][..],
        )));
        icon.set_pixel_size(24);
        row.add_prefix(&icon);

        let action_button = gtk4::Button::builder()
            .label(if connection.active { "Disconnect" } else { "Connect" })
            .css_classes(vec!["flat".to_string()])
            .build();
        row.add_suffix(&action_button);
        row.set_activatable_widget(Some(&action_button));
        row.add_css_class("fade-in");

        // Context menu
        self.add_context_menu(&row.clone().upcast::<gtk4::Widget>(), connection);

        let page = self.clone_ref();
        let connection_for_action = connection.clone();
        action_button.connect_clicked(move |_| {
            let page = page.clone_ref();
            let connection = connection_for_action.clone();
            glib::spawn_future_local(async move {
                if connection.active {
                    page.disconnect_connection(&connection).await;
                } else {
                    page.connect_connection(&connection).await;
                }
            });
        });

        let page = self.clone_ref();
        let connection_for_row = connection.clone();
        row.set_activatable(true);
        row.connect_activated(move |_| {
            let page = page.clone_ref();
            let connection = connection_for_row.clone();
            glib::spawn_future_local(async move {
                if connection.active {
                    page.disconnect_connection(&connection).await;
                } else {
                    page.connect_connection(&connection).await;
                }
            });
        });

        row
    }

    fn add_context_menu(&self, widget: &gtk4::Widget, connection: &Connection) {
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(3);

        let connection_for_menu = connection.clone();
        let page_for_menu = self.clone_ref();
        let widget_for_menu = widget.clone();

        gesture.connect_released(move |_gesture, _n_press, x, y| {
            page_for_menu.show_context_menu(&widget_for_menu, &connection_for_menu, x, y);
        });

        widget.add_controller(gesture);
    }

    fn show_context_menu(&self, widget: &gtk4::Widget, connection: &Connection, x: f64, y: f64) {
        let popover = gtk4::Popover::new();
        popover.set_position(gtk4::PositionType::Bottom);
        popover.set_has_arrow(false);
        popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
            x as i32,
            y as i32,
            1,
            1,
        )));

        let menu_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        menu_box.add_css_class("menu");
        menu_box.set_margin_top(6);
        menu_box.set_margin_bottom(6);

        if connection.active {
            let disconnect_btn = gtk4::Button::builder()
                .label("Disconnect")
                .css_classes(vec!["flat".to_string()])
                .build();

            let page_disc = self.clone_ref();
            let conn_disc = connection.clone();
            let popover_disc = popover.clone();
            disconnect_btn.connect_clicked(move |_| {
                let page = page_disc.clone_ref();
                let connection = conn_disc.clone();
                popover_disc.popdown();

                glib::spawn_future_local(async move {
                    page.disconnect_connection(&connection).await;
                });
            });

            menu_box.append(&disconnect_btn);
        } else {
            let connect_btn = gtk4::Button::builder()
                .label("Connect")
                .css_classes(vec!["flat".to_string()])
                .build();

            let page_conn = self.clone_ref();
            let conn_conn = connection.clone();
            let popover_conn = popover.clone();
            connect_btn.connect_clicked(move |_| {
                let page = page_conn.clone_ref();
                let connection = conn_conn.clone();
                popover_conn.popdown();

                glib::spawn_future_local(async move {
                    page.connect_connection(&connection).await;
                });
            });

            menu_box.append(&connect_btn);
        }

        let details_btn = gtk4::Button::builder()
            .label("Connection Details")
            .css_classes(vec!["flat".to_string()])
            .build();

        let page_details = self.clone_ref();
        let conn_details = connection.clone();
        let popover_details = popover.clone();
        details_btn.connect_clicked(move |_| {
            let page = page_details.clone_ref();
            let connection = conn_details.clone();
            popover_details.popdown();

            glib::spawn_future_local(async move {
                page.show_connection_details_dialog(&connection).await;
            });
        });

        menu_box.append(&details_btn);

        let auto_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        auto_row.set_margin_top(6);
        auto_row.set_margin_bottom(6);
        auto_row.set_margin_start(12);
        auto_row.set_margin_end(12);

        let auto_label = gtk4::Label::new(Some("Auto-connect"));
        auto_label.set_xalign(0.0);
        auto_label.set_hexpand(true);

        let auto_switch = gtk4::Switch::new();
        auto_switch.set_sensitive(false);

        auto_row.append(&auto_label);
        auto_row.append(&auto_switch);
        menu_box.append(&auto_row);

        let auto_switch_state = auto_switch.clone();
        let conn_state = connection.clone();
        let page_state = self.clone_ref();
        glib::spawn_future_local(async move {
            if let Ok(current) = conn_state.autoconnect().await {
                auto_switch_state.set_active(current);
            }
            auto_switch_state.set_sensitive(true);

            let page = page_state.clone_ref();
            let conn = conn_state.clone();
            auto_switch_state.connect_active_notify(move |switch| {
                let enabled = switch.is_active();
                let page = page.clone_ref();
                let conn = conn.clone();

                glib::spawn_future_local(async move {
                    if let Err(e) = conn.set_autoconnect(enabled).await {
                        log::error!("Failed to set autoconnect: {}", e);
                        page.show_toast(&format!("Failed to update auto-connect: {}", e));
                    }
                });
            });
        });

        popover.set_child(Some(&menu_box));
        popover.set_parent(widget);
        popover.popup();
    }

    async fn connect_connection(&self, connection: &Connection) {
        self.show_toast("Connecting...");

        match connection.activate().await {
            Ok(nm::ConnectStatus::Connected) => {
                self.show_toast(&format!("Connected to {}", connection.name));
                self.refresh_connections().await;
            }
            Ok(nm::ConnectStatus::Queued) => {
                self.show_toast("Connection queued...");
                self.refresh_connections().await;
            }
            Err(e) => {
                log::error!("Connection failed: {}", e);
                self.show_toast(&format!("Failed to connect: {}", e));
            }
        }
    }

    async fn disconnect_connection(&self, connection: &Connection) {
        match connection.deactivate().await {
            Ok(_) => {
                self.show_toast("Disconnected");
                self.refresh_connections().await;
            }
            Err(e) => {
                log::error!("Disconnect failed: {}", e);
                self.show_toast(&format!("Failed to disconnect: {}", e));
            }
        }
    }

    fn show_toast(&self, message: &str) {
        let toast = adw::Toast::new(message);
        self.toast_overlay.add_toast(toast);
    }

    async fn show_connection_details_dialog(&self, connection: &Connection) {
        let info = nm::get_network_info(&connection.name).await.ok();

        let dialog = adw::Dialog::builder()
            .title("Connection Details")
            .content_width(520)
            .content_height(700)
            .build();

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

        // Header section (icon, name, status)
        let header_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        header_box.set_halign(gtk4::Align::Center);

        let icon = gtk4::Image::new();
        icon.set_icon_name(Some(icon_name(
            "network-wired-symbolic",
            &["network-wired", "network-transmit-receive-symbolic"][..],
        )));
        icon.set_pixel_size(64);

        let name_label = gtk4::Label::new(Some(&connection.name));
        name_label.add_css_class("title-2");

        let status_text = if connection.active { "Connected" } else { "Not connected" };
        let status_label = gtk4::Label::new(Some(status_text));
        status_label.set_opacity(0.7);

        header_box.append(&icon);
        header_box.append(&name_label);
        header_box.append(&status_label);
        info_box.append(&header_box);

        // Info items section
        let info_section = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let mut items: Vec<(&'static str, String, String)> = Vec::new();
        items.push((
            "network-wired-symbolic",
            "Type".to_string(),
            "Ethernet".to_string(),
        ));
        if let Some(dev) = connection.device.as_ref() {
            items.push((
                "computer-symbolic",
                "Device".to_string(),
                dev.to_string(),
            ));
        }
        items.push((
            "view-refresh-symbolic",
            "State".to_string(),
            if connection.active { "Connected" } else { "Disconnected" }.to_string(),
        ));

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
                &["network-wired-symbolic", "network-transmit-receive-symbolic"][..],
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

        if let Some(parent) = self.widget.root().and_downcast_ref::<gtk4::Window>() {
            dialog.present(Some(parent));
        } else {
            dialog.present(None::<&gtk4::Window>);
        }
    }
}
