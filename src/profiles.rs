// File: profiles.rs
// Location: /src/profiles.rs

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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

    let profile_zone = profile_name.trim().to_lowercase().replace([' ', '_'], "-");

    let connections = NetworkManager::get_connections().await?;
    for connection in connections
        .into_iter()
        .filter(is_connection_profile_eligible)
    {
        let should_enable = selected_uuids.contains(&connection.uuid);
        nm::set_autoconnect_for_connection_uuid(&connection.uuid, should_enable).await?;

        if should_enable && !profile_zone.is_empty() {
            if let Err(e) =
                nm::set_connection_zone_for_connection_uuid(&connection.uuid, &profile_zone).await
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
    let supported_vpn_uuids: HashSet<String> = nm::list_supported_vpn_connections()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|vpn| vpn.uuid)
        .collect();

    let mut connections: Vec<Connection> = NetworkManager::get_connections()
        .await?
        .into_iter()
        .filter(|connection| {
            is_connection_profile_eligible(connection)
                && (connection.conn_type != "vpn"
                    || supported_vpn_uuids.is_empty()
                    || supported_vpn_uuids.contains(&connection.uuid))
        })
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
        "802-11-wireless" | "wifi" | "802-3-ethernet" | "ethernet" | "wireguard" | "vpn"
    )
}

pub fn replace_connection_uuid_references(
    profiles: &mut [NetworkProfile],
    old_uuid: Uuid,
    new_uuid: Uuid,
) -> bool {
    let mut changed = false;

    for profile in profiles {
        for uuid in &mut profile.connections {
            if *uuid == old_uuid {
                *uuid = new_uuid;
                changed = true;
            }
        }
        profile.connections.sort();
        profile.connections.dedup();
    }

    changed
}

pub fn replace_connection_uuid_in_store(
    path: &Path,
    old_uuid: Uuid,
    new_uuid: Uuid,
) -> Result<bool> {
    let mut profiles = load_profiles(path)?;
    let changed = replace_connection_uuid_references(&mut profiles, old_uuid, new_uuid);
    if changed {
        save_profiles(path, &profiles)?;
    }
    Ok(changed)
}

fn normalize_profiles(profiles: &mut Vec<NetworkProfile>) {
    profiles.retain(|p| !p.name.trim().is_empty());
    profiles.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let mut name_counts = HashMap::new();
    for profile in profiles.iter() {
        *name_counts
            .entry(profile.name.to_lowercase())
            .or_insert(0usize) += 1;
    }

    let mut seen_names = HashSet::new();
    profiles.retain(|p| seen_names.insert(p.name.to_lowercase()));
    for profile in profiles.iter_mut() {
        if name_counts
            .get(&profile.name.to_lowercase())
            .copied()
            .unwrap_or(0)
            > 1
        {
            // * Keep a deterministic Title Case display name for case-insensitive duplicates.
            profile.name = to_title_case_name(&profile.name);
        }
    }

    let mut seen_active = false;
    for profile in profiles.iter_mut() {
        if profile.active && !seen_active {
            seen_active = true;
        } else {
            profile.active = false;
        }
    }
}

fn to_title_case_name(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    let mut chars = lower.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::new();
    out.extend(first.to_uppercase());
    out.push_str(chars.as_str());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_duplicate_name_casing_to_title_case() {
        let mut profiles = vec![
            NetworkProfile {
                name: "HOME".to_string(),
                connections: Vec::new(),
                active: true,
            },
            NetworkProfile {
                name: "home".to_string(),
                connections: Vec::new(),
                active: false,
            },
        ];

        normalize_profiles(&mut profiles);

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "Home");
    }

    #[test]
    fn replaces_connection_uuid_references_across_profiles() {
        let old_uuid = Uuid::new_v4();
        let new_uuid = Uuid::new_v4();
        let mut profiles = vec![NetworkProfile {
            name: "Home".to_string(),
            connections: vec![old_uuid],
            active: false,
        }];

        let changed = replace_connection_uuid_references(&mut profiles, old_uuid, new_uuid);

        assert!(changed);
        assert_eq!(profiles[0].connections, vec![new_uuid]);
    }
}
