// File: nm.rs
// Location: /src/nm.rs

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct WifiNetwork {
    pub ssid: String,
    pub signal: u8,
    pub secured: bool,
    pub connected: bool,
    pub band: String,
    pub channel: u32,
    pub freq_mhz: u32,
    pub security_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectStatus {
    Connected,
    Queued,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkInfo {
    pub connection_type: Option<String>,
    pub mac_address: Option<String>,
    pub ip_address: Option<String>,
    pub gateway: Option<String>,
    pub subnet_mask: Option<String>,
    pub dns: Vec<String>,
    pub ipv6_address: Option<String>,
    pub interface: Option<String>,
    pub link_speed_mbps: Option<u32>,
    pub state: Option<String>,
    pub uuid: Option<String>,
    pub dhcp_lease_time_seconds: Option<u32>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SavedConnection {
    pub uuid: String,
    pub ssid: String,
}

pub async fn is_wifi_enabled() -> Result<bool> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "WIFI", "radio"])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim() == "enabled")
}

pub async fn set_wifi_enabled(enabled: bool) -> Result<()> {
    let state = if enabled { "on" } else { "off" };
    let output = Command::new("nmcli")
        .args(["radio", "wifi", state])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to set WiFi state: {}", stderr));
    }
    Ok(())
}

pub async fn scan_networks() -> Result<Vec<WifiNetwork>> {
    let output = Command::new("nmcli")
        .args([
            "-t",
            "-f", "SSID,SIGNAL,SECURITY,ACTIVE,CHAN,FREQ",
            "dev", "wifi", "list",
            "--rescan", "yes",
        ])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to scan networks: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut networks_by_key: HashMap<(String, String), WifiNetwork> = HashMap::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.rsplitn(6, ':').collect();
        if parts.len() < 6 {
            continue;
        }

        let freq_str = parts[0];
        let channel_str = parts[1];
        let active_str = parts[2];
        let security = parts[3];
        let signal_str = parts[4];
        let ssid = parts[5];

        if ssid.is_empty() {
            continue;
        }

        let signal: u8 = signal_str.parse().unwrap_or(0);
        let active = active_str == "yes";
        let secured = !security.is_empty() && security != "--";
        let channel: u32 = parse_u32_from_str(channel_str);
        let freq: u32 = parse_u32_from_str(freq_str);
        
        let band = if (2400..=2500).contains(&freq) {
            "2.4 GHz".to_string()
        } else if (4900..=5900).contains(&freq) {
            "5 GHz".to_string()
        } else if (5925..=7125).contains(&freq) {
            "6 GHz".to_string()
        } else {
            "Unknown".to_string()
        };

        let security_type = if security.contains("WPA3") {
            "WPA3".to_string()
        } else if security.contains("WPA2") {
            "WPA2".to_string()
        } else if security.contains("WPA") {
            "WPA".to_string()
        } else if security.contains("WEP") {
            "WEP".to_string()
        } else if secured {
            "Secured".to_string()
        } else {
            "Open".to_string()
        };

        let ssid = ssid.to_string();
        let network = WifiNetwork {
            ssid: ssid.clone(),
            signal,
            secured,
            connected: active,
            band: band.clone(),
            channel,
            freq_mhz: freq,
            security_type,
        };

        let key = (ssid, band);
        match networks_by_key.get_mut(&key) {
            std::prelude::v1::None => {
                networks_by_key.insert(key, network);
            }
            Some(existing) => {
                if network.connected && !existing.connected {
                    *existing = network;
                } else if network.connected == existing.connected && network.signal > existing.signal {
                    *existing = network;
                }
            }
        }
    }

    let mut networks: Vec<WifiNetwork> = networks_by_key.into_values().collect();
    networks.sort_by(|a, b| {
        if a.connected && !b.connected {
            std::cmp::Ordering::Less
        } else if !a.connected && b.connected {
            std::cmp::Ordering::Greater
        } else {
            b.signal.cmp(&a.signal)
        }
    });

    Ok(networks)
}

fn parse_u32_from_str(value: &str) -> u32 {
    let digits: String = value.chars().filter(|ch| ch.is_ascii_digit()).collect();
    digits.parse().unwrap_or(0)
}

pub async fn get_network_info(ssid: &str) -> Result<NetworkInfo> {
    let mut info = NetworkInfo::default();

    // Pull what we can from a saved connection profile (if it exists).
    // This may fail for networks that aren't saved; that's OK.
    if let Ok(connection_map) = nmcli_key_value_map(&["-t", "connection", "show", ssid][..]).await {
        info.connection_type = connection_map
            .get("connection.type")
            .cloned()
            .filter(|v| !v.is_empty() && v != "--");
        info.uuid = connection_map
            .get("connection.uuid")
            .cloned()
            .filter(|v| !v.is_empty() && v != "--");

        // Prefer the AP/BSSID list if available; otherwise fall back to the profile MAC.
        info.mac_address = connection_map
            .get("802-11-wireless.seen-bssids")
            .cloned()
            .or_else(|| connection_map.get("802-11-wireless.mac-address").cloned())
            .filter(|v| !v.is_empty() && v != "--");

        info.interface = connection_map
            .get("connection.interface-name")
            .cloned()
            .filter(|v| !v.is_empty() && v != "--");
    }

    // Pull runtime IP/DNS/DHCP info from the active device (if connected).
    if let Ok(Some(device)) = get_device_for_active_ssid(ssid).await {
        if let Ok(device_map) =
            nmcli_key_value_map(&["-t", "-f", "GENERAL,IP4,IP6,DHCP4", "device", "show", &device][..])
                .await
        {
            // If the saved profile name doesn't match the SSID, we can still enrich details
            // by following the active connection name from the device.
            let active_connection_name = device_map
                .get("GENERAL.CONNECTION")
                .cloned()
                .filter(|v| !v.is_empty() && v != "--");

            info.interface = device_map
                .get("GENERAL.DEVICE")
                .cloned()
                .or_else(|| info.interface.clone())
                .filter(|v| !v.is_empty() && v != "--");
            info.state = device_map
                .get("GENERAL.STATE")
                .cloned()
                .filter(|v| !v.is_empty() && v != "--");
            info.connection_type = info
                .connection_type
                .or_else(|| device_map.get("GENERAL.TYPE").cloned())
                .filter(|v| !v.is_empty() && v != "--");

            info.link_speed_mbps = device_map
                .get("GENERAL.SPEED")
                .and_then(|v| v.split_whitespace().next())
                .and_then(|v| v.parse::<u32>().ok())
                .filter(|v| *v > 0);

            // If we didn't get a MAC from the profile, use the device HWADDR.
            if info.mac_address.is_none() {
                info.mac_address = device_map
                    .get("GENERAL.HWADDR")
                    .cloned()
                    .filter(|v| !v.is_empty() && v != "--");
            }

            if let Some(ip4_raw) = device_map.get("IP4.ADDRESS[1]") {
                if let Some((ip, mask)) = parse_ipv4_cidr(ip4_raw) {
                    info.ip_address = Some(ip);
                    info.subnet_mask = Some(mask);
                } else {
                    info.ip_address = Some(ip4_raw.to_string());
                }
            }

            info.gateway = device_map
                .get("IP4.GATEWAY")
                .cloned()
                .filter(|v| !v.is_empty() && v != "--");

            info.dns = collect_indexed_values(&device_map, "IP4.DNS");

            info.ipv6_address = device_map
                .get("IP6.ADDRESS[1]")
                .and_then(|raw| raw.split('/').next())
                .map(|s| s.to_string())
                .filter(|v| !v.is_empty() && v != "--");

            info.dhcp_lease_time_seconds = parse_dhcp_lease_time_seconds(&device_map);

            // Enrich profile-only fields (UUID/type/BSSID list) from the active connection name.
            if let Some(conn_name) = active_connection_name.as_deref() {
                if info.uuid.is_none() || info.connection_type.is_none() || info.mac_address.is_none()
                {
                    if let Ok(connection_map) =
                        nmcli_key_value_map(&["-t", "connection", "show", conn_name][..]).await
                    {
                        info.connection_type = info
                            .connection_type
                            .or_else(|| connection_map.get("connection.type").cloned())
                            .filter(|v| !v.is_empty() && v != "--");
                        info.uuid = info
                            .uuid
                            .or_else(|| connection_map.get("connection.uuid").cloned())
                            .filter(|v| !v.is_empty() && v != "--");

                        if info.mac_address.is_none() {
                            info.mac_address = connection_map
                                .get("802-11-wireless.seen-bssids")
                                .cloned()
                                .or_else(|| connection_map.get("802-11-wireless.mac-address").cloned())
                                .filter(|v| !v.is_empty() && v != "--");
                        }
                    }
                }
            }
        }
    }

    Ok(info)
}

pub async fn get_active_wifi_ssid() -> Result<Option<String>> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "ACTIVE,SSID", "dev", "wifi"])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to get active Wi-Fi: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let mut parts = line.splitn(2, ':');
        let active = parts.next().unwrap_or_default();
        let ssid = parts.next().unwrap_or_default();
        if active == "yes" && !ssid.is_empty() {
            return Ok(Some(ssid.to_string()));
        }
    }

    Ok(None)
}

pub async fn get_active_wired_connection() -> Result<Option<String>> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "TYPE,STATE,CONNECTION", "dev", "status"])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to get active wired connection: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let mut parts = line.splitn(3, ':');
        let dev_type = parts.next().unwrap_or_default();
        let state = parts.next().unwrap_or_default();
        let connection = parts.next().unwrap_or_default();

        if dev_type == "ethernet" && state.starts_with("connected") {
            if connection.is_empty() || connection == "--" {
                return Ok(Some("Wired connection".to_string()));
            }
            return Ok(Some(connection.to_string()));
        }
    }

    Ok(None)
}

pub async fn is_network_saved(ssid: &str) -> Result<bool> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "NAME,TYPE", "connection", "show"])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        let parts: Vec<&str> = line.rsplitn(2, ':').collect();
        if parts.len() == 2 {
            let conn_type = parts[0];
            let name = parts[1];
            
            if conn_type == "802-11-wireless" && name == ssid {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

pub async fn get_autoconnect_for_ssid(ssid: &str) -> Result<bool> {
    let output = Command::new("nmcli")
        .args(["-t", "-g", "connection.autoconnect", "connection", "show", ssid])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to get autoconnect: {}", stderr.trim()));
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_lowercase();
    Ok(value == "yes" || value == "true")
}

pub async fn set_autoconnect_for_ssid(ssid: &str, enabled: bool) -> Result<()> {
    let value = if enabled { "yes" } else { "no" };
    let output = Command::new("nmcli")
        .args(["connection", "modify", ssid, "connection.autoconnect", value])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to set autoconnect: {}", stderr.trim()));
    }

    Ok(())
}

pub async fn connect_open_network(ssid: &str) -> Result<ConnectStatus> {
    match connect_open_network_once(ssid).await {
        Ok(status) => Ok(status),
        Err(err) => {
            if is_connection_interrupted_error(&err) {
                let _ = Command::new("nmcli")
                    .args(["device", "wifi", "rescan"])
                    .output()
                    .await;
                return connect_open_network_once(ssid)
                    .await
                    .map_err(|err| anyhow!("Failed to connect: {}", err));
            }
            if is_network_not_found_error(&err) {
                let _ = Command::new("nmcli")
                    .args(["device", "wifi", "rescan"])
                    .output()
                    .await;
                return connect_open_network_once(ssid)
                    .await
                    .map_err(|err| anyhow!("Failed to connect: {}", err));
            }
            Err(anyhow!("Failed to connect: {}", err))
        }
    }
}

pub async fn connect_secured_network(
    ssid: &str,
    password: &str,
    security_type: Option<&str>,
) -> Result<ConnectStatus> {
    match connect_secured_network_once(ssid, password, security_type).await {
        Ok(status) => Ok(status),
        Err(err) => {
            if is_key_mgmt_missing_error(&err) {
                return connect_secured_network_with_key_mgmt(ssid, password, security_type).await;
            }
            if is_connection_interrupted_error(&err) {
                let _ = Command::new("nmcli")
                    .args(["device", "wifi", "rescan"])
                    .output()
                    .await;
                return connect_secured_network_once(ssid, password, security_type)
                    .await
                    .map_err(|err| anyhow!("Failed to connect: {}", err));
            }
            if is_network_not_found_error(&err) {
                let _ = Command::new("nmcli")
                    .args(["device", "wifi", "rescan"])
                    .output()
                    .await;
                return connect_secured_network_once(ssid, password, security_type)
                    .await
                    .map_err(|err| anyhow!("Failed to connect: {}", err));
            }
            Err(anyhow!("Failed to connect: {}", err))
        }
    }
}

pub async fn activate_saved_connection(ssid: &str) -> Result<ConnectStatus> {
    let output = Command::new("nmcli")
        .args(["connection", "up", ssid])
        .output()
        .await?;

    if !output.status.success() {
        let err = nmcli_error_text(&output);
        if is_connection_interrupted_error(&err) {
            let _ = Command::new("nmcli")
                .args(["device", "wifi", "rescan"])
                .output()
                .await;
            let retry = Command::new("nmcli")
                .args(["connection", "up", ssid])
                .output()
                .await?;
            if retry.status.success() {
                return Ok(ConnectStatus::Connected);
            }
        }
        if is_activation_queued(&err) {
            return Ok(ConnectStatus::Queued);
        }
        return Err(anyhow!("Failed to activate connection: {}", err));
    }
    Ok(ConnectStatus::Connected)
}

async fn connect_open_network_once(ssid: &str) -> Result<ConnectStatus, String> {
    let output = Command::new("nmcli")
        .args(["dev", "wifi", "connect", ssid])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        return Ok(ConnectStatus::Connected);
    }
    let err = nmcli_error_text(&output);
    if is_activation_queued(&err) {
        return Ok(ConnectStatus::Queued);
    }
    Err(err)
}

async fn connect_secured_network_once(
    ssid: &str,
    password: &str,
    _security_type: Option<&str>,
) -> Result<ConnectStatus, String> {
    let output = Command::new("nmcli")
        .args(["device", "wifi", "connect", ssid, "password", password])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        return Ok(ConnectStatus::Connected);
    }
    let err = nmcli_error_text(&output);
    if is_activation_queued(&err) {
        return Ok(ConnectStatus::Queued);
    }
    Err(err)
}

fn nmcli_error_text(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn is_activation_queued(stderr: &str) -> bool {
    let err = stderr.to_lowercase();
    err.contains("activation was enqueued") || err.contains("enqueued")
}

pub fn is_network_not_found_error(message: &str) -> bool {
    let msg = message.to_lowercase();
    msg.contains("network could not be found")
        || msg.contains("no network with ssid")
        || (msg.contains("not found") && msg.contains("ssid"))
}

fn is_key_mgmt_missing_error(message: &str) -> bool {
    let msg = message.to_lowercase();
    msg.contains("key-mgmt") && msg.contains("missing")
}

fn is_connection_interrupted_error(message: &str) -> bool {
    let msg = message.to_lowercase();
    msg.contains("base network connection was interrupted")
        || msg.contains("connection was interrupted")
}

async fn connect_secured_network_with_key_mgmt(
    ssid: &str,
    password: &str,
    security_type: Option<&str>,
) -> Result<ConnectStatus> {
    let key_mgmt = key_mgmt_from_security_type(security_type);
    let device = get_wifi_device().await?;

    let add_result = Command::new("nmcli")
        .args([
            "connection",
            "add",
            "type",
            "wifi",
            "ifname",
            &device,
            "con-name",
            ssid,
            "ssid",
            ssid,
        ])
        .output()
        .await?;

    if !add_result.status.success() {
        let stderr = String::from_utf8_lossy(&add_result.stderr);
        if !stderr.to_lowercase().contains("already exists") {
            log::warn!("Failed to add connection: {}", stderr.trim());
        }
    }

    let modify_result = if key_mgmt == "none" {
        Command::new("nmcli")
            .args([
                "connection",
                "modify",
                ssid,
                "wifi-sec.key-mgmt",
                "none",
                "wifi-sec.wep-key0",
                password,
            ])
            .output()
            .await?
    } else {
        Command::new("nmcli")
            .args([
                "connection",
                "modify",
                ssid,
                "wifi-sec.key-mgmt",
                key_mgmt,
                "wifi-sec.psk",
                password,
            ])
            .output()
            .await?
    };

    if !modify_result.status.success() {
        let err = nmcli_error_text(&modify_result);
        return Err(anyhow!("Failed to set security: {}", err));
    }

    activate_saved_connection(ssid).await
}

async fn get_wifi_device() -> Result<String> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "DEVICE,TYPE,STATE", "device"])
        .output()
        .await?;

    if !output.status.success() {
        let err = nmcli_error_text(&output);
        return Err(anyhow!("Failed to list devices: {}", err));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() < 3 {
            continue;
        }
        let device = parts[0];
        let dev_type = parts[1];
        let state = parts[2];
        if dev_type == "wifi" && state != "unavailable" {
            return Ok(device.to_string());
        }
    }

    Err(anyhow!("No available Wi-Fi device found"))
}

fn key_mgmt_from_security_type(security_type: Option<&str>) -> &'static str {
    let Some(sec) = security_type else {
        return "wpa-psk";
    };
    let sec = sec.to_lowercase();
    if sec.contains("wpa3") {
        "sae"
    } else if sec.contains("wep") {
        "none"
    } else {
        "wpa-psk"
    }
}


pub async fn disconnect_network(ssid: &str) -> Result<()> {
    let output = Command::new("nmcli")
        .args(["connection", "down", ssid])
        .output()
        .await?;

    if output.status.success() {
        return Ok(());
    }

    if let Some(device) = get_device_for_active_ssid(ssid).await? {
        let dev_output = Command::new("nmcli")
            .args(["device", "disconnect", &device])
            .output()
            .await?;

        if dev_output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&dev_output.stderr);
        return Err(anyhow!("Failed to disconnect device {}: {}", device, stderr));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(anyhow!("Failed to disconnect: {}", stderr))
}

async fn get_device_for_active_ssid(ssid: &str) -> Result<Option<String>> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "SSID,DEVICE,ACTIVE", "dev", "wifi", "list"])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.rsplitn(3, ':').collect();
        if parts.len() == 3 {
            let active = parts[0];
            let device = parts[1];
            let net_ssid = parts[2];
            if net_ssid == ssid && active == "yes" {
                return Ok(Some(device.to_string()));
            }
        }
    }
    Ok(None)
}

async fn nmcli_key_value_map(args: &[&str]) -> Result<HashMap<String, String>> {
    let output = Command::new("nmcli").args(args).output().await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("nmcli failed: {}", stderr.trim()));
    }

    Ok(parse_key_value_output(&output.stdout))
}

fn parse_key_value_output(stdout: &[u8]) -> HashMap<String, String> {
    let text = String::from_utf8_lossy(stdout);
    let mut map = HashMap::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        map.insert(key.trim().to_string(), value.trim().to_string());
    }

    map
}

fn parse_ipv4_cidr(cidr: &str) -> Option<(String, String)> {
    let (ip, prefix_str) = cidr.trim().split_once('/')?;
    let prefix: u8 = prefix_str.parse().ok()?;
    if prefix > 32 {
        return None;
    }
    let mask: u32 = if prefix == 0 {
        0
    } else {
        (!0u32) << (32 - prefix)
    };
    let mask_str = format!(
        "{}.{}.{}.{}",
        (mask >> 24) & 0xff,
        (mask >> 16) & 0xff,
        (mask >> 8) & 0xff,
        mask & 0xff
    );
    Some((ip.to_string(), mask_str))
}

fn collect_indexed_values(map: &HashMap<String, String>, prefix: &str) -> Vec<String> {
    let mut items: Vec<(u32, String)> = map
        .iter()
        .filter_map(|(k, v)| {
            if !k.starts_with(prefix) {
                return None;
            }
            let idx = k
                .split_once('[')
                .and_then(|(_, rest)| rest.split_once(']'))
                .and_then(|(num, _)| num.parse::<u32>().ok())
                .unwrap_or(0);
            let value = v.trim();
            if value.is_empty() || value == "--" {
                return None;
            }
            Some((idx, value.to_string()))
        })
        .collect();

    items.sort_by_key(|(idx, _)| *idx);
    items.into_iter().map(|(_, v)| v).collect()
}

fn parse_dhcp_lease_time_seconds(map: &HashMap<String, String>) -> Option<u32> {
    for (key, value) in map {
        if !key.starts_with("DHCP4.OPTION") {
            continue;
        }

        let v = value.trim();
        let (left, right) = v.split_once('=').unwrap_or((v, ""));
        let left = left.trim();
        let right = right.trim();

        if left != "dhcp_lease_time" {
            continue;
        }

        if let Ok(seconds) = right.parse::<u32>() {
            return Some(seconds);
        }
    }
    None
}

#[allow(dead_code)]
pub async fn get_saved_connections() -> Result<Vec<SavedConnection>> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "NAME,UUID,TYPE", "connection", "show"])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut connections = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.rsplitn(3, ':').collect();
        if parts.len() < 3 {
            continue;
        }

        let conn_type = parts[0];
        let uuid = parts[1];
        let name = parts[2];

        if conn_type != "802-11-wireless" {
            continue;
        }

        if name == "Hotspot" {
            continue;
        }

        connections.push(SavedConnection {
            ssid: name.to_string(),
            uuid: uuid.to_string(),
        });
    }

    Ok(connections)
}

#[allow(dead_code)]
pub async fn get_saved_password(uuid: &str) -> Result<String> {
    let output = Command::new("nmcli")
        .args(["--show-secrets", "-g", "802-11-wireless-security.psk", "connection", "show", uuid])
        .output()
        .await?;

    if !output.status.success() {
        return Err(anyhow!("Failed to get password"));
    }

    let password = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(password)
}

pub async fn get_saved_password_for_ssid(ssid: &str) -> Result<String> {
    let output = Command::new("nmcli")
        .args([
            "--show-secrets",
            "-g",
            "802-11-wireless-security.psk",
            "connection",
            "show",
            ssid,
        ])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to get password: {}", stderr.trim()));
    }

    let password = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if password.is_empty() {
        return Err(anyhow!("Empty password returned"));
    }

    Ok(password)
}

pub async fn delete_connection_by_ssid(ssid: &str) -> Result<()> {
    let output = Command::new("nmcli")
        .args(["connection", "delete", ssid])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to delete connection: {}", stderr.trim()));
    }

    Ok(())
}

#[allow(dead_code)]
pub async fn delete_connection(uuid: &str) -> Result<()> {
    let output = Command::new("nmcli")
        .args(["connection", "delete", uuid])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to delete connection: {}", stderr));
    }
    Ok(())
}
