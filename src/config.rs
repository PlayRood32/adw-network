// File: config.rs
// Location: /src/config.rs

use serde::{Deserialize, Serialize};
use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotspotConfig {
    pub ssid: String,
    pub password: String,
    pub band: String,
    pub channel: String,
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub color_scheme: String,
    #[serde(default = "default_auto_scan")]
    pub auto_scan: bool,
    #[serde(default = "default_expand_connected_details")]
    pub expand_connected_details: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            color_scheme: "system".to_string(),
            auto_scan: true,
            expand_connected_details: false,
        }
    }
}

impl AppSettings {
    pub fn validate(&self) -> Result<()> {
        match self.color_scheme.as_str() {
            "system" | "light" | "dark" => Ok(()),
            _ => anyhow::bail!("Invalid color scheme"),
        }
    }
}

fn default_auto_scan() -> bool {
    true
}

fn default_expand_connected_details() -> bool {
    false
}

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
        }
    }
}

impl HotspotConfig {
    pub fn validate_ssid(&self) -> Result<()> {
        if self.ssid.is_empty() || self.ssid.len() > 32 {
            anyhow::bail!("SSID must be 1-32 characters");
        }
        
        if !self.ssid.chars().all(|c| c.is_ascii() && !c.is_ascii_control()) {
            anyhow::bail!("SSID contains invalid characters");
        }
        
        Ok(())
    }
    
    pub fn validate_password(&self) -> Result<()> {
        if !self.password.is_empty() && (self.password.len() < 8 || self.password.len() > 63) {
            anyhow::bail!("Password must be 8-63 characters or empty for open network");
        }
        
        if !self.password.chars().all(|c| c.is_ascii() && !c.is_ascii_control()) {
            anyhow::bail!("Password contains invalid characters");
        }
        
        Ok(())
    }
    
    pub fn validate(&self) -> Result<()> {
        self.validate_ssid()?;
        self.validate_password()?;
        Ok(())
    }
}

pub fn load_config(path: &std::path::Path) -> Result<HotspotConfig> {
    let content = std::fs::read_to_string(path)?;
    let config: HotspotConfig = serde_json::from_str(&content)?;
    config.validate()?;
    Ok(config)
}

pub fn save_config(path: &std::path::Path, config: &HotspotConfig) -> Result<()> {
    config.validate()?;
    
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let json = serde_json::to_string_pretty(config)?;
    std::fs::write(path, json)?;
    
    Ok(())
}

pub fn hotspot_config_path() -> PathBuf {
    std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".config/adw-network/hotspot.json"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/adw-network-hotspot.json"))
}

pub fn load_app_settings(path: &std::path::Path) -> Result<AppSettings> {
    let content = std::fs::read_to_string(path)?;
    let settings: AppSettings = serde_json::from_str(&content)?;
    settings.validate()?;
    Ok(settings)
}

pub fn save_app_settings(path: &std::path::Path, settings: &AppSettings) -> Result<()> {
    settings.validate()?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(settings)?;
    std::fs::write(path, json)?;

    Ok(())
}

pub fn app_settings_path() -> PathBuf {
    std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".config/adw-network/settings.json"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/adw-network-settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_config() {
        let config = HotspotConfig {
            ssid: "TestNetwork".to_string(),
            password: "password123".to_string(),
            band: "2.4 GHz".to_string(),
            channel: "Auto".to_string(),
            hidden: false,
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
        };
        assert!(config.validate().is_ok());
    }
}
