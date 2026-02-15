// File: profiles.rs
// Location: /src/profiles.rs

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::nm::{self, Connection, NetworkManager};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkProfile {
    pub name: String,
    #[serde(default)]
    pub connections: Vec<Uuid>,
    #[serde(default)]
    pub active: bool,
}

pub fn profiles_path() -> PathBuf {
    std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".config/adw-network/profiles.json"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/adw-network-profiles.json"))
}

pub fn load_profiles(path: &Path) -> Result<Vec<NetworkProfile>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(path)?;
    let mut profiles: Vec<NetworkProfile> = serde_json::from_str(&content)?;
    normalize_profiles(&mut profiles);
    Ok(profiles)
}

pub fn save_profiles(path: &Path, profiles: &[NetworkProfile]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut to_save = profiles.to_vec();
    normalize_profiles(&mut to_save);
    let json = serde_json::to_string_pretty(&to_save)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub async fn activate_profile_by_name(path: &Path, profile_name: &str) -> Result<()> {
    let mut profiles = load_profiles(path)?;
    apply_profile_selection(&mut profiles, profile_name).await?;
    save_profiles(path, &profiles)
}

pub async fn apply_profile_selection(
    profiles: &mut [NetworkProfile],
    profile_name: &str,
) -> Result<()> {
    let target_idx = profiles
        .iter()
        .position(|p| p.name == profile_name)
        .ok_or_else(|| anyhow!("Profile not found: {}", profile_name))?;

    let selected_uuids: HashSet<String> = profiles[target_idx]
        .connections
        .iter()
        .map(Uuid::to_string)
        .collect();

    let profile_zone = profile_name
        .trim()
        .to_lowercase()
        .replace(' ', "-")
        .replace('_', "-");

    let connections = NetworkManager::get_connections().await?;
    for connection in connections.into_iter().filter(is_connection_profile_eligible) {
        let should_enable = selected_uuids.contains(&connection.uuid);
        nm::set_autoconnect_for_connection_uuid(&connection.uuid, should_enable).await?;

        if should_enable && !profile_zone.is_empty() {
            if let Err(e) = nm::set_connection_zone_for_connection_uuid(&connection.uuid, &profile_zone).await
            {
                log::warn!(
                    "Failed to set connection zone for {} ({}): {}",
                    connection.name,
                    connection.uuid,
                    e
                );
            }
        }
    }

    for (idx, profile) in profiles.iter_mut().enumerate() {
        profile.active = idx == target_idx;
    }

    Ok(())
}

pub async fn get_profile_eligible_connections() -> Result<Vec<Connection>> {
    let mut connections: Vec<Connection> = NetworkManager::get_connections()
        .await?
        .into_iter()
        .filter(is_connection_profile_eligible)
        .collect();

    connections.sort_by(|a, b| {
        if a.active && !b.active {
            std::cmp::Ordering::Less
        } else if !a.active && b.active {
            std::cmp::Ordering::Greater
        } else {
            a.name.cmp(&b.name)
        }
    });

    Ok(connections)
}

pub fn parse_uuid(value: &str) -> Result<Uuid> {
    Uuid::parse_str(value).with_context(|| format!("Invalid UUID: {}", value))
}

fn is_connection_profile_eligible(connection: &Connection) -> bool {
    matches!(
        connection.conn_type.as_str(),
        "802-11-wireless" | "wifi" | "802-3-ethernet" | "ethernet"
    )
}

fn normalize_profiles(profiles: &mut Vec<NetworkProfile>) {
    profiles.retain(|p| !p.name.trim().is_empty());
    profiles.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let mut seen_names = HashSet::new();
    profiles.retain(|p| seen_names.insert(p.name.to_lowercase()));

    let mut seen_active = false;
    for profile in profiles.iter_mut() {
        if profile.active && !seen_active {
            seen_active = true;
        } else {
            profile.active = false;
        }
    }
}
