// File: devices_page.rs
// Location: /src/ui/devices_page.rs

use gtk4::prelude::*;
use gtk4::glib;
use libadwaita::{self as adw, prelude::*};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::hotspot;
use crate::ui::icon_name;
use anyhow::Result;

pub struct DevicesPage {
    pub widget: gtk4::Box,
    toast_overlay: adw::ToastOverlay,
    list_box: gtk4::ListBox,
    empty_state: adw::StatusPage,
    refresh_button: gtk4::Button,
    auto_refresh_active: std::rc::Rc<std::cell::Cell<bool>>,
    refresh_in_flight: std::rc::Rc<std::cell::Cell<bool>>,
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
    pub fn new() -> Self {
        let widget = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        let toast_overlay = adw::ToastOverlay::new();
        
        // Header with refresh button
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        header.set_margin_top(12);
        header.set_margin_bottom(12);
        header.set_margin_start(12);
        header.set_margin_end(12);
        
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
        
        header.append(&title);
        header.append(&refresh_button);
        widget.append(&header);

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .build();

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_bottom(12);

        let list_box = gtk4::ListBox::builder()
            .css_classes(vec!["boxed-list".to_string()])
            .selection_mode(gtk4::SelectionMode::None)
            .build();

        let empty_state = adw::StatusPage::builder()
            .icon_name(icon_name(
                // "network-wireless-hotspot-symbolic",
                "",
                &["network-wireless-symbolic", "network-workgroup-symbolic", "computer"][..],
            ))
            .title("Waiting for devices to connect…")
            .description("Devices will appear here when they join your hotspot")
            .build();
        empty_state.add_css_class("devices-empty");
        empty_state.set_visible(true);
        list_box.set_visible(false);

        content.append(&list_box);
        content.append(&empty_state);

        scrolled.set_child(Some(&content));
        toast_overlay.set_child(Some(&scrolled));
        widget.append(&toast_overlay);

        let auto_refresh_active = std::rc::Rc::new(std::cell::Cell::new(true));
        let refresh_in_flight = std::rc::Rc::new(std::cell::Cell::new(false));

        let page = Self {
            widget,
            toast_overlay,
            list_box,
            empty_state,
            refresh_button: refresh_button.clone(),
            auto_refresh_active: auto_refresh_active.clone(),
            refresh_in_flight: refresh_in_flight.clone(),
        };

        // Initial refresh
        let page_ref = page.clone_ref();
        glib::spawn_future_local(async move {
            page_ref.refresh_devices().await;
        });

        // Refresh button handler with spinner
        let page_ref = page.clone_ref();
        refresh_button.connect_clicked(move |btn| {
            let page = page_ref.clone_ref();
            let btn_clone = btn.clone();
            
            // Show spinner
            btn.set_icon_name(icon_name(
                "process-working-symbolic",
                &["view-refresh-symbolic", "view-refresh"][..],
            ));
            
            glib::spawn_future_local(async move {
                page.refresh_devices().await;
                
                // Restore refresh icon
                btn_clone.set_icon_name(icon_name(
                    "view-refresh-symbolic",
                    &["view-refresh", "reload-symbolic"][..],
                ));
            });
        });

        // Auto-refresh every 5 seconds
        let page_ref = page.clone_ref();
        glib::timeout_add_seconds_local(5, move || {
            if page_ref.refresh_in_flight.get() {
                return glib::ControlFlow::Continue;
            }

            let page = page_ref.clone_ref();
            glib::spawn_future_local(async move {
                match hotspot::is_hotspot_active().await {
                    Ok(true) => {
                        page.auto_refresh_active.set(true);
                        page.refresh_devices().await;
                    }
                    _ => {
                        page.show_empty_state();
                        page.auto_refresh_active.set(false);
                    }
                }
            });
            glib::ControlFlow::Continue
        });

        page
    }

    pub fn clone_ref(&self) -> Self {
        Self {
            widget: self.widget.clone(),
            toast_overlay: self.toast_overlay.clone(),
            list_box: self.list_box.clone(),
            empty_state: self.empty_state.clone(),
            refresh_button: self.refresh_button.clone(),
            auto_refresh_active: self.auto_refresh_active.clone(),
            refresh_in_flight: self.refresh_in_flight.clone(),
        }
    }

    pub async fn refresh_devices(&self) {
        if self.refresh_in_flight.get() {
            return;
        }
        self.refresh_in_flight.set(true);
        self.refresh_button.set_sensitive(false);
        self.list_box.add_css_class("list-loading");
        
        match hotspot::is_hotspot_active().await {
            Ok(true) => {
                self.auto_refresh_active.set(true);
                match self.get_connected_devices().await {
                    Ok(devices) => self.update_list(devices),
                    Err(e) => {
                        log::error!("Failed to get connected devices: {}", e);
                        self.show_empty_state();
                    }
                }
            }
            _ => {
                self.auto_refresh_active.set(false);
                self.show_empty_state();
            }
        }
        
        self.refresh_button.set_sensitive(true);
        self.list_box.remove_css_class("list-loading");
        self.refresh_in_flight.set(false);
    }

    async fn get_connected_devices(&self) -> Result<Vec<ConnectedDevice>> {
        let mut devices = Vec::new();

        // Method 1: Try to get devices from ARP table
        if let Ok(output) = tokio::process::Command::new("ip")
            .args(["neigh", "show"])
            .output()
            .await
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                // Parse format: "192.168.50.2 dev ap0 lladdr aa:bb:cc:dd:ee:ff REACHABLE"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 5 {
                    let ip = parts[0];
                    
                    if let Some(idx) = parts.iter().position(|&p| p == "lladdr") {
                        if idx + 1 < parts.len() {
                            let mac = parts[idx + 1];
                            
                            // Filter out gateway/router and invalid addresses
                            if !ip.ends_with(".1") && !ip.ends_with(".0") && !ip.starts_with("fe80") {
                                let hostname = get_hostname_for_ip(ip).await;
                                
                                devices.push(ConnectedDevice {
                                    ip: ip.to_string(),
                                    mac: mac.to_string(),
                                    hostname,
                                    lease_expiry: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Method 2: If ARP didn't work, try dnsmasq leases
        if devices.is_empty() {
            if let Ok(leases) = self.get_devices_from_leases().await {
                devices = leases;
            }
        }

        Ok(devices)
    }

    async fn get_devices_from_leases(&self) -> Result<Vec<ConnectedDevice>> {
        let mut devices = Vec::new();

        // Try NetworkManager dnsmasq leases directory
        let nm_lease_dir = std::path::Path::new("/var/lib/NetworkManager");
        if nm_lease_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(nm_lease_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with("dnsmasq-") && name_str.ends_with(".leases") {
                        if let Ok(content) = tokio::fs::read_to_string(entry.path()).await {
                            parse_lease_content(&content, &mut devices);
                        }
                    }
                }
            }
        }

        if !devices.is_empty() {
            return Ok(devices);
        }

        // Try fallback paths
        let fallback_paths = [
            "/var/lib/dnsmasq/dnsmasq.leases",
            "/var/lib/misc/dnsmasq.leases",
            "/var/db/dnsmasq.leases",
            "/tmp/dnsmasq.leases",
        ];

        for path in &fallback_paths {
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                parse_lease_content(&content, &mut devices);
                if !devices.is_empty() {
                    break;
                }
            }
        }

        Ok(devices)
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

        for device in &devices {
            let title = device.hostname.clone()
                .unwrap_or_else(|| device.ip.clone());

            let mut subtitle_parts = Vec::new();
            match &device.hostname {
                Some(_) => subtitle_parts.push(format!("{} • {}", device.ip, device.mac)),
                std::prelude::v1::None => subtitle_parts.push(device.mac.clone()),
            }

            if let Some(expiry) = device.lease_expiry {
                if let Some(lease_info) = format_lease_remaining(expiry) {
                    subtitle_parts.push(lease_info);
                }
            }

            let subtitle = subtitle_parts.join(" • ");

            let row = adw::ActionRow::builder()
                .title(&title)
                .subtitle(&subtitle)
                .build();
            row.add_css_class("fade-in");

            let icon = gtk4::Image::from_icon_name(device_icon_name(device));
            row.add_prefix(&icon);

            self.add_device_context_menu(&row, device);
            self.list_box.append(&row);
        }
    }

    fn add_device_context_menu(&self, row: &adw::ActionRow, device: &ConnectedDevice) {
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(3);

        let row_for_menu = row.clone();
        let toast_overlay = self.toast_overlay.clone();
        let device_name = device
            .hostname
            .clone()
            .unwrap_or_else(|| device.ip.clone());

        gesture.connect_released(move |_gesture, _n_press, x, y| {
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

            let block_btn = gtk4::Button::builder()
                .label("Block device")
                .css_classes(vec!["flat".to_string()])
                .build();

            let popover_block = popover.clone();
            let toast_overlay = toast_overlay.clone();
            let device_name = device_name.clone();
            block_btn.connect_clicked(move |_| {
                popover_block.popdown();
                let toast = adw::Toast::new(&format!(
                    "Block device is coming soon for {}",
                    device_name
                ));
                toast_overlay.add_toast(toast);
            });

            menu_box.append(&block_btn);
            popover.set_child(Some(&menu_box));
            popover.set_parent(&row_for_menu);
            popover.popup();
        });

        row.add_controller(gesture);
    }

    fn show_empty_state(&self) {
        self.list_box.set_visible(false);
        self.empty_state.set_visible(true);
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
            &["display-symbolic", "video-display-symbolic", "computer-symbolic"][..],
        ),
        DeviceKind::Computer => icon_name(
            "computer-symbolic",
            &["computer-apple-ipad-symbolic", "computer-old-symbolic"][..],
        ),
        DeviceKind::Iot => icon_name(
            "network-wireless-symbolic",
            &["network-workgroup-symbolic", "network-transmit-receive-symbolic"][..],
        ),
        DeviceKind::Unknown => icon_name(
            "network-wired-symbolic",
            &["network-workgroup-symbolic", "network-transmit-receive-symbolic"][..],
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
        "apple",
        "samsung",
        "huawei",
        "xiaomi",
        "oneplus",
        "oppo",
        "vivo",
        "google",
        "motorola",
        "nokia",
        "sony",
        "htc",
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

async fn get_hostname_for_ip(ip: &str) -> Option<String> {
    // Try to resolve hostname using nslookup
    if let Ok(output) = tokio::process::Command::new("nslookup")
        .arg(ip)
        .output()
        .await
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("name =") {
                if let Some(name) = line.split("name =").nth(1) {
                    let hostname = name.trim().trim_end_matches('.');
                    if !hostname.is_empty() {
                        return Some(hostname.to_string());
                    }
                }
            }
        }
    }
    None
}

fn parse_lease_content(content: &str, devices: &mut Vec<ConnectedDevice>) {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        // dnsmasq lease format: timestamp mac ip hostname
        let mac = parts[1];
        let ip = parts[2];

        // Filter out gateway
        if ip.ends_with(".1") || ip.ends_with(".0") {
            continue;
        }

        let hostname = if parts.len() > 3 && parts[3] != "*" {
            Some(parts[3].to_string())
        } else {
            None
        };

        devices.push(ConnectedDevice {
            mac: mac.to_string(),
            ip: ip.to_string(),
            hostname,
            lease_expiry: parse_lease_expiry(parts[0]),
        });
    }
}

fn parse_lease_expiry(raw: &str) -> Option<i64> {
    raw.parse::<i64>().ok()
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
