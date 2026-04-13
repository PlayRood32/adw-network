use anyhow::{anyhow, Result};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;
use tokio::process::Command;
use tokio::time::{sleep, Duration};
use uuid::Uuid;
use zvariant::{OwnedValue, Str};

use crate::nm_dbus::{
    DbusAccessPoint, DbusActiveConnection, DbusConnectionProfile, NmDbusClient, SettingsMap,
    NM_ACTIVE_CONNECTION_STATE_ACTIVATED, NM_DEVICE_TYPE_ETHERNET, NM_DEVICE_TYPE_WIFI,
};

pub const NMCLI_RETRIEVAL_TOAST: &str =
    "Unable to retrieve data from NetworkManager – check your connection";

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceType {
    Ethernet,
    Wifi,
    Loopback,
    Other(String),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Device {
    pub name: String,
    pub device_type: DeviceType,
    pub state: String,
    pub connection: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Connection {
    pub name: String,
    pub uuid: String,
    pub conn_type: String,
    pub device: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VpnKind {
    WireGuard,
    OpenVpn,
}

impl VpnKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::WireGuard => "WireGuard",
            Self::OpenVpn => "OpenVPN",
        }
    }
}

#[derive(Debug, Clone)]
pub struct VpnConnection {
    pub name: String,
    pub uuid: String,
    pub kind: VpnKind,
    pub active: bool,
}

#[derive(Debug, Clone, Default)]
pub struct WireGuardConnectionConfig {
    pub name: String,
    pub interface_name: String,
    pub addresses: Vec<String>,
    pub dns_servers: Vec<String>,
    pub private_key: String,
    pub public_key: String,
    pub preshared_key: Option<String>,
    pub endpoint: String,
    pub allowed_ips: Vec<String>,
    pub mtu: Option<u32>,
    pub persistent_keepalive: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct OpenVpnConnectionConfig {
    pub name: String,
    pub remote: String,
    pub username: Option<String>,
}

pub struct NetworkManager;

pub fn is_nmcli_retrieval_error(message: &str) -> bool {
    message.contains(NMCLI_RETRIEVAL_TOAST)
}

pub async fn dbus_client() -> Result<NmDbusClient> {
    NmDbusClient::new()
        .await
        .map_err(|e| anyhow!("{} [{}]", NMCLI_RETRIEVAL_TOAST, e))
}

impl NetworkManager {
    pub async fn get_devices() -> Result<Vec<Device>> {
        let client = dbus_client().await?;
        let active = client.list_active_connections().await?;
        let devices = client.list_devices().await?;

        let mut active_by_path: HashMap<String, String> = HashMap::new();
        for conn in active {
            for dev in conn.devices {
                active_by_path.insert(dev.to_string(), conn.id.clone());
            }
        }

        let out = devices
            .into_iter()
            .map(|device| {
                let device_type = match device.device_type {
                    NM_DEVICE_TYPE_ETHERNET => DeviceType::Ethernet,
                    NM_DEVICE_TYPE_WIFI => DeviceType::Wifi,
                    14 => DeviceType::Loopback,
                    other => DeviceType::Other(other.to_string()),
                };

                Device {
                    name: device.interface,
                    device_type,
                    state: nm_device_state_label(device.state).to_string(),
                    connection: active_by_path.get(&device.path.to_string()).cloned(),
                }
            })
            .collect();

        Ok(out)
    }

    pub async fn get_connections() -> Result<Vec<Connection>> {
        let client = dbus_client().await?;
        let profiles = client.list_connections().await?;
        let active = client.list_active_connections().await?;

        let mut active_map: HashMap<String, (String, bool)> = HashMap::new();
        for conn in active {
            let device = conn
                .devices
                .first()
                .map(|p| p.to_string())
                .filter(|v| !v.is_empty() && v != "/");
            active_map.insert(conn.uuid.clone(), (device.unwrap_or_default(), true));
        }

        let mut out = Vec::new();
        for p in profiles {
            let (device, active) = active_map
                .get(&p.uuid)
                .map(|(device, active)| {
                    (
                        if device.is_empty() {
                            None
                        } else {
                            Some(device.clone())
                        },
                        *active,
                    )
                })
                .unwrap_or((None, false));

            out.push(Connection {
                name: p.id,
                uuid: p.uuid,
                conn_type: p.conn_type,
                device,
                active,
            });
        }

        Ok(out)
    }
}

impl Connection {
    pub fn is_ethernet(&self) -> bool {
        self.conn_type == "802-3-ethernet" || self.conn_type == "ethernet"
    }

    pub async fn activate(&self) -> Result<ConnectStatus> {
        let client = dbus_client().await?;
        client.activate_connection_by_id(&self.name, None).await?;
        Ok(ConnectStatus::Connected)
    }

    pub async fn deactivate(&self) -> Result<()> {
        let client = dbus_client().await?;
        client.deactivate_connection_by_id(&self.name).await
    }

    pub async fn autoconnect(&self) -> Result<bool> {
        get_autoconnect_for_connection(&self.name).await
    }

    pub async fn set_autoconnect(&self, enabled: bool) -> Result<()> {
        set_autoconnect_for_connection(&self.name, enabled).await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectStatus {
    Connected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternetConnectivity {
    Unknown,
    NoInternet,
    Portal,
    Limited,
    Full,
}

impl InternetConnectivity {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Unknown => "Checking internet",
            Self::NoInternet => "No internet",
            Self::Portal => "Login required",
            Self::Limited => "Limited internet",
            Self::Full => "Online",
        }
    }
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
    dbus_client().await?.is_wifi_enabled().await
}

pub async fn set_wifi_enabled(enabled: bool) -> Result<()> {
    dbus_client().await?.set_wifi_enabled(enabled).await
}

pub async fn is_ethernet_enabled() -> Result<bool> {
    dbus_client().await?.is_ethernet_enabled().await
}

pub async fn set_ethernet_enabled(enabled: bool) -> Result<()> {
    dbus_client().await?.set_ethernet_enabled(enabled).await
}

pub async fn has_wifi_device() -> Result<bool> {
    if !is_wifi_present().await? {
        return Ok(false);
    }

    let devices = NetworkManager::get_devices().await?;
    Ok(devices
        .into_iter()
        .any(|d| d.device_type == DeviceType::Wifi && d.state != "unmanaged"))
}

pub async fn is_wifi_present() -> Result<bool> {
    dbus_client().await?.is_wifi_present().await
}

pub async fn has_ethernet_device() -> Result<bool> {
    let devices = NetworkManager::get_devices().await?;
    Ok(devices
        .into_iter()
        .any(|d| d.device_type == DeviceType::Ethernet))
}

pub async fn scan_networks() -> Result<Vec<WifiNetwork>> {
    let client = dbus_client().await?;

    // * Merge cached access points with a fresh scan because NM scan completion is asynchronous.
    let mut aps = client.list_access_points().await.unwrap_or_default();
    let _ = client.request_wifi_scan().await;
    sleep(Duration::from_millis(1200)).await;
    if let Ok(scanned) = client.list_access_points().await {
        aps.extend(scanned);
    }

    let mut networks_by_key: HashMap<(String, String, String), WifiNetwork> = HashMap::new();

    for ap in aps {
        let normalized_freq = normalize_frequency_mhz(ap.frequency);
        let band = band_from_frequency(normalized_freq);
        let network = WifiNetwork {
            ssid: ap.ssid.clone(),
            signal: ap.strength,
            secured: is_ap_secured(&ap),
            connected: ap.active,
            band: band.to_string(),
            channel: channel_from_frequency(normalized_freq),
            freq_mhz: normalized_freq,
            security_type: ap_security_type(&ap),
        };

        // * Keep distinct entries for SSID + band + security because one SSID may expose variants.
        let key = (
            network.ssid.clone(),
            network.band.clone(),
            network.security_type.clone(),
        );
        match networks_by_key.get_mut(&key) {
            None => {
                networks_by_key.insert(key, network);
            }
            Some(existing) => {
                if (network.connected && !existing.connected)
                    || (network.connected == existing.connected && network.signal > existing.signal)
                {
                    *existing = network;
                }
            }
        }
    }

    let mut networks: Vec<WifiNetwork> = networks_by_key.into_values().collect();
    networks.sort_by(compare_wifi_networks);

    Ok(networks)
}

pub async fn get_network_info(ssid: &str) -> Result<NetworkInfo> {
    let client = dbus_client().await?;

    let (profile, active, device, ip4_info) = client.get_network_info_by_id(ssid).await?;

    let mut info = NetworkInfo::default();

    if let Some(p) = profile.as_ref() {
        info.connection_type = Some(p.conn_type.clone());
        info.uuid = Some(p.uuid.clone());
        info.interface = p.interface_name.clone();

        if let Some(wifi) = p.settings.get("802-11-wireless") {
            if let Some(value) = wifi.get("mac-address") {
                info.mac_address = value
                    .try_clone()
                    .ok()
                    .and_then(|v| String::try_from(v).ok());
            }
            if info.mac_address.is_none() {
                if let Some(value) = wifi.get("seen-bssids") {
                    info.mac_address = value
                        .try_clone()
                        .ok()
                        .and_then(|v| String::try_from(v).ok());
                }
            }
        }
    }

    if let Some(active) = active.as_ref() {
        info.state = Some(active_connection_state_label(active.state).to_string());
        if info.connection_type.is_none() {
            info.connection_type = Some(active.conn_type.clone());
        }
    }

    if let Some(device) = device.as_ref() {
        info.interface = Some(device.interface.clone());
        if info.mac_address.is_none() {
            info.mac_address = Some(device.interface.clone());
        }
    }

    if let Some(ip) = ip4_info.addresses.first() {
        info.ip_address = Some(ip.clone());
    }
    info.gateway = ip4_info.gateway;
    info.dns = ip4_info.dns;
    info.dhcp_lease_time_seconds = ip4_info.dhcp_lease_time_seconds;

    Ok(info)
}

pub async fn get_active_wifi_ssid() -> Result<Option<String>> {
    dbus_client().await?.get_active_wifi_ssid().await
}

pub async fn get_active_wired_connection() -> Result<Option<String>> {
    dbus_client().await?.get_active_wired_connection().await
}

fn vpn_kind_for_profile(profile: &DbusConnectionProfile) -> Option<VpnKind> {
    if profile.conn_type == "wireguard" {
        return Some(VpnKind::WireGuard);
    }

    if profile.conn_type == "vpn" {
        let service_type = profile
            .settings
            .get("vpn")
            .and_then(|vpn| vpn.get("service-type"))
            .and_then(value_string);
        if matches!(
            service_type.as_deref(),
            Some("org.freedesktop.NetworkManager.openvpn")
        ) {
            return Some(VpnKind::OpenVpn);
        }
    }

    None
}

pub async fn list_supported_vpn_connections() -> Result<Vec<VpnConnection>> {
    let client = dbus_client().await?;
    let profiles = client.list_connections().await?;
    let active = client.list_active_connections().await?;

    let mut active_map = HashMap::new();
    for connection in active {
        active_map.insert(connection.uuid.clone(), connection);
    }

    let mut out = Vec::new();
    for profile in profiles {
        let Some(kind) = vpn_kind_for_profile(&profile) else {
            continue;
        };
        let active = active_map
            .get(&profile.uuid)
            .map(|connection| connection.state == NM_ACTIVE_CONNECTION_STATE_ACTIVATED)
            .unwrap_or(false);
        out.push(VpnConnection {
            name: profile.id,
            uuid: profile.uuid,
            kind,
            active,
        });
    }

    out.sort_by(|a, b| {
        if a.active && !b.active {
            Ordering::Less
        } else if !a.active && b.active {
            Ordering::Greater
        } else {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        }
    });

    Ok(out)
}

pub async fn activate_vpn_connection(uuid: &str) -> Result<()> {
    dbus_client()
        .await?
        .activate_connection_by_uuid(uuid, None)
        .await?;
    Ok(())
}

pub async fn deactivate_vpn_connection(uuid: &str) -> Result<()> {
    dbus_client()
        .await?
        .deactivate_connection_by_uuid(uuid)
        .await
}

pub async fn delete_vpn_connection(uuid: &str) -> Result<()> {
    dbus_client().await?.delete_connection_by_uuid(uuid).await
}

pub async fn get_wireguard_connection_config(uuid: &str) -> Result<WireGuardConnectionConfig> {
    let client = dbus_client().await?;
    let profile = client
        .find_connection_by_uuid(uuid)
        .await?
        .ok_or_else(|| anyhow!("VPN connection {} not found", uuid))?;
    let kind = vpn_kind_for_profile(&profile)
        .ok_or_else(|| anyhow!("Connection {} is not a supported VPN", uuid))?;
    if kind != VpnKind::WireGuard {
        return Err(anyhow!("Connection {} is not a WireGuard VPN", uuid));
    }

    let addresses = collect_address_strings(&profile.settings);
    let dns_servers = collect_dns_strings(&profile.settings);
    let interface_name = profile
        .interface_name
        .clone()
        .unwrap_or_else(|| "wg0".to_string());

    let mut config = WireGuardConnectionConfig {
        name: profile.id.clone(),
        interface_name,
        addresses,
        dns_servers,
        private_key: profile
            .settings
            .get("wireguard")
            .and_then(|section| section.get("private-key"))
            .and_then(value_string)
            .unwrap_or_default(),
        mtu: profile
            .settings
            .get("wireguard")
            .and_then(|section| section.get("mtu"))
            .and_then(value_u32),
        ..WireGuardConnectionConfig::default()
    };

    for (section_name, section) in &profile.settings {
        let Some(public_key) = section_name.strip_prefix("wireguard-peer.") else {
            continue;
        };
        config.public_key = public_key.to_string();
        config.endpoint = section
            .get("endpoint")
            .and_then(value_string)
            .unwrap_or_default();
        config.preshared_key = section.get("preshared-key").and_then(value_string);
        config.persistent_keepalive = section.get("persistent-keepalive").and_then(value_u32);
        config.allowed_ips = section
            .get("allowed-ips")
            .and_then(value_string_list)
            .unwrap_or_else(|| vec!["0.0.0.0/0".to_string(), "::/0".to_string()]);
        break;
    }

    Ok(config)
}

pub async fn get_openvpn_connection_config(uuid: &str) -> Result<OpenVpnConnectionConfig> {
    let client = dbus_client().await?;
    let profile = client
        .find_connection_by_uuid(uuid)
        .await?
        .ok_or_else(|| anyhow!("VPN connection {} not found", uuid))?;
    let kind = vpn_kind_for_profile(&profile)
        .ok_or_else(|| anyhow!("Connection {} is not a supported VPN", uuid))?;
    if kind != VpnKind::OpenVpn {
        return Err(anyhow!("Connection {} is not an OpenVPN connection", uuid));
    }

    let vpn_section = profile
        .settings
        .get("vpn")
        .ok_or_else(|| anyhow!("OpenVPN settings are missing"))?;
    let vpn_data = vpn_section
        .get("data")
        .and_then(value_string_map)
        .unwrap_or_default();

    Ok(OpenVpnConnectionConfig {
        name: profile.id,
        remote: vpn_data.get("remote").cloned().unwrap_or_default(),
        username: vpn_section
            .get("user-name")
            .and_then(value_string)
            .or_else(|| vpn_data.get("username").cloned()),
    })
}

pub async fn create_openvpn_connection(config: &OpenVpnConnectionConfig) -> Result<()> {
    let client = dbus_client().await?;
    client
        .add_connection(&build_openvpn_settings(config)?)
        .await?;
    Ok(())
}

pub async fn update_openvpn_connection(uuid: &str, config: &OpenVpnConnectionConfig) -> Result<()> {
    let client = dbus_client().await?;
    let profile = client
        .find_connection_by_uuid(uuid)
        .await?
        .ok_or_else(|| anyhow!("VPN connection {} not found", uuid))?;
    let mut settings = clone_settings_map(&profile.settings)?;

    let connection = settings.entry("connection".to_string()).or_default();
    connection.insert("id".to_string(), owned_string(&config.name));
    connection.insert("type".to_string(), owned_string("vpn"));
    connection.insert(
        "uuid".to_string(),
        profile
            .settings
            .get("connection")
            .and_then(|section| section.get("uuid"))
            .and_then(|value| value.try_clone().ok())
            .unwrap_or_else(|| owned_string(&profile.uuid)),
    );

    let vpn = settings.entry("vpn".to_string()).or_default();
    vpn.insert(
        "service-type".to_string(),
        owned_string("org.freedesktop.NetworkManager.openvpn"),
    );
    if let Some(username) = config
        .username
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        vpn.insert("user-name".to_string(), owned_string(username.trim()));
    } else {
        vpn.remove("user-name");
    }
    vpn.insert(
        "data".to_string(),
        owned_string_map(HashMap::from([
            ("connection-type".to_string(), "password".to_string()),
            ("remote".to_string(), config.remote.trim().to_string()),
        ]))?,
    );

    let ipv4 = settings.entry("ipv4".to_string()).or_default();
    ipv4.insert("method".to_string(), owned_string("auto"));
    let ipv6 = settings.entry("ipv6".to_string()).or_default();
    ipv6.insert("method".to_string(), owned_string("ignore"));

    client
        .update_connection_settings(&profile.path, &settings)
        .await
}

pub async fn create_wireguard_connection(config: &WireGuardConnectionConfig) -> Result<String> {
    import_wireguard_config(None, config, false).await
}

pub async fn update_wireguard_connection(
    uuid: &str,
    config: &WireGuardConnectionConfig,
) -> Result<String> {
    import_wireguard_config(Some(uuid), config, true).await
}

pub async fn import_vpn_connection(path: &Path) -> Result<()> {
    let kind = detect_vpn_file_type(path).await?;
    let vpn_type = match kind {
        VpnKind::WireGuard => "wireguard",
        VpnKind::OpenVpn => "openvpn",
    };
    run_nmcli_command(&[
        "connection",
        "import",
        "type",
        vpn_type,
        "file",
        path.to_string_lossy().as_ref(),
    ])
    .await
}

pub async fn rename_connection_uuid(uuid: &str, name: &str) -> Result<()> {
    let client = dbus_client().await?;
    let profile = client
        .find_connection_by_uuid(uuid)
        .await?
        .ok_or_else(|| anyhow!("Connection {} not found", uuid))?;
    let mut settings = clone_settings_map(&profile.settings)?;
    let connection = settings.entry("connection".to_string()).or_default();
    connection.insert("id".to_string(), owned_string(name));
    client
        .update_connection_settings(&profile.path, &settings)
        .await
}

pub async fn get_active_connection_name() -> Result<Option<String>> {
    dbus_client().await?.get_active_connection_name().await
}

pub async fn get_primary_connected_device() -> Result<Option<String>> {
    dbus_client().await?.get_primary_connected_device().await
}

pub async fn get_internet_connectivity() -> Result<InternetConnectivity> {
    let state = dbus_client().await?.get_connectivity_state().await?;
    Ok(match state {
        1 => InternetConnectivity::NoInternet,
        2 => InternetConnectivity::Portal,
        3 => InternetConnectivity::Limited,
        4 => InternetConnectivity::Full,
        _ => InternetConnectivity::Unknown,
    })
}

pub async fn set_custom_ipv4_dns_for_connection(
    connection: &str,
    dns_servers: &[String],
    search_domains: &[String],
) -> Result<()> {
    dbus_client()
        .await?
        .set_custom_ipv4_dns_for_connection(connection, dns_servers, search_domains)
        .await
}

pub async fn reapply_connection(connection: &str) -> Result<()> {
    dbus_client().await?.reapply_connection(connection).await
}

pub async fn is_network_saved(ssid: &str) -> Result<bool> {
    let client = dbus_client().await?;
    let conn = client.find_connection_by_id(ssid).await?;
    Ok(conn
        .map(|c| c.conn_type == "802-11-wireless")
        .unwrap_or(false))
}

pub async fn get_autoconnect_for_ssid(ssid: &str) -> Result<bool> {
    dbus_client()
        .await?
        .get_connection_autoconnect_by_id(ssid)
        .await
}

pub async fn get_autoconnect_for_connection(name: &str) -> Result<bool> {
    dbus_client()
        .await?
        .get_connection_autoconnect_by_id(name)
        .await
}

pub async fn set_autoconnect_for_ssid(ssid: &str, enabled: bool) -> Result<()> {
    dbus_client()
        .await?
        .set_connection_autoconnect_by_id(ssid, enabled)
        .await
}

pub async fn set_autoconnect_for_connection(name: &str, enabled: bool) -> Result<()> {
    dbus_client()
        .await?
        .set_connection_autoconnect_by_id(name, enabled)
        .await
}

pub async fn set_autoconnect_for_connection_uuid(uuid: &str, enabled: bool) -> Result<()> {
    dbus_client()
        .await?
        .set_connection_autoconnect_by_uuid(uuid, enabled)
        .await
}

pub async fn set_connection_zone_for_connection_uuid(uuid: &str, zone: &str) -> Result<()> {
    dbus_client()
        .await?
        .set_connection_zone_by_uuid(uuid, zone)
        .await
}

pub async fn connect_open_network(ssid: &str) -> Result<ConnectStatus> {
    connect_wifi_network(ssid, None, None, false).await
}

pub async fn connect_secured_network(
    ssid: &str,
    password: &str,
    security_type: Option<&str>,
) -> Result<ConnectStatus> {
    connect_wifi_network(ssid, Some(password), security_type, false).await
}

pub async fn connect_hidden_network(
    ssid: &str,
    password: Option<&str>,
    security_type: Option<&str>,
) -> Result<ConnectStatus> {
    connect_wifi_network(ssid, password, security_type, true).await
}

pub async fn activate_saved_connection(ssid: &str) -> Result<ConnectStatus> {
    dbus_client()
        .await?
        .activate_connection_by_id(ssid, None)
        .await?;
    Ok(ConnectStatus::Connected)
}

pub fn is_network_not_found_error(message: &str) -> bool {
    let msg = message.to_lowercase();
    msg.contains("not found") || msg.contains("unknown connection")
}

pub async fn disconnect_network(ssid: &str) -> Result<()> {
    dbus_client().await?.disconnect_connection_by_id(ssid).await
}

async fn map_saved_connections(profiles: Vec<DbusConnectionProfile>) -> Vec<SavedConnection> {
    profiles
        .into_iter()
        .filter(|c| c.conn_type == "802-11-wireless" && c.id != "Hotspot")
        .map(|c| SavedConnection {
            uuid: c.uuid,
            ssid: c.id,
        })
        .collect()
}

#[allow(dead_code)]
pub async fn get_saved_connections() -> Result<Vec<SavedConnection>> {
    let profiles = dbus_client().await?.list_connections().await?;
    Ok(map_saved_connections(profiles).await)
}

pub async fn delete_connection_by_ssid(ssid: &str) -> Result<()> {
    dbus_client().await?.delete_connection_by_id(ssid).await
}

#[allow(dead_code)]
pub async fn delete_connection(uuid: &str) -> Result<()> {
    dbus_client().await?.delete_connection_by_uuid(uuid).await
}

pub fn is_vpn_plugin_missing_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("vpn plugin")
        || lower.contains("service-type")
        || lower.contains("openvpn")
            && (lower.contains("not installed") || lower.contains("not supported"))
}

fn nm_device_state_label(state: u32) -> &'static str {
    match state {
        10 => "unmanaged",
        20 => "unavailable",
        30 => "disconnected",
        40 => "prepare",
        50 => "config",
        60 => "need-auth",
        70 => "ip-config",
        80 => "ip-check",
        90 => "secondaries",
        100 => "activated",
        110 => "deactivating",
        120 => "failed",
        _ => "unknown",
    }
}

fn active_connection_state_label(state: u32) -> &'static str {
    match state {
        0 => "unknown",
        1 => "activating",
        2 => "activated",
        3 => "deactivating",
        4 => "deactivated",
        _ => "unknown",
    }
}

fn band_from_frequency(freq: u32) -> &'static str {
    if (2400..=2500).contains(&freq) {
        "2.4 GHz"
    } else if (4900..=5900).contains(&freq) {
        "5 GHz"
    } else if (5925..=7125).contains(&freq) {
        "6 GHz"
    } else {
        "Unknown"
    }
}

fn channel_from_frequency(freq: u32) -> u32 {
    if (2412..=2472).contains(&freq) {
        ((freq - 2407) / 5).max(1)
    } else if freq == 2484 {
        14
    } else if (5000..=5900).contains(&freq) {
        (freq - 5000) / 5
    } else if (5955..=7115).contains(&freq) {
        (freq - 5950) / 5
    } else {
        0
    }
}

fn normalize_frequency_mhz(freq: u32) -> u32 {
    if freq >= 1_000_000_000 {
        // Some drivers may report Hz instead of MHz.
        freq / 1_000_000
    } else if freq >= 1_000_000 {
        // Some drivers may report kHz instead of MHz.
        freq / 1_000
    } else {
        freq
    }
}

fn is_ap_secured(ap: &DbusAccessPoint) -> bool {
    ap.flags != 0 || ap.wpa_flags != 0 || ap.rsn_flags != 0
}

fn ap_security_type(ap: &DbusAccessPoint) -> String {
    if ap.rsn_flags != 0 {
        if ap.rsn_flags & 0x0000_0400 != 0 {
            return "WPA3".to_string();
        }
        return "WPA2".to_string();
    }

    if ap.wpa_flags != 0 {
        return "WPA".to_string();
    }

    if ap.flags != 0 {
        return "WEP".to_string();
    }

    "Open".to_string()
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

async fn connect_wifi_network(
    ssid: &str,
    password: Option<&str>,
    security_type: Option<&str>,
    hidden: bool,
) -> Result<ConnectStatus> {
    let key_mgmt = password.map(|_| key_mgmt_from_security_type(security_type));

    dbus_client()
        .await?
        .add_and_activate_wifi_connection(ssid, password, key_mgmt, hidden)
        .await?;

    Ok(ConnectStatus::Connected)
}

fn compare_wifi_networks(a: &WifiNetwork, b: &WifiNetwork) -> Ordering {
    match (a.connected, b.connected) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }

    let ssid_cmp = a.ssid.to_lowercase().cmp(&b.ssid.to_lowercase());
    if ssid_cmp != Ordering::Equal {
        return ssid_cmp;
    }

    let band_cmp = wifi_band_sort_key(&a.band).cmp(&wifi_band_sort_key(&b.band));
    if band_cmp != Ordering::Equal {
        return band_cmp;
    }

    let security_cmp =
        wifi_security_sort_key(&a.security_type).cmp(&wifi_security_sort_key(&b.security_type));
    if security_cmp != Ordering::Equal {
        return security_cmp;
    }

    b.signal
        .cmp(&a.signal)
        .then_with(|| a.channel.cmp(&b.channel))
        .then_with(|| a.band.cmp(&b.band))
        .then_with(|| a.security_type.cmp(&b.security_type))
}

fn wifi_band_sort_key(band: &str) -> u8 {
    let normalized = band.to_lowercase();
    if normalized.contains("2.4") {
        0
    } else if normalized.contains("5") && !normalized.contains("2.4") && !normalized.contains("6") {
        1
    } else if normalized.contains('6') {
        2
    } else {
        3
    }
}

fn wifi_security_sort_key(security: &str) -> u8 {
    let normalized = security.to_lowercase();
    if normalized.contains("open") {
        0
    } else if normalized.contains("wep") {
        1
    } else if normalized.contains("wpa") {
        2
    } else {
        3
    }
}

pub async fn get_hotspot_ip() -> Result<Option<String>> {
    dbus_client().await?.get_hotspot_ip().await
}

fn owned_value_to_string(value: &zvariant::OwnedValue) -> Option<String> {
    value
        .try_clone()
        .ok()
        .and_then(|v| String::try_from(v).ok())
        .or_else(|| {
            Vec::<u8>::try_from(value.try_clone().ok()?)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
        })
}

fn owned_string(value: &str) -> OwnedValue {
    OwnedValue::from(Str::from(value))
}

fn owned_string_map(map: HashMap<String, String>) -> Result<OwnedValue> {
    Ok(OwnedValue::from(map))
}

fn value_string(value: &OwnedValue) -> Option<String> {
    value
        .try_clone()
        .ok()
        .and_then(|owned| String::try_from(owned).ok())
        .or_else(|| {
            Vec::<u8>::try_from(value.try_clone().ok()?)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
        })
}

fn value_u32(value: &OwnedValue) -> Option<u32> {
    u32::try_from(value).ok()
}

fn value_string_list(value: &OwnedValue) -> Option<Vec<String>> {
    Vec::<String>::try_from(value.try_clone().ok()?).ok()
}

fn value_string_map(value: &OwnedValue) -> Option<HashMap<String, String>> {
    HashMap::<String, String>::try_from(value.try_clone().ok()?).ok()
}

fn clone_settings_map(settings: &SettingsMap) -> Result<SettingsMap> {
    let mut cloned = SettingsMap::new();
    for (section_name, section) in settings {
        let mut cloned_section = HashMap::new();
        for (key, value) in section {
            cloned_section.insert(key.clone(), value.try_clone()?);
        }
        cloned.insert(section_name.clone(), cloned_section);
    }
    Ok(cloned)
}

fn collect_address_strings(settings: &SettingsMap) -> Vec<String> {
    let mut out = Vec::new();
    for section_name in ["ipv4", "ipv6"] {
        let Some(section) = settings.get(section_name) else {
            continue;
        };
        let Some(value) = section.get("address-data") else {
            continue;
        };
        let Ok(data) = Vec::<HashMap<String, OwnedValue>>::try_from(
            value.try_clone().ok().unwrap_or_else(|| owned_string("")),
        ) else {
            continue;
        };
        for item in data {
            let Some(address) = item.get("address").and_then(value_string) else {
                continue;
            };
            let prefix = item.get("prefix").and_then(value_u32);
            if let Some(prefix) = prefix {
                out.push(format!("{address}/{prefix}"));
            } else {
                out.push(address);
            }
        }
    }
    out
}

fn collect_dns_strings(settings: &SettingsMap) -> Vec<String> {
    let mut out = Vec::new();
    for section_name in ["ipv4", "ipv6"] {
        let Some(section) = settings.get(section_name) else {
            continue;
        };
        let Some(value) = section.get("dns-data") else {
            continue;
        };
        if let Some(list) = value_string_list(value) {
            out.extend(list);
        }
    }
    out
}

fn build_openvpn_settings(config: &OpenVpnConnectionConfig) -> Result<SettingsMap> {
    let mut settings = SettingsMap::new();

    let mut connection = HashMap::new();
    connection.insert("id".to_string(), owned_string(config.name.trim()));
    connection.insert("type".to_string(), owned_string("vpn"));
    connection.insert(
        "uuid".to_string(),
        owned_string(&Uuid::new_v4().to_string()),
    );
    connection.insert("autoconnect".to_string(), false.into());
    settings.insert("connection".to_string(), connection);

    let mut vpn = HashMap::new();
    vpn.insert(
        "service-type".to_string(),
        owned_string("org.freedesktop.NetworkManager.openvpn"),
    );
    if let Some(username) = config
        .username
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        vpn.insert("user-name".to_string(), owned_string(username.trim()));
    }
    vpn.insert(
        "data".to_string(),
        owned_string_map(HashMap::from([
            ("connection-type".to_string(), "password".to_string()),
            ("remote".to_string(), config.remote.trim().to_string()),
        ]))?,
    );
    settings.insert("vpn".to_string(), vpn);

    let mut ipv4 = HashMap::new();
    ipv4.insert("method".to_string(), owned_string("auto"));
    settings.insert("ipv4".to_string(), ipv4);

    let mut ipv6 = HashMap::new();
    ipv6.insert("method".to_string(), owned_string("ignore"));
    settings.insert("ipv6".to_string(), ipv6);

    Ok(settings)
}

async fn detect_vpn_file_type(path: &Path) -> Result<VpnKind> {
    let content = fs::read_to_string(path).await.unwrap_or_default();
    let lower = content.to_lowercase();
    if lower.contains("[interface]") && lower.contains("[peer]") {
        return Ok(VpnKind::WireGuard);
    }
    if lower.contains("client") || lower.contains("dev tun") || lower.contains("remote ") {
        return Ok(VpnKind::OpenVpn);
    }

    match path.extension().and_then(|ext| ext.to_str()) {
        Some("conf") => Ok(VpnKind::WireGuard),
        Some("ovpn") => Ok(VpnKind::OpenVpn),
        _ => Err(anyhow!(
            "Could not detect VPN file type. Use a WireGuard .conf or OpenVPN .ovpn file"
        )),
    }
}

async fn import_wireguard_config(
    existing_uuid: Option<&str>,
    config: &WireGuardConnectionConfig,
    reconnect_if_active: bool,
) -> Result<String> {
    let client = dbus_client().await?;
    let before = list_supported_vpn_connections().await?;
    let was_active = existing_uuid
        .and_then(|uuid| before.iter().find(|vpn| vpn.uuid == uuid))
        .map(|vpn| vpn.active)
        .unwrap_or(false);

    if let Some(uuid) = existing_uuid {
        client.deactivate_connection_by_uuid(uuid).await.ok();
        client.delete_connection_by_uuid(uuid).await?;
    }

    let temp_path = build_temp_wireguard_path(&config.name);
    let contents = build_wireguard_config_text(config);
    fs::write(&temp_path, contents).await?;
    let import_result = run_nmcli_command(&[
        "connection",
        "import",
        "type",
        "wireguard",
        "file",
        temp_path.to_string_lossy().as_ref(),
    ])
    .await;
    let _ = fs::remove_file(&temp_path).await;
    import_result?;

    let after = list_supported_vpn_connections().await?;
    let imported = after
        .iter()
        .find(|vpn| {
            !before.iter().any(|existing| existing.uuid == vpn.uuid)
                && vpn.kind == VpnKind::WireGuard
        })
        .cloned()
        .or_else(|| {
            after.into_iter().find(|vpn| {
                vpn.kind == VpnKind::WireGuard && vpn.name == sanitized_wireguard_name(&config.name)
            })
        })
        .ok_or_else(|| anyhow!("WireGuard connection was imported but could not be located"))?;

    rename_connection_uuid(&imported.uuid, config.name.trim()).await?;

    if reconnect_if_active && was_active {
        activate_vpn_connection(&imported.uuid).await.ok();
    }

    Ok(imported.uuid)
}

fn build_wireguard_config_text(config: &WireGuardConnectionConfig) -> String {
    let mut lines = vec![
        "[Interface]".to_string(),
        format!("PrivateKey = {}", config.private_key.trim()),
    ];

    if !config.addresses.is_empty() {
        lines.push(format!("Address = {}", config.addresses.join(", ")));
    }
    if !config.dns_servers.is_empty() {
        lines.push(format!("DNS = {}", config.dns_servers.join(", ")));
    }
    if let Some(mtu) = config.mtu.filter(|value| *value > 0) {
        lines.push(format!("MTU = {}", mtu));
    }

    lines.push(String::new());
    lines.push("[Peer]".to_string());
    lines.push(format!("PublicKey = {}", config.public_key.trim()));
    if let Some(preshared_key) = config
        .preshared_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("PresharedKey = {}", preshared_key));
    }
    lines.push(format!("Endpoint = {}", config.endpoint.trim()));
    if !config.allowed_ips.is_empty() {
        lines.push(format!("AllowedIPs = {}", config.allowed_ips.join(", ")));
    }
    if let Some(keepalive) = config.persistent_keepalive.filter(|value| *value > 0) {
        lines.push(format!("PersistentKeepalive = {}", keepalive));
    }

    lines.join("\n")
}

fn build_temp_wireguard_path(name: &str) -> std::path::PathBuf {
    let mut sanitized = sanitized_wireguard_name(name);
    if sanitized.is_empty() {
        sanitized = format!("adw-network-{}", Uuid::new_v4());
    }
    std::env::temp_dir().join(format!("{sanitized}.conf"))
}

fn sanitized_wireguard_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

async fn run_nmcli_command(args: &[&str]) -> Result<()> {
    let output = Command::new("nmcli").args(args).output().await?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(anyhow!("nmcli command failed"))
    } else {
        Err(anyhow!(stderr))
    }
}

fn connection_profile_is_hotspot(profile: &DbusConnectionProfile) -> bool {
    profile
        .settings
        .get("802-11-wireless")
        .and_then(|wireless| wireless.get("mode"))
        .and_then(owned_value_to_string)
        .map(|mode| mode.eq_ignore_ascii_case("ap"))
        .unwrap_or(false)
}

pub async fn get_active_hotspot_connection() -> Result<Option<DbusActiveConnection>> {
    let client = dbus_client().await?;
    let active = client.list_active_connections().await?;

    for connection in active {
        if connection.state != NM_ACTIVE_CONNECTION_STATE_ACTIVATED {
            continue;
        }

        if connection.id == "Hotspot" {
            return Ok(Some(connection));
        }

        if connection.conn_type != "802-11-wireless" && connection.conn_type != "wifi" {
            continue;
        }

        if let Ok(Some(profile)) = client.find_connection_by_uuid(&connection.uuid).await {
            if connection_profile_is_hotspot(&profile) {
                return Ok(Some(connection));
            }
        }
    }

    Ok(None)
}

pub async fn is_hotspot_active() -> Result<bool> {
    Ok(get_active_hotspot_connection().await?.is_some())
}
