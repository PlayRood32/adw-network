use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use zbus::{Connection, Proxy};
use zvariant::{OwnedObjectPath, OwnedValue, Str};

const MM_SERVICE: &str = "org.freedesktop.ModemManager1";
const MM_PATH: &str = "/org/freedesktop/ModemManager1";
const MM_OBJECT_MANAGER_IFACE: &str = "org.freedesktop.DBus.ObjectManager";
const MM_MODEM_IFACE: &str = "org.freedesktop.ModemManager1.Modem";
const MM_MODEM_SIMPLE_IFACE: &str = "org.freedesktop.ModemManager1.Modem.Simple";
const MM_MODEM_3GPP_IFACE: &str = "org.freedesktop.ModemManager1.Modem.Modem3gpp";
const MM_SIM_IFACE: &str = "org.freedesktop.ModemManager1.Sim";
const MM_BEARER_IFACE: &str = "org.freedesktop.ModemManager1.Bearer";

type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

#[derive(Debug, Clone, Default)]
pub struct MobileDataStatus {
    pub service_available: bool,
    pub modem_present: bool,
    pub modem_path: Option<String>,
    pub device_name: Option<String>,
    pub state_label: String,
    pub connected: bool,
    pub radio_enabled: bool,
    pub signal_quality_percent: Option<u8>,
    pub network_generation: Option<String>,
    pub operator_name: Option<String>,
    pub sim_status: String,
    pub pin_status: String,
    pub apn: Option<String>,
}

#[derive(Clone)]
struct ModemManagerClient {
    conn: Connection,
}

#[derive(Debug, Clone)]
struct ParsedModem {
    status: MobileDataStatus,
    primary_bearer: Option<OwnedObjectPath>,
}

impl ModemManagerClient {
    async fn new() -> Result<Self> {
        Ok(Self {
            conn: Connection::system().await?,
        })
    }

    async fn proxy<'a>(&'a self, path: &'a str, iface: &'a str) -> Result<Proxy<'a>> {
        Ok(Proxy::new(&self.conn, MM_SERVICE, path, iface).await?)
    }

    async fn managed_objects(&self) -> Result<ManagedObjects> {
        let proxy = self.proxy(MM_PATH, MM_OBJECT_MANAGER_IFACE).await?;
        let objects: ManagedObjects = proxy.call("GetManagedObjects", &()).await?;
        Ok(objects)
    }

    async fn first_modem(&self) -> Result<ParsedModem> {
        let objects = self.managed_objects().await?;
        parse_first_modem(&objects).ok_or_else(|| anyhow!("No mobile modem detected"))
    }
}

pub async fn get_mobile_data_status() -> Result<MobileDataStatus> {
    let client = match ModemManagerClient::new().await {
        Ok(client) => client,
        Err(e) => {
            if is_modemmanager_unavailable_error(&e.to_string()) {
                return Ok(MobileDataStatus {
                    service_available: false,
                    modem_present: false,
                    state_label: "ModemManager not available".to_string(),
                    sim_status: "Unavailable".to_string(),
                    pin_status: "Unavailable".to_string(),
                    ..MobileDataStatus::default()
                });
            }
            return Err(e);
        }
    };

    let objects = match client.managed_objects().await {
        Ok(objects) => objects,
        Err(e) => {
            if is_modemmanager_unavailable_error(&e.to_string()) {
                return Ok(MobileDataStatus {
                    service_available: false,
                    modem_present: false,
                    state_label: "ModemManager not available".to_string(),
                    sim_status: "Unavailable".to_string(),
                    pin_status: "Unavailable".to_string(),
                    ..MobileDataStatus::default()
                });
            }
            return Err(e);
        }
    };

    Ok(parse_first_modem(&objects)
        .map(|parsed| parsed.status)
        .unwrap_or_else(|| MobileDataStatus {
            service_available: true,
            modem_present: false,
            state_label: "No mobile modem detected".to_string(),
            sim_status: "Unavailable".to_string(),
            pin_status: "Unavailable".to_string(),
            ..MobileDataStatus::default()
        }))
}

pub async fn connect_mobile_data(apn: Option<&str>) -> Result<()> {
    let client = ModemManagerClient::new().await?;
    let modem = client.first_modem().await?;
    let Some(modem_path) = modem.status.modem_path.as_deref() else {
        return Err(anyhow!("No mobile modem detected"));
    };

    let proxy = client.proxy(modem_path, MM_MODEM_SIMPLE_IFACE).await?;
    let mut properties: HashMap<&str, OwnedValue> = HashMap::new();
    if let Some(apn) = apn.map(str::trim).filter(|value| !value.is_empty()) {
        properties.insert("apn", OwnedValue::from(Str::from(apn)));
    }

    let _: OwnedObjectPath = proxy
        .call("Connect", &(properties))
        .await
        .with_context(|| "Failed to connect mobile data")?;
    Ok(())
}

pub async fn disconnect_mobile_data() -> Result<()> {
    let client = ModemManagerClient::new().await?;
    let modem = client.first_modem().await?;
    let Some(modem_path) = modem.status.modem_path.as_deref() else {
        return Err(anyhow!("No mobile modem detected"));
    };

    let proxy = client.proxy(modem_path, MM_MODEM_SIMPLE_IFACE).await?;
    let bearer = modem
        .primary_bearer
        .unwrap_or_else(|| OwnedObjectPath::try_from("/").expect("root path is valid"));
    let _: () = proxy
        .call("Disconnect", &(bearer))
        .await
        .with_context(|| "Failed to disconnect mobile data")?;
    Ok(())
}

pub async fn set_radio_enabled(enabled: bool) -> Result<()> {
    let client = ModemManagerClient::new().await?;
    let modem = client.first_modem().await?;
    let Some(modem_path) = modem.status.modem_path.as_deref() else {
        return Err(anyhow!("No mobile modem detected"));
    };

    let proxy = client.proxy(modem_path, MM_MODEM_IFACE).await?;
    let _: () = proxy.call("Enable", &(enabled)).await.with_context(|| {
        if enabled {
            "Failed to enable mobile radio"
        } else {
            "Failed to disable mobile radio"
        }
    })?;
    Ok(())
}

pub fn is_modemmanager_unavailable_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("org.freedesktop.modemmanager1")
        && (lower.contains("name has no owner")
            || lower.contains("serviceunknown")
            || lower.contains("unknownmethod")
            || lower.contains("was not provided by any .service files")
            || lower.contains("spawn.service not found")
            || lower.contains("connection refused"))
}

fn parse_first_modem(objects: &ManagedObjects) -> Option<ParsedModem> {
    let (modem_path, modem_ifaces) = objects
        .iter()
        .find(|(_, ifaces)| ifaces.contains_key(MM_MODEM_IFACE))?;

    let modem = modem_ifaces.get(MM_MODEM_IFACE)?;
    let modem_3gpp = modem_ifaces.get(MM_MODEM_3GPP_IFACE);

    let modem_state = value_i32(modem.get("State")).unwrap_or(0);
    let signal_quality = value_signal_quality(modem.get("SignalQuality"));
    let access_tech = value_u32(modem.get("AccessTechnologies"))
        .or_else(|| value_u32(modem.get("CurrentAccessTechnologies")));
    let unlock_required = value_u32(modem.get("UnlockRequired")).unwrap_or(0);
    let sim_path = value_object_path(modem.get("Sim")).filter(|path| path.as_str() != "/");
    let ports = value_string_list(modem.get("Ports"));
    let bearer_paths = value_object_path_list(modem.get("Bearers"));

    let mut apn = None;
    let mut connected = matches!(modem_state, 10 | 11);
    let mut active_bearer = None;

    for bearer_path in bearer_paths {
        let Some(bearer_ifaces) = objects.get(&bearer_path) else {
            continue;
        };
        let Some(bearer) = bearer_ifaces.get(MM_BEARER_IFACE) else {
            continue;
        };

        let bearer_connected = value_bool(bearer.get("Connected")).unwrap_or(false);
        if bearer_connected && active_bearer.is_none() {
            active_bearer = Some(bearer_path.clone());
        }
        connected |= bearer_connected;

        if apn.is_none() {
            apn = bearer
                .get("Properties")
                .and_then(value_dict)
                .and_then(|dict| dict.get("apn").and_then(value_string));
        }
    }

    let (sim_status, pin_status) = sim_path
        .as_ref()
        .and_then(|path| objects.get(path))
        .and_then(|ifaces| ifaces.get(MM_SIM_IFACE))
        .map(|sim| {
            let sim_identifier = sim.get("SimIdentifier").and_then(value_string);
            let present = sim_identifier
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false);
            if present {
                (
                    "SIM detected".to_string(),
                    unlock_required_label(unlock_required).to_string(),
                )
            } else {
                ("SIM unavailable".to_string(), "Unavailable".to_string())
            }
        })
        .unwrap_or_else(|| {
            (
                if unlock_required > 0 {
                    "SIM locked".to_string()
                } else {
                    "SIM unavailable".to_string()
                },
                unlock_required_label(unlock_required).to_string(),
            )
        });

    let operator_name = modem_3gpp.and_then(|props| {
        props
            .get("OperatorName")
            .and_then(value_string)
            .or_else(|| props.get("OperatorCode").and_then(value_string))
    });

    let device_name = modem
        .get("PrimaryPort")
        .and_then(value_string)
        .filter(|value| !value.is_empty())
        .or_else(|| ports.into_iter().next());

    Some(ParsedModem {
        primary_bearer: active_bearer,
        status: MobileDataStatus {
            service_available: true,
            modem_present: true,
            modem_path: Some(modem_path.to_string()),
            device_name,
            state_label: modem_state_label(modem_state).to_string(),
            connected,
            radio_enabled: !matches!(modem_state, 3 | 4),
            signal_quality_percent: signal_quality,
            network_generation: access_tech.map(|value| access_technology_label(value).to_string()),
            operator_name,
            sim_status,
            pin_status,
            apn,
        },
    })
}

fn value_i32(value: Option<&OwnedValue>) -> Option<i32> {
    value.and_then(|value| i32::try_from(value).ok())
}

fn value_u32(value: Option<&OwnedValue>) -> Option<u32> {
    value.and_then(|value| u32::try_from(value).ok())
}

fn value_bool(value: Option<&OwnedValue>) -> Option<bool> {
    value.and_then(|value| bool::try_from(value).ok())
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
        .filter(|value| !value.trim().is_empty())
}

fn value_dict(value: &OwnedValue) -> Option<HashMap<String, OwnedValue>> {
    HashMap::<String, OwnedValue>::try_from(value.try_clone().ok()?).ok()
}

fn value_object_path(value: Option<&OwnedValue>) -> Option<OwnedObjectPath> {
    value.and_then(|value| OwnedObjectPath::try_from(value.try_clone().ok()?).ok())
}

fn value_object_path_list(value: Option<&OwnedValue>) -> Vec<OwnedObjectPath> {
    value
        .and_then(|value| Vec::<OwnedObjectPath>::try_from(value.try_clone().ok()?).ok())
        .unwrap_or_default()
}

fn value_string_list(value: Option<&OwnedValue>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };

    let Some(owned) = value.try_clone().ok() else {
        return Vec::new();
    };

    Vec::<String>::try_from(owned).unwrap_or_default()
}

fn value_signal_quality(value: Option<&OwnedValue>) -> Option<u8> {
    let value = value?;
    if let Ok((quality, _recent)) = <(u32, bool)>::try_from(value.try_clone().ok()?) {
        return u8::try_from(quality).ok();
    }
    if let Ok((quality, _recent)) = <(u8, bool)>::try_from(value.try_clone().ok()?) {
        return Some(quality);
    }
    value_u32(Some(value)).and_then(|quality| u8::try_from(quality).ok())
}

fn modem_state_label(state: i32) -> &'static str {
    match state {
        -1 => "Failed",
        1 => "Initializing",
        2 => "Locked",
        3 | 4 => "Radio off",
        5 => "Enabling",
        6 => "Ready",
        7 => "Searching",
        8 => "Registered",
        9 => "Disconnecting",
        10 => "Connecting",
        11 => "Connected",
        _ => "Unknown",
    }
}

fn access_technology_label(access_tech: u32) -> &'static str {
    if access_tech & 0x0000_8000 != 0 {
        "5G"
    } else if access_tech & 0x0000_4000 != 0 {
        "4G / LTE"
    } else if access_tech & 0x0000_03E0 != 0 {
        "3G"
    } else if access_tech & 0x0000_001E != 0 {
        "2G"
    } else {
        "Unavailable"
    }
}

fn unlock_required_label(code: u32) -> &'static str {
    match code {
        0 => "Unlocked",
        1 => "PIN required",
        2 => "PIN2 required",
        3 => "PUK required",
        4 => "PUK2 required",
        5 => "Device lock",
        6 => "Network lock",
        _ => "Locked",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modem_state_mapping_is_readable() {
        assert_eq!(modem_state_label(11), "Connected");
        assert_eq!(modem_state_label(3), "Radio off");
        assert_eq!(modem_state_label(-1), "Failed");
    }

    #[test]
    fn access_technology_mapping_prefers_latest_generation() {
        assert_eq!(access_technology_label(0x0000_8000), "5G");
        assert_eq!(access_technology_label(0x0000_4000), "4G / LTE");
        assert_eq!(access_technology_label(0x0000_0020), "3G");
        assert_eq!(access_technology_label(0x0000_0008), "2G");
    }

    #[test]
    fn detects_modemmanager_unavailable_errors() {
        assert!(is_modemmanager_unavailable_error(
            "org.freedesktop.ModemManager1: The name org.freedesktop.ModemManager1 was not provided by any .service files"
        ));
        assert!(!is_modemmanager_unavailable_error("permission denied"));
    }
}
