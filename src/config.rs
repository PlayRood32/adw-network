use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HotspotConfig {
    pub ssid: String,
    pub password: String,
    pub band: String,
    pub channel: String,
    pub hidden: bool,
    #[serde(default)]
    pub upload_limit_kbps: Option<u32>,
    #[serde(default)]
    pub download_limit_kbps: Option<u32>,
    #[serde(default)]
    pub max_connected_devices: Option<u32>,
    #[serde(default)]
    pub mac_filter_mode: HotspotMacFilterMode,
    #[serde(default)]
    pub client_rules: Vec<HotspotClientRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSettings {
    pub color_scheme: String,
    #[serde(default = "default_auto_scan")]
    pub auto_scan: bool,
    #[serde(default = "default_expand_connected_details")]
    pub expand_connected_details: bool,
    #[serde(default = "default_icons_only_navigation")]
    pub icons_only_navigation: bool,
    #[serde(default = "default_hotspot_password_storage")]
    pub hotspot_password_storage: HotspotPasswordStorage,
    #[serde(default = "default_hotspot_quota_reset_policy")]
    pub hotspot_quota_reset_policy: HotspotQuotaResetPolicy,
    #[serde(default = "default_plain_json_debug_opt_in")]
    pub plain_json_debug_opt_in: bool,
    #[serde(default = "default_module_layout_customized")]
    pub module_layout_customized: bool,
    #[serde(default = "default_show_wifi_module")]
    pub show_wifi_module: bool,
    #[serde(default = "default_show_ethernet_module")]
    pub show_ethernet_module: bool,
    #[serde(default = "default_show_hotspot_module")]
    pub show_hotspot_module: bool,
    #[serde(default = "default_show_devices_module")]
    pub show_devices_module: bool,
    #[serde(default = "default_show_profiles_module")]
    pub show_profiles_module: bool,
    #[serde(default = "default_module_order")]
    pub module_order: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum HotspotMacFilterMode {
    #[default]
    Disabled,
    Allowlist,
    Blocklist,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum HotspotQuotaResetPolicy {
    #[default]
    Never,
    DailyMidnight,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HotspotClientRule {
    pub mac_address: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub blocked: bool,
    #[serde(default)]
    pub upload_limit_kbps: Option<u32>,
    #[serde(default)]
    pub download_limit_kbps: Option<u32>,
    #[serde(default)]
    pub time_limit_minutes: Option<u32>,
    #[serde(default)]
    pub upload_quota_mb: Option<u64>,
    #[serde(default)]
    pub download_quota_mb: Option<u64>,
    #[serde(default)]
    pub blocked_domains: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            color_scheme: "system".to_string(),
            auto_scan: true,
            expand_connected_details: false,
            icons_only_navigation: true,
            hotspot_password_storage: HotspotPasswordStorage::Keyring,
            hotspot_quota_reset_policy: HotspotQuotaResetPolicy::Never,
            plain_json_debug_opt_in: false,
            module_layout_customized: false,
            show_wifi_module: true,
            show_ethernet_module: true,
            show_hotspot_module: false,
            show_devices_module: false,
            show_profiles_module: true,
            module_order: default_module_order(),
        }
    }
}

impl AppSettings {
    pub fn validate(&self) -> Result<()> {
        match self.color_scheme.as_str() {
            "system" | "light" | "dark" => {}
            _ => anyhow::bail!("Invalid color scheme"),
        }

        if !self.any_module_visible() {
            anyhow::bail!("At least one top navigation module must stay visible");
        }

        Ok(())
    }

    pub fn any_module_visible(&self) -> bool {
        self.show_wifi_module
            || self.show_ethernet_module
            || self.show_hotspot_module
            || self.show_devices_module
            || self.show_profiles_module
    }

    pub fn normalize_module_layout(&mut self) -> bool {
        let mut changed = false;
        let default_order = default_module_order();
        let valid_names: HashSet<&str> = default_order.iter().map(String::as_str).collect();
        let mut normalized_order = Vec::new();
        let mut seen = HashSet::new();

        for item in &self.module_order {
            if valid_names.contains(item.as_str()) && seen.insert(item.clone()) {
                normalized_order.push(item.clone());
            }
        }
        for item in &default_order {
            if seen.insert(item.clone()) {
                normalized_order.push(item.clone());
            }
        }
        if normalized_order != self.module_order {
            self.module_order = normalized_order;
            changed = true;
        }

        if !self.any_module_visible() {
            let defaults = AppSettings::default();
            self.module_layout_customized = false;
            self.show_wifi_module = defaults.show_wifi_module;
            self.show_ethernet_module = defaults.show_ethernet_module;
            self.show_hotspot_module = defaults.show_hotspot_module;
            self.show_devices_module = defaults.show_devices_module;
            self.show_profiles_module = defaults.show_profiles_module;
            self.module_order = defaults.module_order;
            changed = true;
        }

        changed
    }
}

pub fn plain_json_warning_active(settings: &AppSettings) -> bool {
    // * Keep the plain-JSON warning logic in one place for load and change flows.
    settings.hotspot_password_storage == HotspotPasswordStorage::PlainJson
        && settings.plain_json_debug_opt_in
}

fn default_auto_scan() -> bool {
    true
}

fn default_expand_connected_details() -> bool {
    false
}

fn default_icons_only_navigation() -> bool {
    true
}

fn default_plain_json_debug_opt_in() -> bool {
    false
}

fn default_module_layout_customized() -> bool {
    false
}

fn default_show_wifi_module() -> bool {
    true
}

fn default_show_ethernet_module() -> bool {
    true
}

fn default_show_hotspot_module() -> bool {
    false
}

fn default_show_devices_module() -> bool {
    false
}

fn default_show_profiles_module() -> bool {
    true
}

fn default_module_order() -> Vec<String> {
    vec![
        "Wi-Fi".to_string(),
        "Ethernet".to_string(),
        "Hotspot".to_string(),
        "Devices".to_string(),
        "Profiles".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HotspotPasswordStorage {
    Keyring,
    NetworkManager,
    PlainJson,
}

fn default_hotspot_password_storage() -> HotspotPasswordStorage {
    HotspotPasswordStorage::Keyring
}

fn default_hotspot_quota_reset_policy() -> HotspotQuotaResetPolicy {
    HotspotQuotaResetPolicy::Never
}

// * Theme selection is handled through app settings only.

impl Default for HotspotConfig {
    fn default() -> Self {
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "Hotspot".to_string());

        Self {
            ssid: hostname,
            password: String::new(),
            band: "Auto".to_string(),
            channel: "Auto".to_string(),
            hidden: false,
            upload_limit_kbps: None,
            download_limit_kbps: None,
            max_connected_devices: None,
            mac_filter_mode: HotspotMacFilterMode::Disabled,
            client_rules: Vec::new(),
        }
    }
}

impl HotspotConfig {
    fn validate_limit(limit: Option<u32>, label: &str) -> Result<()> {
        if matches!(limit, Some(0)) {
            anyhow::bail!("{} must be greater than 0", label);
        }
        Ok(())
    }

    fn validate_quota(limit: Option<u64>, label: &str) -> Result<()> {
        if matches!(limit, Some(0)) {
            anyhow::bail!("{} must be greater than 0", label);
        }
        Ok(())
    }

    pub fn normalize(&mut self) {
        self.client_rules
            .retain(|rule| !rule.mac_address.trim().is_empty());
        for rule in &mut self.client_rules {
            rule.mac_address = normalize_mac_address(&rule.mac_address)
                .unwrap_or_else(|| rule.mac_address.trim().to_uppercase());
            rule.display_name = rule
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);
            let mut blocked_domains = Vec::new();
            for domain in &rule.blocked_domains {
                if let Some(normalized) = normalize_blocked_domain(domain) {
                    if !blocked_domains.contains(&normalized) {
                        blocked_domains.push(normalized);
                    }
                }
            }
            blocked_domains.sort();
            rule.blocked_domains = blocked_domains;
        }
        self.client_rules
            .sort_by(|a, b| a.mac_address.cmp(&b.mac_address));
        self.client_rules
            .dedup_by(|a, b| a.mac_address == b.mac_address);
    }

    pub fn validate_ssid(&self) -> Result<()> {
        if self.ssid.is_empty() || self.ssid.len() > 32 {
            anyhow::bail!("SSID must be 1-32 characters");
        }

        if !self
            .ssid
            .chars()
            .all(|c| c.is_ascii() && !c.is_ascii_control())
        {
            anyhow::bail!("SSID contains invalid characters");
        }

        Ok(())
    }

    pub fn validate_password(&self) -> Result<()> {
        if !self.password.is_empty() && (self.password.len() < 8 || self.password.len() > 63) {
            anyhow::bail!("Password must be 8-63 characters or empty for open network");
        }

        if !self
            .password
            .chars()
            .all(|c| c.is_ascii() && !c.is_ascii_control())
        {
            anyhow::bail!("Password contains invalid characters");
        }

        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        self.validate_ssid()?;
        self.validate_password()?;
        Self::validate_limit(self.upload_limit_kbps, "Upload limit")?;
        Self::validate_limit(self.download_limit_kbps, "Download limit")?;
        Self::validate_limit(self.max_connected_devices, "Device limit")?;

        for rule in &self.client_rules {
            if normalize_mac_address(&rule.mac_address).is_none() {
                anyhow::bail!("Invalid MAC address: {}", rule.mac_address);
            }
            Self::validate_limit(rule.upload_limit_kbps, "Per-device upload limit")?;
            Self::validate_limit(rule.download_limit_kbps, "Per-device download limit")?;
            Self::validate_limit(rule.time_limit_minutes, "Per-device time limit")?;
            Self::validate_quota(rule.upload_quota_mb, "Per-device upload quota")?;
            Self::validate_quota(rule.download_quota_mb, "Per-device download quota")?;
            for domain in &rule.blocked_domains {
                if normalize_blocked_domain(domain).is_none() {
                    anyhow::bail!("Invalid blocked domain: {}", domain);
                }
            }
        }
        Ok(())
    }
}

pub fn normalize_blocked_domain(value: &str) -> Option<String> {
    let mut normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    if let Some(stripped) = normalized.strip_prefix("https://") {
        normalized = stripped.to_string();
    } else if let Some(stripped) = normalized.strip_prefix("http://") {
        normalized = stripped.to_string();
    }

    if let Some((host, _)) = normalized.split_once('/') {
        normalized = host.to_string();
    }
    if let Some((host, _)) = normalized.split_once('?') {
        normalized = host.to_string();
    }
    if let Some((host, _)) = normalized.split_once('#') {
        normalized = host.to_string();
    }

    normalized = normalized.trim_matches('.').to_string();
    if let Some(stripped) = normalized.strip_prefix("www.") {
        normalized = stripped.to_string();
    }

    if normalized.is_empty() {
        return None;
    }
    if !normalized
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
    {
        return None;
    }
    if normalized.starts_with('-')
        || normalized.ends_with('-')
        || normalized.contains("..")
        || !normalized.contains('.')
    {
        return None;
    }

    Some(normalized)
}

pub fn load_config(path: &std::path::Path) -> Result<HotspotConfig> {
    let content = std::fs::read_to_string(path)?;
    let mut config: HotspotConfig = serde_json::from_str(&content)?;
    config.normalize();
    config.validate()?;
    Ok(config)
}

pub fn save_config(path: &std::path::Path, config: &HotspotConfig) -> Result<()> {
    let mut config = config.clone();
    config.normalize();
    config.validate()?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, json)?;

    Ok(())
}

pub fn hotspot_config_path() -> PathBuf {
    std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".config/adw-network/hotspot.json"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/adw-network-hotspot.json"))
}

pub fn load_app_settings_with_status(path: &std::path::Path) -> Result<(AppSettings, bool)> {
    let content = std::fs::read_to_string(path)?;
    let mut settings: AppSettings = serde_json::from_str(&content)?;
    let mut changed = false;
    // * Legacy plain-json storage now requires an explicit debug opt-in after upgrade.
    if settings.hotspot_password_storage == HotspotPasswordStorage::PlainJson
        && !settings.plain_json_debug_opt_in
    {
        settings.hotspot_password_storage = HotspotPasswordStorage::Keyring;
        changed = true;
    }
    changed |= settings.normalize_module_layout();
    settings.validate()?;
    Ok((settings, changed))
}

pub fn load_app_settings(path: &std::path::Path) -> Result<AppSettings> {
    Ok(load_app_settings_with_status(path)?.0)
}

pub fn save_app_settings(path: &std::path::Path, settings: &AppSettings) -> Result<()> {
    let mut settings = settings.clone();
    settings.normalize_module_layout();
    settings.validate()?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(&settings)?;
    std::fs::write(path, json)?;

    Ok(())
}

pub fn normalize_mac_address(value: &str) -> Option<String> {
    let hex: String = value.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 12 {
        return None;
    }

    let mut normalized = String::with_capacity(17);
    for (idx, chunk) in hex.as_bytes().chunks(2).enumerate() {
        if idx > 0 {
            normalized.push(':');
        }
        normalized.push((chunk[0] as char).to_ascii_uppercase());
        normalized.push((chunk[1] as char).to_ascii_uppercase());
    }

    Some(normalized)
}

pub fn app_settings_path() -> PathBuf {
    std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".config/adw-network/settings.json"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/adw-network-settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_valid_config() {
        let config = HotspotConfig {
            ssid: "TestNetwork".to_string(),
            password: "password123".to_string(),
            band: "2.4 GHz".to_string(),
            channel: "Auto".to_string(),
            hidden: false,
            ..HotspotConfig::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_ssid_too_long() {
        let config = HotspotConfig {
            ssid: "a".repeat(33),
            password: "password123".to_string(),
            band: "2.4 GHz".to_string(),
            channel: "Auto".to_string(),
            hidden: false,
            ..HotspotConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_password_too_short() {
        let config = HotspotConfig {
            ssid: "TestNetwork".to_string(),
            password: "short".to_string(),
            band: "2.4 GHz".to_string(),
            channel: "Auto".to_string(),
            hidden: false,
            ..HotspotConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_open_network() {
        let config = HotspotConfig {
            ssid: "OpenNetwork".to_string(),
            password: String::new(),
            band: "2.4 GHz".to_string(),
            channel: "Auto".to_string(),
            hidden: false,
            ..HotspotConfig::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_plain_json_migrates_to_keyring_without_debug_opt_in() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        let content = r#"{
  "color_scheme": "system",
  "auto_scan": true,
  "expand_connected_details": false,
  "icons_only_navigation": true,
  "hotspot_password_storage": "plain-json"
}"#;
        std::fs::write(&path, content).expect("write settings");

        let settings = load_app_settings(&path).expect("load settings");
        assert_eq!(
            settings.hotspot_password_storage,
            HotspotPasswordStorage::Keyring
        );
        assert!(!settings.plain_json_debug_opt_in);
    }

    #[test]
    fn test_plain_json_warning_active_helper() {
        let mut settings = AppSettings {
            hotspot_password_storage: HotspotPasswordStorage::PlainJson,
            plain_json_debug_opt_in: true,
            ..AppSettings::default()
        };
        assert!(plain_json_warning_active(&settings));

        settings.plain_json_debug_opt_in = false;
        assert!(!plain_json_warning_active(&settings));
    }

    #[test]
    fn normalizes_zero_visible_modules_back_to_safe_defaults() {
        let mut settings = AppSettings {
            show_wifi_module: false,
            show_ethernet_module: false,
            show_hotspot_module: false,
            show_devices_module: false,
            show_profiles_module: false,
            module_layout_customized: true,
            ..AppSettings::default()
        };

        let changed = settings.normalize_module_layout();

        assert!(changed);
        assert!(settings.show_wifi_module);
        assert!(settings.show_ethernet_module);
        assert!(settings.show_profiles_module);
        assert!(!settings.show_hotspot_module);
        assert!(!settings.show_devices_module);
        assert!(!settings.module_layout_customized);
        assert_eq!(settings.module_order, default_module_order());
    }

    #[test]
    fn normalizes_module_order_duplicates_and_unknown_entries() {
        let mut settings = AppSettings {
            module_order: vec![
                "Profiles".to_string(),
                "Wi-Fi".to_string(),
                "Profiles".to_string(),
                "Unknown".to_string(),
            ],
            ..AppSettings::default()
        };

        let changed = settings.normalize_module_layout();

        assert!(changed);
        assert_eq!(
            settings.module_order,
            vec![
                "Profiles".to_string(),
                "Wi-Fi".to_string(),
                "Ethernet".to_string(),
                "Hotspot".to_string(),
                "Devices".to_string(),
            ]
        );
    }

    #[test]
    fn normalizes_mac_addresses_to_uppercase_pairs() {
        assert_eq!(
            normalize_mac_address("aabbccddeeff").as_deref(),
            Some("AA:BB:CC:DD:EE:FF")
        );
        assert_eq!(
            normalize_mac_address("aa-bb-cc-dd-ee-ff").as_deref(),
            Some("AA:BB:CC:DD:EE:FF")
        );
        assert!(normalize_mac_address("invalid").is_none());
    }
}
