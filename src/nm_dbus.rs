use anyhow::{anyhow, Context, Result};
use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use tokio::time::{sleep, Duration};
use zbus::{Connection, Proxy};
use zvariant::{Array, OwnedObjectPath, OwnedValue, Str};

use crate::config::HotspotConfig;

const NM_SERVICE: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_IFACE: &str = "org.freedesktop.NetworkManager";
const NM_SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const NM_SETTINGS_IFACE: &str = "org.freedesktop.NetworkManager.Settings";
const NM_DEVICE_IFACE: &str = "org.freedesktop.NetworkManager.Device";
const NM_WIFI_DEVICE_IFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const NM_AP_IFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";
const NM_SETTINGS_CONN_IFACE: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const NM_ACTIVE_CONN_IFACE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const NM_IP4_CONFIG_IFACE: &str = "org.freedesktop.NetworkManager.IP4Config";
const NM_DHCP4_CONFIG_IFACE: &str = "org.freedesktop.NetworkManager.DHCP4Config";

pub const NM_DEVICE_TYPE_ETHERNET: u32 = 1;
pub const NM_DEVICE_TYPE_WIFI: u32 = 2;
pub const NM_WIFI_DEVICE_CAP_AP: u32 = 0x00000040;

pub const NM_DEVICE_STATE_UNMANAGED: u32 = 10;
pub const NM_DEVICE_STATE_UNAVAILABLE: u32 = 20;

pub const NM_ACTIVE_CONNECTION_STATE_ACTIVATED: u32 = 2;

pub type SettingsMap = HashMap<String, HashMap<String, OwnedValue>>;

fn hotspot_band_for_nm(band: &str) -> Option<String> {
    let trimmed = band.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("auto")
        || trimmed.eq_ignore_ascii_case("custom")
    {
        return None;
    }
    if trimmed == "2.4 GHz" {
        return Some("bg".to_string());
    }
    if trimmed == "5 GHz" {
        return Some("a".to_string());
    }
    if trimmed == "2.4 GHz (Wider Range)" {
        return Some("bg".to_string());
    }
    if trimmed == "5 GHz (Faster Speed)" {
        return Some("a".to_string());
    }

    // * Pass through custom band strings so valid driver-specific values are still supported.
    Some(trimmed.to_string())
}

fn hotspot_channel_for_nm(channel: &str) -> Option<u32> {
    let trimmed = channel.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        return None;
    }
    // * Ignore invalid custom channel text instead of sending a bad value to NetworkManager.
    trimmed.parse::<u32>().ok().filter(|value| *value > 0)
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DbusDevice {
    pub path: OwnedObjectPath,
    pub interface: String,
    pub device_type: u32,
    pub state: u32,
    pub active_connection: Option<OwnedObjectPath>,
    pub ip4_config: Option<OwnedObjectPath>,
    pub dhcp4_config: Option<OwnedObjectPath>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct DbusConnectionProfile {
    pub path: OwnedObjectPath,
    pub id: String,
    pub uuid: String,
    pub conn_type: String,
    pub interface_name: Option<String>,
    pub autoconnect: Option<bool>,
    pub zone: Option<String>,
    pub settings: SettingsMap,
}

#[derive(Debug, Clone)]
pub struct DbusActiveConnection {
    pub path: OwnedObjectPath,
    pub id: String,
    pub uuid: String,
    pub conn_type: String,
    pub state: u32,
    pub devices: Vec<OwnedObjectPath>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DbusAccessPoint {
    pub path: OwnedObjectPath,
    pub device: OwnedObjectPath,
    pub ssid: String,
    pub frequency: u32,
    pub strength: u8,
    pub flags: u32,
    pub wpa_flags: u32,
    pub rsn_flags: u32,
    pub active: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DbusIp4Info {
    pub addresses: Vec<String>,
    pub gateway: Option<String>,
    pub dns: Vec<String>,
    pub dhcp_lease_time_seconds: Option<u32>,
}

#[derive(Clone)]
pub struct NmDbusClient {
    conn: Connection,
}

impl NmDbusClient {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            conn: Connection::system().await?,
        })
    }

    async fn proxy<'a>(&'a self, path: &'a str, iface: &'a str) -> Result<Proxy<'a>> {
        Ok(Proxy::new(&self.conn, NM_SERVICE, path, iface).await?)
    }

    fn root_path() -> Result<OwnedObjectPath> {
        Ok(OwnedObjectPath::try_from("/")?)
    }

    fn connection_section_mut<'a>(
        settings: &'a mut SettingsMap,
        section: &str,
    ) -> &'a mut HashMap<String, OwnedValue> {
        settings.entry(section.to_string()).or_default()
    }

    fn clone_settings_map(settings: &SettingsMap) -> Result<SettingsMap> {
        let mut cloned = HashMap::new();
        for (section, values) in settings {
            let mut value_map = HashMap::new();
            for (key, value) in values {
                value_map.insert(key.clone(), value.try_clone()?);
            }
            cloned.insert(section.clone(), value_map);
        }
        Ok(cloned)
    }

    fn ov_str(value: &str) -> OwnedValue {
        OwnedValue::from(Str::from(value))
    }

    fn ov_bytes(value: &[u8]) -> Result<OwnedValue> {
        let array = Array::from(value.to_vec());
        Ok(OwnedValue::try_from(array)?)
    }

    fn ov_u32_array(value: Vec<u32>) -> Result<OwnedValue> {
        let array = Array::from(value);
        Ok(OwnedValue::try_from(array)?)
    }

    fn ov_str_array(value: &[String]) -> Result<OwnedValue> {
        let array = Array::from(
            value
                .iter()
                .map(|v| Str::from(v.as_str()))
                .collect::<Vec<_>>(),
        );
        Ok(OwnedValue::try_from(array)?)
    }

    fn value_string(value: &OwnedValue) -> Option<String> {
        value
            .try_clone()
            .ok()
            .and_then(|v| String::try_from(v).ok())
            .or_else(|| {
                Vec::<u8>::try_from(value.try_clone().ok()?)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok())
            })
            .or_else(|| {
                OwnedObjectPath::try_from(value.try_clone().ok()?)
                    .ok()
                    .map(|p| p.to_string())
            })
    }

    fn value_bool(value: &OwnedValue) -> Option<bool> {
        bool::try_from(value).ok()
    }

    fn get_setting_string(settings: &SettingsMap, section: &str, key: &str) -> Option<String> {
        settings
            .get(section)
            .and_then(|s| s.get(key))
            .and_then(Self::value_string)
            .filter(|v| !v.is_empty())
    }

    fn get_setting_bool(settings: &SettingsMap, section: &str, key: &str) -> Option<bool> {
        settings
            .get(section)
            .and_then(|s| s.get(key))
            .and_then(Self::value_bool)
    }

    fn resolve_parent_interface(
        parent_ref: &str,
        by_uuid: &HashMap<String, &DbusConnectionProfile>,
        by_path: &HashMap<String, &DbusConnectionProfile>,
    ) -> Option<String> {
        if let Some(conn) = by_uuid.get(parent_ref) {
            return conn.interface_name.clone();
        }
        if parent_ref.starts_with('/') {
            return by_path
                .get(parent_ref)
                .and_then(|c| c.interface_name.clone());
        }
        by_path
            .get(parent_ref)
            .and_then(|c| c.interface_name.clone())
            .or_else(|| Some(parent_ref.to_string()))
    }

    fn collect_parent_ethernet_ifaces(connections: &[DbusConnectionProfile]) -> HashSet<String> {
        let by_uuid: HashMap<String, &DbusConnectionProfile> =
            connections.iter().map(|c| (c.uuid.clone(), c)).collect();
        let by_path: HashMap<String, &DbusConnectionProfile> = connections
            .iter()
            .map(|c| (c.path.to_string(), c))
            .collect();
        let mut parents = HashSet::new();

        for conn in connections {
            // * Keep parent interfaces up when child VLAN or bridge connections depend on them.
            let refs = [
                Self::get_setting_string(&conn.settings, "connection", "master"),
                Self::get_setting_string(&conn.settings, "connection", "controller"),
                Self::get_setting_string(&conn.settings, "vlan", "parent"),
            ];
            for parent_ref in refs.into_iter().flatten() {
                if let Some(parent_iface) =
                    Self::resolve_parent_interface(&parent_ref, &by_uuid, &by_path)
                {
                    parents.insert(parent_iface);
                }
            }
        }

        parents
    }

    pub async fn is_wifi_enabled(&self) -> Result<bool> {
        let nm = self.proxy(NM_PATH, NM_IFACE).await?;
        Ok(nm.get_property("WirelessEnabled").await?)
    }

    pub async fn set_wifi_enabled(&self, enabled: bool) -> Result<()> {
        let nm = self.proxy(NM_PATH, NM_IFACE).await?;
        nm.set_property("WirelessEnabled", &enabled).await?;
        Ok(())
    }

    pub async fn is_wifi_present(&self) -> Result<bool> {
        let nm = self.proxy(NM_PATH, NM_IFACE).await?;
        let enabled_hw: bool = nm.get_property("WirelessHardwareEnabled").await?;
        Ok(enabled_hw)
    }

    pub async fn get_connectivity_state(&self) -> Result<u32> {
        let nm = self.proxy(NM_PATH, NM_IFACE).await?;
        let state: u32 = nm.get_property("Connectivity").await.unwrap_or(0);
        Ok(state)
    }

    pub async fn list_device_paths(&self) -> Result<Vec<OwnedObjectPath>> {
        let nm = self.proxy(NM_PATH, NM_IFACE).await?;
        let devices: Vec<OwnedObjectPath> = nm.call("GetDevices", &()).await?;
        Ok(devices)
    }

    pub async fn list_devices(&self) -> Result<Vec<DbusDevice>> {
        let mut devices = Vec::new();
        for path in self.list_device_paths().await? {
            let dev = self.proxy(path.as_str(), NM_DEVICE_IFACE).await?;
            let interface: String = dev.get_property("Interface").await?;
            let device_type: u32 = dev.get_property("DeviceType").await?;
            let state: u32 = dev.get_property("State").await?;

            let active_connection: OwnedObjectPath = dev.get_property("ActiveConnection").await?;
            let ip4_config: OwnedObjectPath = dev.get_property("Ip4Config").await?;
            let dhcp4_config: OwnedObjectPath = dev.get_property("Dhcp4Config").await?;

            devices.push(DbusDevice {
                path,
                interface,
                device_type,
                state,
                active_connection: if active_connection.as_str() == "/" {
                    None
                } else {
                    Some(active_connection)
                },
                ip4_config: if ip4_config.as_str() == "/" {
                    None
                } else {
                    Some(ip4_config)
                },
                dhcp4_config: if dhcp4_config.as_str() == "/" {
                    None
                } else {
                    Some(dhcp4_config)
                },
            });
        }
        Ok(devices)
    }

    pub async fn get_wifi_devices(&self) -> Result<Vec<DbusDevice>> {
        let devices = self.list_devices().await?;
        Ok(devices
            .into_iter()
            .filter(|d| {
                d.device_type == NM_DEVICE_TYPE_WIFI
                    && d.state != NM_DEVICE_STATE_UNMANAGED
                    && d.state != NM_DEVICE_STATE_UNAVAILABLE
            })
            .collect())
    }

    pub async fn get_ethernet_devices(&self) -> Result<Vec<DbusDevice>> {
        let devices = self.list_devices().await?;
        Ok(devices
            .into_iter()
            .filter(|d| d.device_type == NM_DEVICE_TYPE_ETHERNET)
            .collect())
    }

    pub async fn request_wifi_scan(&self) -> Result<()> {
        for device in self.get_wifi_devices().await? {
            let wifi = self
                .proxy(device.path.as_str(), NM_WIFI_DEVICE_IFACE)
                .await?;
            let opts: HashMap<String, OwnedValue> = HashMap::new();
            let _: () = wifi.call("RequestScan", &(opts)).await?;
        }
        Ok(())
    }

    pub async fn list_access_points(&self) -> Result<Vec<DbusAccessPoint>> {
        let mut aps = Vec::new();

        for device in self.get_wifi_devices().await? {
            let wifi = self
                .proxy(device.path.as_str(), NM_WIFI_DEVICE_IFACE)
                .await?;
            let active_ap: OwnedObjectPath = wifi.get_property("ActiveAccessPoint").await?;
            let ap_paths: Vec<OwnedObjectPath> = wifi.call("GetAllAccessPoints", &()).await?;

            for ap_path in ap_paths {
                let ap = self.proxy(ap_path.as_str(), NM_AP_IFACE).await?;
                let ssid_raw: Vec<u8> = ap.get_property("Ssid").await.unwrap_or_default();
                let ssid = String::from_utf8_lossy(&ssid_raw).trim().to_string();
                if ssid.is_empty() {
                    continue;
                }

                let frequency: u32 = ap.get_property("Frequency").await.unwrap_or(0);
                let strength: u8 = ap.get_property("Strength").await.unwrap_or(0);
                let flags: u32 = ap.get_property("Flags").await.unwrap_or(0);
                let wpa_flags: u32 = ap.get_property("WpaFlags").await.unwrap_or(0);
                let rsn_flags: u32 = ap.get_property("RsnFlags").await.unwrap_or(0);

                aps.push(DbusAccessPoint {
                    path: ap_path.clone(),
                    device: device.path.clone(),
                    ssid,
                    frequency,
                    strength,
                    flags,
                    wpa_flags,
                    rsn_flags,
                    active: ap_path == active_ap,
                });
            }
        }

        Ok(aps)
    }

    pub async fn list_connection_paths(&self) -> Result<Vec<OwnedObjectPath>> {
        let settings = self.proxy(NM_SETTINGS_PATH, NM_SETTINGS_IFACE).await?;
        let paths: Vec<OwnedObjectPath> = settings.call("ListConnections", &()).await?;
        Ok(paths)
    }

    pub async fn get_connection_settings(&self, path: &OwnedObjectPath) -> Result<SettingsMap> {
        let conn = self.proxy(path.as_str(), NM_SETTINGS_CONN_IFACE).await?;
        let settings: SettingsMap = conn.call("GetSettings", &()).await?;
        Ok(settings)
    }

    pub async fn update_connection_settings(
        &self,
        path: &OwnedObjectPath,
        settings: &SettingsMap,
    ) -> Result<()> {
        let conn = self.proxy(path.as_str(), NM_SETTINGS_CONN_IFACE).await?;
        let _: () = conn.call("Update", &(settings)).await?;
        Ok(())
    }

    pub async fn list_connections(&self) -> Result<Vec<DbusConnectionProfile>> {
        let mut out = Vec::new();

        for path in self.list_connection_paths().await? {
            let settings = self
                .get_connection_settings(&path)
                .await
                .unwrap_or_default();

            let id = Self::get_setting_string(&settings, "connection", "id").unwrap_or_default();
            let uuid =
                Self::get_setting_string(&settings, "connection", "uuid").unwrap_or_default();
            let conn_type =
                Self::get_setting_string(&settings, "connection", "type").unwrap_or_default();

            if id.is_empty() || uuid.is_empty() || conn_type.is_empty() {
                continue;
            }

            let interface_name =
                Self::get_setting_string(&settings, "connection", "interface-name");
            let autoconnect = Self::get_setting_bool(&settings, "connection", "autoconnect");
            let zone = Self::get_setting_string(&settings, "connection", "zone");

            out.push(DbusConnectionProfile {
                path,
                id,
                uuid,
                conn_type,
                interface_name,
                autoconnect,
                zone,
                settings,
            });
        }

        Ok(out)
    }

    pub async fn list_active_connections(&self) -> Result<Vec<DbusActiveConnection>> {
        let nm = self.proxy(NM_PATH, NM_IFACE).await?;
        let active_paths: Vec<OwnedObjectPath> = nm.get_property("ActiveConnections").await?;

        let mut out = Vec::new();
        for path in active_paths {
            let active = self.proxy(path.as_str(), NM_ACTIVE_CONN_IFACE).await?;
            let id: String = active.get_property("Id").await.unwrap_or_default();
            let uuid: String = active.get_property("Uuid").await.unwrap_or_default();
            let conn_type: String = active.get_property("Type").await.unwrap_or_default();
            let state: u32 = active.get_property("State").await.unwrap_or(0);
            let devices: Vec<OwnedObjectPath> =
                active.get_property("Devices").await.unwrap_or_default();

            out.push(DbusActiveConnection {
                path,
                id,
                uuid,
                conn_type,
                state,
                devices,
            });
        }

        Ok(out)
    }

    pub async fn find_connection_by_id(&self, id: &str) -> Result<Option<DbusConnectionProfile>> {
        Ok(self
            .list_connections()
            .await?
            .into_iter()
            .find(|c| c.id == id))
    }

    pub async fn find_connection_by_uuid(
        &self,
        uuid: &str,
    ) -> Result<Option<DbusConnectionProfile>> {
        Ok(self
            .list_connections()
            .await?
            .into_iter()
            .find(|c| c.uuid == uuid))
    }

    pub async fn activate_connection_path(
        &self,
        connection_path: &OwnedObjectPath,
        iface: Option<&str>,
    ) -> Result<OwnedObjectPath> {
        let devices = self.list_devices().await?;
        let device = if let Some(iface) = iface {
            devices
                .into_iter()
                .find(|d| d.interface == iface)
                .ok_or_else(|| anyhow!("Network device {} not found", iface))?
        } else {
            devices
                .into_iter()
                .find(|d| d.state != NM_DEVICE_STATE_UNMANAGED)
                .ok_or_else(|| anyhow!("No managed NetworkManager device found"))?
        };

        let nm = self.proxy(NM_PATH, NM_IFACE).await?;
        let root = Self::root_path()?;
        let active_path: OwnedObjectPath = nm
            .call(
                "ActivateConnection",
                &(connection_path.clone(), device.path.clone(), root),
            )
            .await?;
        Ok(active_path)
    }

    pub async fn activate_connection_by_id(
        &self,
        id: &str,
        iface: Option<&str>,
    ) -> Result<OwnedObjectPath> {
        let connection = self
            .find_connection_by_id(id)
            .await?
            .ok_or_else(|| anyhow!("Connection {} not found", id))?;
        self.activate_connection_path(&connection.path, iface).await
    }

    pub async fn activate_connection_by_uuid(
        &self,
        uuid: &str,
        iface: Option<&str>,
    ) -> Result<OwnedObjectPath> {
        let connection = self
            .find_connection_by_uuid(uuid)
            .await?
            .ok_or_else(|| anyhow!("Connection {} not found", uuid))?;
        self.activate_connection_path(&connection.path, iface).await
    }

    async fn wait_for_wifi_activation(
        &self,
        active_path: &OwnedObjectPath,
        ssid: &str,
        hidden: bool,
    ) -> Result<()> {
        let max_attempts = if hidden { 30 } else { 20 };

        for _ in 0..max_attempts {
            if self.get_active_wifi_ssid().await?.as_deref() == Some(ssid) {
                return Ok(());
            }

            if let Ok(active) = self.proxy(active_path.as_str(), NM_ACTIVE_CONN_IFACE).await {
                let state: u32 = active.get_property("State").await.unwrap_or(0);
                let id: String = active.get_property("Id").await.unwrap_or_default();
                if state == NM_ACTIVE_CONNECTION_STATE_ACTIVATED && (id.is_empty() || id == ssid) {
                    return Ok(());
                }
                if matches!(state, 3 | 4) {
                    break;
                }
            }

            sleep(Duration::from_millis(350)).await;
        }

        if hidden {
            Err(anyhow!(
                "Hidden Wi-Fi network {} could not be found or activated",
                ssid
            ))
        } else {
            Err(anyhow!("Wi-Fi network {} could not be activated", ssid))
        }
    }

    pub async fn deactivate_connection_by_id(&self, id: &str) -> Result<()> {
        let nm = self.proxy(NM_PATH, NM_IFACE).await?;
        let active = self.list_active_connections().await?;

        for conn in active {
            if conn.id == id {
                let _: () = nm.call("DeactivateConnection", &(conn.path)).await?;
            }
        }

        Ok(())
    }

    pub async fn deactivate_connection_by_uuid(&self, uuid: &str) -> Result<()> {
        let nm = self.proxy(NM_PATH, NM_IFACE).await?;
        let active = self.list_active_connections().await?;

        for conn in active {
            if conn.uuid == uuid {
                let _: () = nm.call("DeactivateConnection", &(conn.path)).await?;
            }
        }

        Ok(())
    }

    pub async fn delete_connection_by_id(&self, id: &str) -> Result<()> {
        if let Some(conn) = self.find_connection_by_id(id).await? {
            let settings_conn = self
                .proxy(conn.path.as_str(), NM_SETTINGS_CONN_IFACE)
                .await?;
            let _: () = settings_conn.call("Delete", &()).await?;
        }
        Ok(())
    }

    pub async fn delete_connection_by_uuid(&self, uuid: &str) -> Result<()> {
        if let Some(conn) = self.find_connection_by_uuid(uuid).await? {
            let settings_conn = self
                .proxy(conn.path.as_str(), NM_SETTINGS_CONN_IFACE)
                .await?;
            let _: () = settings_conn.call("Delete", &()).await?;
        }
        Ok(())
    }

    pub async fn add_connection(&self, settings: &SettingsMap) -> Result<OwnedObjectPath> {
        let settings_proxy = self.proxy(NM_SETTINGS_PATH, NM_SETTINGS_IFACE).await?;
        let path: OwnedObjectPath = settings_proxy.call("AddConnection", &(settings)).await?;
        Ok(path)
    }

    pub async fn ensure_wifi_device_ready(&self, iface: &str) -> Result<()> {
        if !self.is_wifi_enabled().await.unwrap_or(false) {
            self.set_wifi_enabled(true).await?;
        }

        let device = self
            .list_devices()
            .await?
            .into_iter()
            .find(|d| d.interface == iface && d.device_type == NM_DEVICE_TYPE_WIFI)
            .ok_or_else(|| anyhow!("Wi-Fi interface {} not available", iface))?;

        if device.state == NM_DEVICE_STATE_UNMANAGED {
            return Err(anyhow!("Wi-Fi interface {} is unmanaged", iface));
        }
        if device.state == NM_DEVICE_STATE_UNAVAILABLE {
            // * Surface an explicit unavailable-adapter state before hotspot activation.
            return Err(anyhow!("Wi-Fi interface {} is unavailable", iface));
        }

        let wifi = self
            .proxy(device.path.as_str(), NM_WIFI_DEVICE_IFACE)
            .await?;
        let caps: u32 = wifi.get_property("WirelessCapabilities").await.unwrap_or(0);
        if caps & NM_WIFI_DEVICE_CAP_AP == 0 {
            // * Detect adapters that cannot enter AP or hotspot mode before activation.
            return Err(anyhow!(
                "Wi-Fi interface {} hotspot mode not supported",
                iface
            ));
        }

        Ok(())
    }

    pub async fn upsert_hotspot_connection(
        &self,
        config: &HotspotConfig,
        iface: &str,
    ) -> Result<()> {
        let mut settings: SettingsMap = HashMap::new();

        let mut connection = HashMap::new();
        connection.insert("id".to_string(), Self::ov_str("Hotspot"));
        connection.insert("type".to_string(), Self::ov_str("802-11-wireless"));
        connection.insert("autoconnect".to_string(), false.into());
        connection.insert("interface-name".to_string(), Self::ov_str(iface));
        settings.insert("connection".to_string(), connection);

        let mut wifi = HashMap::new();
        wifi.insert("ssid".to_string(), Self::ov_bytes(config.ssid.as_bytes())?);
        wifi.insert("mode".to_string(), Self::ov_str("ap"));
        wifi.insert("hidden".to_string(), config.hidden.into());
        if let Some(band) = hotspot_band_for_nm(&config.band) {
            // * Persist both predefined and custom hotspot band values into NM settings.
            wifi.insert("band".to_string(), Self::ov_str(&band));
        }
        if let Some(channel) = hotspot_channel_for_nm(&config.channel) {
            // * Persist numeric custom channel values when the UI provides one.
            wifi.insert("channel".to_string(), channel.into());
        }
        settings.insert("802-11-wireless".to_string(), wifi);

        let mut ipv4 = HashMap::new();
        ipv4.insert("method".to_string(), Self::ov_str("shared"));
        settings.insert("ipv4".to_string(), ipv4);

        let mut ipv6 = HashMap::new();
        ipv6.insert("method".to_string(), Self::ov_str("disabled"));
        settings.insert("ipv6".to_string(), ipv6);

        if !config.password.is_empty() {
            let mut sec = HashMap::new();
            sec.insert("key-mgmt".to_string(), Self::ov_str("wpa-psk"));
            sec.insert("psk".to_string(), Self::ov_str(&config.password));
            settings.insert("802-11-wireless-security".to_string(), sec);
        }

        if let Some(existing) = self.find_connection_by_id("Hotspot").await? {
            self.update_connection_settings(&existing.path, &settings)
                .await?;
            return Ok(());
        }

        let settings_proxy = self.proxy(NM_SETTINGS_PATH, NM_SETTINGS_IFACE).await?;
        let _: OwnedObjectPath = settings_proxy.call("AddConnection", &(settings)).await?;

        Ok(())
    }

    pub async fn add_and_activate_wifi_connection(
        &self,
        ssid: &str,
        password: Option<&str>,
        key_mgmt: Option<&str>,
        hidden: bool,
    ) -> Result<()> {
        let device = self
            .get_wifi_devices()
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No Wi-Fi device available"))?;
        let existing_connection = self.find_connection_by_id(ssid).await?;

        let mut settings: SettingsMap = HashMap::new();

        let mut connection = HashMap::new();
        connection.insert("id".to_string(), Self::ov_str(ssid));
        connection.insert("type".to_string(), Self::ov_str("802-11-wireless"));
        connection.insert("autoconnect".to_string(), true.into());
        settings.insert("connection".to_string(), connection);

        let mut wifi = HashMap::new();
        wifi.insert("ssid".to_string(), Self::ov_bytes(ssid.as_bytes())?);
        wifi.insert("mode".to_string(), Self::ov_str("infrastructure"));
        if hidden {
            wifi.insert("hidden".to_string(), true.into());
        }
        settings.insert("802-11-wireless".to_string(), wifi);

        if let Some(password) = password {
            let mut sec = HashMap::new();
            sec.insert(
                "key-mgmt".to_string(),
                Self::ov_str(key_mgmt.unwrap_or("wpa-psk")),
            );
            if key_mgmt == Some("none") {
                sec.insert("wep-key0".to_string(), Self::ov_str(password));
            } else {
                sec.insert("psk".to_string(), Self::ov_str(password));
            }
            settings.insert("802-11-wireless-security".to_string(), sec);
        }

        let nm = self.proxy(NM_PATH, NM_IFACE).await?;
        let root = Self::root_path()?;

        let add_result: Result<(OwnedObjectPath, OwnedObjectPath), zbus::Error> = nm
            .call(
                "AddAndActivateConnection",
                &(settings, device.path.clone(), root.clone()),
            )
            .await;

        if let Ok((_, active_path)) = add_result {
            if let Err(e) = self
                .wait_for_wifi_activation(&active_path, ssid, hidden)
                .await
            {
                if existing_connection.is_none() {
                    let _ = self.delete_connection_by_id(ssid).await;
                }
                return Err(e);
            }
            return Ok(());
        }

        // If connection already exists or AddAndActivate fails for policy reasons,
        // attempt regular activation of an existing profile.
        if let Some(existing) = self.find_connection_by_id(ssid).await? {
            let active_path = self
                .activate_connection_path(&existing.path, Some(&device.interface))
                .await?;
            self.wait_for_wifi_activation(&active_path, ssid, hidden)
                .await?;
            return Ok(());
        }

        Err(anyhow!("Failed to activate Wi-Fi connection {}", ssid))
    }

    pub async fn disconnect_connection_by_id(&self, id: &str) -> Result<()> {
        self.deactivate_connection_by_id(id).await
    }

    pub async fn set_connection_autoconnect_by_uuid(
        &self,
        uuid: &str,
        enabled: bool,
    ) -> Result<()> {
        let conn = self
            .find_connection_by_uuid(uuid)
            .await?
            .ok_or_else(|| anyhow!("Connection {} not found", uuid))?;

        let mut settings = Self::clone_settings_map(&conn.settings)?;
        Self::connection_section_mut(&mut settings, "connection")
            .insert("autoconnect".to_string(), enabled.into());
        self.update_connection_settings(&conn.path, &settings).await
    }

    pub async fn set_connection_autoconnect_by_id(&self, id: &str, enabled: bool) -> Result<()> {
        let conn = self
            .find_connection_by_id(id)
            .await?
            .ok_or_else(|| anyhow!("Connection {} not found", id))?;

        let mut settings = Self::clone_settings_map(&conn.settings)?;
        Self::connection_section_mut(&mut settings, "connection")
            .insert("autoconnect".to_string(), enabled.into());
        self.update_connection_settings(&conn.path, &settings).await
    }

    pub async fn get_connection_autoconnect_by_id(&self, id: &str) -> Result<bool> {
        let conn = self
            .find_connection_by_id(id)
            .await?
            .ok_or_else(|| anyhow!("Connection {} not found", id))?;
        Ok(conn.autoconnect.unwrap_or(false))
    }

    pub async fn set_connection_zone_by_uuid(&self, uuid: &str, zone: &str) -> Result<()> {
        let conn = self
            .find_connection_by_uuid(uuid)
            .await?
            .ok_or_else(|| anyhow!("Connection {} not found", uuid))?;

        let mut settings = Self::clone_settings_map(&conn.settings)?;
        Self::connection_section_mut(&mut settings, "connection")
            .insert("zone".to_string(), Self::ov_str(zone));
        self.update_connection_settings(&conn.path, &settings).await
    }

    pub async fn set_custom_ipv4_dns_for_connection(
        &self,
        id: &str,
        dns_servers: &[String],
        search_domains: &[String],
    ) -> Result<()> {
        let conn = self
            .find_connection_by_id(id)
            .await?
            .ok_or_else(|| anyhow!("Connection {} not found", id))?;

        let mut settings = Self::clone_settings_map(&conn.settings)?;
        let ipv4 = Self::connection_section_mut(&mut settings, "ipv4");

        let dns_u32: Vec<u32> = dns_servers
            .iter()
            .filter_map(|raw| raw.parse::<Ipv4Addr>().ok())
            .map(u32::from)
            .collect();

        if dns_u32.is_empty() {
            return Err(anyhow!("At least one IPv4 DNS server is required"));
        }

        ipv4.insert("dns".to_string(), Self::ov_u32_array(dns_u32)?);
        ipv4.insert("ignore-auto-dns".to_string(), true.into());
        if !search_domains.is_empty() {
            ipv4.insert(
                "dns-search".to_string(),
                Self::ov_str_array(search_domains)?,
            );
        }

        self.update_connection_settings(&conn.path, &settings).await
    }

    pub async fn reapply_connection(&self, id: &str) -> Result<()> {
        let conn = self
            .find_connection_by_id(id)
            .await?
            .ok_or_else(|| anyhow!("Connection {} not found", id))?;

        let iface = conn.interface_name.clone();
        self.deactivate_connection_by_uuid(&conn.uuid).await?;
        self.activate_connection_path(&conn.path, iface.as_deref())
            .await?;
        Ok(())
    }

    pub async fn get_ip4_info(&self, device_path: &OwnedObjectPath) -> Result<DbusIp4Info> {
        let dev = self.proxy(device_path.as_str(), NM_DEVICE_IFACE).await?;
        let ip4_path: OwnedObjectPath = dev.get_property("Ip4Config").await?;
        let dhcp4_path: OwnedObjectPath = dev.get_property("Dhcp4Config").await?;

        if ip4_path.as_str() == "/" {
            return Ok(DbusIp4Info::default());
        }

        let ip4 = self.proxy(ip4_path.as_str(), NM_IP4_CONFIG_IFACE).await?;

        let mut out = DbusIp4Info::default();

        let address_data: Vec<HashMap<String, OwnedValue>> =
            ip4.get_property("AddressData").await.unwrap_or_default();
        for item in address_data {
            if let Some(value) = item.get("address").and_then(Self::value_string) {
                if !value.is_empty() {
                    out.addresses.push(value);
                }
            }
        }

        let gateway: String = ip4.get_property("Gateway").await.unwrap_or_default();
        if !gateway.is_empty() {
            out.gateway = Some(gateway);
        }

        let nameserver_data: Vec<HashMap<String, OwnedValue>> =
            ip4.get_property("NameserverData").await.unwrap_or_default();
        for item in nameserver_data {
            if let Some(value) = item.get("address").and_then(Self::value_string) {
                if !value.is_empty() {
                    out.dns.push(value);
                }
            }
        }

        if dhcp4_path.as_str() != "/" {
            let dhcp4 = self
                .proxy(dhcp4_path.as_str(), NM_DHCP4_CONFIG_IFACE)
                .await?;
            let options: HashMap<String, String> =
                dhcp4.get_property("Options").await.unwrap_or_default();
            out.dhcp_lease_time_seconds = options
                .get("dhcp_lease_time")
                .and_then(|v| v.parse::<u32>().ok());
        }

        Ok(out)
    }

    pub async fn get_active_wifi_ssid(&self) -> Result<Option<String>> {
        let active = self.list_active_connections().await?;
        for conn in active {
            if conn.conn_type == "802-11-wireless"
                && conn.state == NM_ACTIVE_CONNECTION_STATE_ACTIVATED
                && !conn.id.is_empty()
            {
                return Ok(Some(conn.id));
            }
        }
        Ok(None)
    }

    pub async fn get_active_wired_connection(&self) -> Result<Option<String>> {
        let active = self.list_active_connections().await?;
        for conn in active {
            if (conn.conn_type == "802-3-ethernet" || conn.conn_type == "ethernet")
                && conn.state == NM_ACTIVE_CONNECTION_STATE_ACTIVATED
                && !conn.id.is_empty()
            {
                return Ok(Some(conn.id));
            }
        }
        Ok(None)
    }

    pub async fn get_active_connection_name(&self) -> Result<Option<String>> {
        let active = self.list_active_connections().await?;

        if let Some(wifi) = active.iter().find(|c| {
            c.conn_type == "802-11-wireless" && c.state == NM_ACTIVE_CONNECTION_STATE_ACTIVATED
        }) {
            return Ok(Some(wifi.id.clone()));
        }

        Ok(active
            .into_iter()
            .find(|c| c.state == NM_ACTIVE_CONNECTION_STATE_ACTIVATED)
            .map(|c| c.id))
    }

    pub async fn get_primary_connected_device(&self) -> Result<Option<String>> {
        let devices = self.list_devices().await?;

        let mut wired = None;
        let mut wifi = None;

        for d in devices {
            if d.active_connection.is_none() {
                continue;
            }
            match d.device_type {
                NM_DEVICE_TYPE_ETHERNET => {
                    if wired.is_none() {
                        wired = Some(d.interface.clone());
                    }
                }
                NM_DEVICE_TYPE_WIFI => {
                    if wifi.is_none() {
                        wifi = Some(d.interface.clone());
                    }
                }
                _ => {}
            }
        }

        Ok(wired.or(wifi))
    }

    pub async fn get_hotspot_ip(&self) -> Result<Option<String>> {
        let active = self.list_active_connections().await?;
        let hotspot = active
            .into_iter()
            .find(|c| c.id == "Hotspot" && c.state == NM_ACTIVE_CONNECTION_STATE_ACTIVATED);

        let Some(hotspot) = hotspot else {
            return Ok(None);
        };

        let Some(device) = hotspot.devices.first() else {
            return Ok(None);
        };

        let ip4 = self.get_ip4_info(device).await?;
        Ok(ip4.addresses.into_iter().next())
    }

    pub async fn get_network_info_by_id(
        &self,
        id: &str,
    ) -> Result<(
        Option<DbusConnectionProfile>,
        Option<DbusActiveConnection>,
        Option<DbusDevice>,
        DbusIp4Info,
    )> {
        let profile = self.find_connection_by_id(id).await?;
        let active = self.list_active_connections().await?.into_iter().find(|c| {
            c.id == id
                || c.uuid
                    == profile
                        .as_ref()
                        .map(|p| p.uuid.as_str())
                        .unwrap_or_default()
        });

        let device = if let Some(active) = &active {
            if let Some(path) = active.devices.first() {
                self.list_devices()
                    .await?
                    .into_iter()
                    .find(|d| d.path == *path)
            } else {
                None
            }
        } else {
            None
        };

        let ip4_info = if let Some(device) = &device {
            self.get_ip4_info(&device.path).await.unwrap_or_default()
        } else {
            DbusIp4Info::default()
        };

        Ok((profile, active, device, ip4_info))
    }

    pub async fn set_ethernet_enabled(&self, enabled: bool) -> Result<()> {
        let devices = self.get_ethernet_devices().await?;
        let nm = self.proxy(NM_PATH, NM_IFACE).await?;
        let root = Self::root_path()?;
        let all_connections = self.list_connections().await?;
        let parent_ethernet_ifaces = Self::collect_parent_ethernet_ifaces(&all_connections);
        let ethernet_connections: Vec<&DbusConnectionProfile> = all_connections
            .iter()
            .filter(|c| c.conn_type == "802-3-ethernet" || c.conn_type == "ethernet")
            .collect();
        let active_connections = self.list_active_connections().await?;
        let active_by_path: HashMap<String, DbusActiveConnection> = active_connections
            .into_iter()
            .map(|c| (c.path.to_string(), c))
            .collect();
        let mut used_connection_paths = HashSet::new();

        for dev in devices {
            if enabled {
                let exact = ethernet_connections
                    .iter()
                    .copied()
                    .find(|c| c.interface_name.as_deref() == Some(dev.interface.as_str()));
                let generic = ethernet_connections.iter().copied().find(|c| {
                    c.interface_name.is_none()
                        && !used_connection_paths.contains(&c.path.to_string())
                });
                let candidate = exact.or(generic);

                if let Some(conn) = candidate {
                    used_connection_paths.insert(conn.path.to_string());
                    let _: OwnedObjectPath = nm
                        .call(
                            "ActivateConnection",
                            &(conn.path.clone(), dev.path.clone(), root.clone()),
                        )
                        .await
                        .with_context(|| {
                            format!("Failed to activate ethernet on {}", dev.interface)
                        })?;
                }
            } else if let Some(active_path) = dev.active_connection {
                if dev.interface.contains('.') || dev.interface.contains(':') {
                    // * Skip aliased/sub-interface links while toggling ethernet off.
                    continue;
                }
                if parent_ethernet_ifaces.contains(&dev.interface) {
                    // * Keep parent links up when child virtual connections depend on them.
                    continue;
                }
                let should_deactivate = active_by_path
                    .get(active_path.as_str())
                    // * Deactivate only canonical ethernet active connections.
                    .map(|conn| conn.conn_type == "802-3-ethernet")
                    .unwrap_or(false);
                if should_deactivate {
                    let _: () = nm.call("DeactivateConnection", &(active_path)).await?;
                }
            }
        }

        Ok(())
    }

    pub async fn is_ethernet_enabled(&self) -> Result<bool> {
        let devices = self.get_ethernet_devices().await?;
        Ok(devices.into_iter().any(|d| {
            d.state != NM_DEVICE_STATE_UNAVAILABLE && d.state != NM_DEVICE_STATE_UNMANAGED
        }))
    }
}
