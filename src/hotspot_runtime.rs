use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HotspotRuntimeState {
    #[serde(default)]
    pub temporary_password: Option<String>,
    #[serde(default)]
    pub quota_window_key: Option<String>,
    #[serde(default)]
    pub last_applied_signature: Option<String>,
    #[serde(default)]
    pub clients: Vec<HotspotRuntimeClient>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HotspotRuntimeClient {
    pub mac_address: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub ip_address: Option<String>,
    #[serde(default)]
    pub first_seen_at: i64,
    #[serde(default)]
    pub last_seen_at: i64,
    #[serde(default)]
    pub last_connected_at: Option<i64>,
    #[serde(default)]
    pub online_seconds: u64,
    #[serde(default)]
    pub upload_bytes: u64,
    #[serde(default)]
    pub download_bytes: u64,
    #[serde(default)]
    pub last_upload_counter_bytes: u64,
    #[serde(default)]
    pub last_download_counter_bytes: u64,
    #[serde(default)]
    pub blocked_reason: Option<String>,
}

impl HotspotRuntimeState {
    pub fn normalize(&mut self) {
        self.temporary_password = self
            .temporary_password
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);

        self.clients
            .retain(|client| !client.mac_address.trim().is_empty());
        for client in &mut self.clients {
            client.mac_address = crate::config::normalize_mac_address(&client.mac_address)
                .unwrap_or_else(|| client.mac_address.trim().to_uppercase());
            client.display_name = client
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);
            client.ip_address = client
                .ip_address
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);
            client.blocked_reason = client
                .blocked_reason
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);
        }
        self.clients
            .sort_by(|left, right| left.mac_address.cmp(&right.mac_address));
        self.clients
            .dedup_by(|left, right| left.mac_address == right.mac_address);
    }

    pub fn client_mut(&mut self, mac_address: &str) -> Option<&mut HotspotRuntimeClient> {
        self.clients
            .iter_mut()
            .find(|client| client.mac_address == mac_address)
    }
}

pub fn hotspot_runtime_state_path() -> PathBuf {
    std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".local/share/adw-network/hotspot-runtime.json"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/adw-network-hotspot-runtime.json"))
}

pub fn load_runtime_state(path: &std::path::Path) -> Result<HotspotRuntimeState> {
    let content = std::fs::read_to_string(path)?;
    let mut state: HotspotRuntimeState = serde_json::from_str(&content)?;
    state.normalize();
    Ok(state)
}

pub fn save_runtime_state(path: &std::path::Path, state: &HotspotRuntimeState) -> Result<()> {
    let mut state = state.clone();
    state.normalize();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(&state)?;
    std::fs::write(path, json)?;
    Ok(())
}
