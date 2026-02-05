// File: hotspot.rs
// Location: /src/hotspot.rs
//
// Credits & Inspirations:
// - GNOME Settings Network panel for network management patterns

use anyhow::{Result, anyhow};
use tokio::process::Command;
use crate::config::HotspotConfig;
use log::{debug, warn, info, error};
use std::collections::HashSet;

async fn hotspot_connection_exists() -> Result<bool> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "NAME", "connection", "show"])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().any(|l| l.trim() == "Hotspot"))
}

async fn cleanup_shared_connections() -> Result<()> {
    info!("Cleaning up any conflicting shared connections");
    
    // Get all active connections
    let output = Command::new("nmcli")
        .args(["-t", "-f", "NAME,TYPE", "connection", "show", "--active"])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() == 2 {
            let conn_name = parts[0];
            let conn_type = parts[1];
            
            // Skip non-wifi connections and our hotspot
            if !conn_type.contains("wireless") || conn_name == "Hotspot" {
                continue;
            }
            
            // Check if this is a shared connection (AP mode)
            let detail_output = Command::new("nmcli")
                .args(["-t", "-f", "ipv4.method,wifi.mode", "connection", "show", conn_name])
                .output()
                .await;
            
            if let Ok(detail) = detail_output {
                let detail_str = String::from_utf8_lossy(&detail.stdout);
                if detail_str.contains("shared") || detail_str.contains("ap") {
                    info!("Deactivating conflicting shared connection: {}", conn_name);
                    let _ = Command::new("nmcli")
                        .args(["connection", "down", conn_name])
                        .output()
                        .await;
                }
            }
        }
    }
    
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    Ok(())
}

fn band_to_nmcli(band: &str) -> &'static str {
    match band {
        "2.4 GHz" => "bg",
        "5 GHz"   => "a",
        _         => "bg",
    }
}

pub async fn create_hotspot_on(config: &HotspotConfig, iface: &str) -> Result<()> {
    info!("Creating/updating hotspot: SSID={}, Interface={}, Hidden={}", config.ssid, iface, config.hidden);
    
    config.validate()?;
    
    // Check if device exists and is a WiFi device
    check_device_exists(iface).await?;
    
    // Disconnect WiFi client connections on this interface before starting hotspot
    disconnect_wifi_on_interface(iface).await?;
    
    // Clean up any existing shared connections that might conflict
    cleanup_shared_connections().await?;
    
    // Always delete existing connection and create fresh one to avoid configuration issues
    if hotspot_connection_exists().await? {
        info!("Deleting existing hotspot connection to create fresh one");
        let _ = stop_hotspot().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
    
    info!("Creating new hotspot connection");
    add_hotspot(config, iface).await?;
    
    // Check if hotspot is already active (happens with 'dev wifi hotspot' command)
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    
    if is_hotspot_active().await.unwrap_or(false) {
        info!("Hotspot is already active after creation");
        return Ok(());
    }
    
    // If not active, try to activate it
    info!("Hotspot not active, attempting to activate");
    activate_hotspot_fast().await
}

async fn disconnect_wifi_on_interface(iface: &str) -> Result<()> {
    info!("Checking for active WiFi connections on {}", iface);
    
    // Get all active connections on this device
    let output = Command::new("nmcli")
        .args(["-t", "-f", "NAME,DEVICE,TYPE", "connection", "show", "--active"])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut disconnected_any = false;

    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(3, ':').collect();
        if parts.len() >= 3 {
            let conn_name = parts[0];
            let device = parts[1];
            let conn_type = parts[2];
            
            // Skip if not on our interface or not a WiFi connection
            if device != iface || !conn_type.contains("wireless") {
                continue;
            }
            
            // Skip hotspot itself
            if conn_name == "Hotspot" {
                continue;
            }
            
            info!("Disconnecting WiFi connection '{}' on {}", conn_name, iface);
            let _ = Command::new("nmcli")
                .args(["connection", "down", conn_name])
                .output()
                .await;
            
            disconnected_any = true;
        }
    }

    // If we disconnected anything, wait longer for the interface to settle
    if disconnected_any {
        info!("Waiting for interface to settle after disconnection");
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }

    Ok(())
}

async fn check_device_exists(iface: &str) -> Result<()> {
    debug!("Checking if device {} exists", iface);
    
    let output = Command::new("nmcli")
        .args(["-t", "-f", "DEVICE,TYPE", "device", "status"])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() >= 2 && parts[0] == iface {
            let device_type = parts[1].trim();
            debug!("Device {} found with type: {}", iface, device_type);
            
            // Check if it's a WiFi device
            if device_type != "wifi" {
                return Err(anyhow!("Device {} is not a WiFi device (type: {})", iface, device_type));
            }
            
            return Ok(());
        }
    }
    
    Err(anyhow!("WiFi device {} not found", iface))
}

async fn add_hotspot(config: &HotspotConfig, iface: &str) -> Result<()> {
    info!("Adding new hotspot connection on interface {}", iface);
    
    // Try using the simpler 'nmcli dev wifi hotspot' command first
    let mut hotspot_args = vec![
        "dev", "wifi", "hotspot",
        "ifname", iface,
        "con-name", "Hotspot",
        "ssid", &config.ssid,
    ];

    let password_str;
    if !config.password.is_empty() {
        password_str = config.password.clone();
        hotspot_args.push("password");
        hotspot_args.push(&password_str);
    }

    let band_str;
    if config.band != "Auto" {
        band_str = band_to_nmcli(&config.band).to_string();
        hotspot_args.push("band");
        hotspot_args.push(&band_str);
    }

    info!("Trying hotspot command: nmcli {:?}", hotspot_args);
    
    let output = Command::new("nmcli")
        .args(&hotspot_args)
        .output()
        .await?;

    if output.status.success() {
        info!("Hotspot created successfully using dev wifi hotspot command");
        
        // Now modify to set autoconnect=no and hidden if needed
        let _ = Command::new("nmcli")
            .args(["connection", "modify", "Hotspot", "autoconnect", "no"])
            .output()
            .await;

        if config.hidden {
            let _ = Command::new("nmcli")
                .args(["connection", "modify", "Hotspot", "wifi.hidden", "yes"])
                .output()
                .await;
        }

        return Ok(());
    }

    // Log why hotspot command failed
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    warn!("Hotspot command failed - stdout: {}, stderr: {}", stdout, stderr);
    warn!("Falling back to manual connection creation");

    // Fallback to manual connection creation
    let mut args: Vec<String> = vec![
        "connection".into(), "add".into(),
        "type".into(), "wifi".into(),
        "ifname".into(), iface.into(),
        "con-name".into(), "Hotspot".into(),
        "autoconnect".into(), "no".into(),
        "ssid".into(), config.ssid.clone(),
        "mode".into(), "ap".into(),
        "ipv4.method".into(), "shared".into(),
        "ipv4.addresses".into(), "192.168.50.1/24".into(),
        "ipv6.method".into(), "disabled".into(),
    ];

    if !config.password.is_empty() {
        args.push("wifi-sec.key-mgmt".into());
        args.push("wpa-psk".into());
        args.push("wifi-sec.psk".into());
        args.push(config.password.clone());
    }

    if config.band != "Auto" {
        args.push("wifi.band".into());
        args.push(band_to_nmcli(&config.band).into());
    }

    if config.channel != "Auto" {
        args.push("wifi.channel".into());
        args.push(config.channel.clone());
    }

    if config.hidden {
        args.push("wifi.hidden".into());
        args.push("yes".into());
    }

    debug!("nmcli add: {:?}", args);

    let output = Command::new("nmcli").args(&args).output().await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("Failed to add hotspot: {}", stderr);
        return Err(anyhow!("Failed to add hotspot: {}", stderr));
    }

    info!("Hotspot connection added successfully");
    Ok(())
}

async fn activate_hotspot_fast() -> Result<()> {
    info!("Activating hotspot");
    
    let result = Command::new("nmcli")
        .args(["connection", "up", "Hotspot"])
        .output()
        .await?;

    if result.status.success() {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        if is_hotspot_active().await? {
            info!("Hotspot activated successfully");
            return Ok(());
        }
    }

    let stderr = String::from_utf8_lossy(&result.stderr);
    warn!("First activation attempt failed: {}, retrying...", stderr);

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let result = Command::new("nmcli")
        .args(["connection", "up", "Hotspot"])
        .output()
        .await?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        error!("Hotspot activation failed: {}", stderr);
        return Err(anyhow!("Failed to activate hotspot: {}", stderr));
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    if is_hotspot_active().await? {
        info!("Hotspot activated on retry");
        Ok(())
    } else {
        Err(anyhow!("Hotspot activation completed but not active"))
    }
}

pub async fn stop_hotspot() -> Result<()> {
    info!("Stopping hotspot connection");
    
    // First try to deactivate
    let _ = Command::new("nmcli")
        .args(["connection", "down", "Hotspot"])
        .output()
        .await;

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Then delete the connection
    let output = Command::new("nmcli")
        .args(["connection", "delete", "Hotspot"])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("Error: unknown connection") {
            error!("Failed to delete hotspot: {}", stderr);
            return Err(anyhow!("Failed to delete hotspot: {}", stderr));
        }
    }

    info!("Hotspot stopped successfully");
    Ok(())
}

pub async fn is_hotspot_active() -> Result<bool> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "NAME,STATE", "connection", "show", "--active"])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() == 2 && parts[0] == "Hotspot" {
            return Ok(parts[1].trim() == "activated");
        }
    }

    Ok(false)
}

pub async fn get_wifi_devices() -> Result<Vec<String>> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "DEVICE,TYPE", "device", "status"])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() == 2 && parts[1] == "wifi" {
            devices.push(parts[0].to_string());
        }
    }

    if devices.is_empty() {
        return Err(anyhow!("No WiFi devices found. Make sure you have a wireless network adapter."));
    }

    Ok(devices)
}

pub async fn get_hotspot_ip() -> Result<Option<String>> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "IP4.ADDRESS", "connection", "show", "Hotspot"])
        .output()
        .await?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some((_, value)) = line.split_once(':') {
            let value = value.trim();
            if value.is_empty() || value == "--" {
                continue;
            }
            let ip = value.split('/').next().unwrap_or(value);
            return Ok(Some(ip.to_string()));
        }
    }

    Ok(None)
}

pub async fn get_connected_device_count() -> Result<usize> {
    let mut ips: HashSet<String> = HashSet::new();

    if let Ok(output) = Command::new("ip").args(["neigh", "show"]).output().await {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 5 {
                continue;
            }
            let ip = parts[0];
            if ip.ends_with(".1") || ip.ends_with(".0") || ip.starts_with("fe80") {
                continue;
            }
            if parts.iter().any(|p| *p == "lladdr") {
                ips.insert(ip.to_string());
            }
        }
    }

    if ips.is_empty() {
        collect_ips_from_leases(&mut ips).await;
    }

    Ok(ips.len())
}

async fn collect_ips_from_leases(ips: &mut HashSet<String>) {
    let nm_lease_dir = std::path::Path::new("/var/lib/NetworkManager");
    if nm_lease_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(nm_lease_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("dnsmasq-") && name_str.ends_with(".leases") {
                    if let Ok(content) = tokio::fs::read_to_string(entry.path()).await {
                        parse_lease_content(&content, ips);
                    }
                }
            }
        }
    }

    if !ips.is_empty() {
        return;
    }

    let fallback_paths = [
        "/var/lib/dnsmasq/dnsmasq.leases",
        "/var/lib/misc/dnsmasq.leases",
        "/var/db/dnsmasq.leases",
        "/tmp/dnsmasq.leases",
    ];

    for path in &fallback_paths {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            parse_lease_content(&content, ips);
            if !ips.is_empty() {
                break;
            }
        }
    }
}

fn parse_lease_content(content: &str, ips: &mut HashSet<String>) {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        let ip = parts[2];
        if ip.ends_with(".1") || ip.ends_with(".0") || ip.starts_with("fe80") {
            continue;
        }

        ips.insert(ip.to_string());
    }
}
