use anyhow::{anyhow, Result};
use std::time::Instant;
use std::collections::HashMap;
use tokio::fs;
use tokio::process::Command;
use tokio::time::{sleep, Duration};
use serde::Serialize;
use log::{debug, info, warn};

pub const HOTSPOT_UNSUPPORTED_TOAST: &str = "This Wi-Fi adapter does not support hotspot mode";
const HOTSPOT_NFT_TABLE: &str = "adw_network_hotspot";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotspotAdvancedSupport {
    pub tc_available: bool,
    pub nft_available: bool,
}

impl HotspotAdvancedSupport {
    pub fn available(self) -> bool {
        self.tc_available && self.nft_available
    }

    pub fn missing_reason(self) -> Option<String> {
        if self.available() {
            return None;
        }

        let mut missing = Vec::new();
        if !self.tc_available {
            missing.push("tc");
        }
        if !self.nft_available {
            missing.push("nftables");
        }
        Some(format!(
            "Advanced hotspot controls need: {}",
            missing.join(", ")
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectedClientCountInfo {
    pub count: usize,
    pub estimated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotspotClientDevice {
    pub ip: String,
    pub mac: String,
    pub hostname: Option<String>,
    pub lease_expiry: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct RuntimePolicyPlan {
    tracked_macs: std::collections::BTreeSet<String>,
    blocked_macs: std::collections::BTreeMap<String, String>,
    domain_blocks: std::collections::BTreeMap<String, ResolvedDomainBlock>,
    resolved_client_ips: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default)]
struct ResolvedDomainBlock {
    domains: Vec<String>,
    ipv4: std::collections::BTreeSet<String>,
    ipv6: std::collections::BTreeSet<String>,
}

#[derive(Debug, Clone, Default)]
struct CounterSnapshot {
    upload: HashMap<String, u64>,
    download: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeRulesSignature {
    tracked_macs: Vec<String>,
    blocked_macs: Vec<String>,
    global_upload_limit_kbps: Option<u32>,
    global_download_limit_kbps: Option<u32>,
    max_connected_devices: Option<u32>,
    mac_filter_mode: crate::config::HotspotMacFilterMode,
    resolved_client_ips: Vec<(String, String)>,
    client_rules: Vec<ClientRuleSignature>,
    domain_blocks: Vec<DomainBlockSignature>,
}

#[derive(Debug, Clone, Serialize)]
struct ClientRuleSignature {
    mac_address: String,
    blocked: bool,
    upload_limit_kbps: Option<u32>,
    download_limit_kbps: Option<u32>,
    time_limit_minutes: Option<u32>,
    upload_quota_mb: Option<u64>,
    download_quota_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct DomainBlockSignature {
    mac_address: String,
    domains: Vec<String>,
    ipv4: Vec<String>,
    ipv6: Vec<String>,
}

fn hotspot_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn runtime_state_path() -> std::path::PathBuf {
    crate::hotspot_runtime::hotspot_runtime_state_path()
}

fn load_runtime_state_or_default() -> crate::hotspot_runtime::HotspotRuntimeState {
    crate::hotspot_runtime::load_runtime_state(&runtime_state_path()).unwrap_or_default()
}

fn save_runtime_state_safe(state: &crate::hotspot_runtime::HotspotRuntimeState) {
    if let Err(e) = crate::hotspot_runtime::save_runtime_state(&runtime_state_path(), state) {
        warn!("Failed to save hotspot runtime state: {}", e);
    }
}

pub fn spawn_runtime_daemon() {
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }

    tokio::spawn(async {
        loop {
            if let Err(e) = runtime_tick(false).await {
                warn!("Hotspot runtime sync failed: {}", e);
            }
            sleep(Duration::from_secs(8)).await;
        }
    });
}

pub fn load_temporary_password() -> Option<String> {
    load_runtime_state_or_default().temporary_password
}

pub fn store_temporary_password(password: Option<&str>) {
    let mut state = load_runtime_state_or_default();
    state.temporary_password = password
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    save_runtime_state_safe(&state);
}

pub async fn create_hotspot_on(config: &crate::config::HotspotConfig, iface: &str) -> Result<()> {
    let _guard = hotspot_lock()
        .try_lock()
        .map_err(|_| anyhow!("Hotspot operation already in progress"))?;

    let effective_config = apply_temporary_password_override(config);

    info!(
        "Creating/updating hotspot: SSID={}, Interface={}, Hidden={}",
        effective_config.ssid, iface, effective_config.hidden
    );

    effective_config.validate()?;

    let overall_start = Instant::now();

    // Connect to NetworkManager with timeout
    let start = Instant::now();
    let client = match tokio::time::timeout(Duration::from_secs(5), crate::nm::dbus_client()).await
    {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err(anyhow!("Timed out connecting to NetworkManager")),
    };
    debug!("dbus_client init: {} ms", start.elapsed().as_millis());

    // Ensure device is ready with a timeout to avoid indefinite hangs
    let start = Instant::now();
    match tokio::time::timeout(Duration::from_secs(8), client.ensure_wifi_device_ready(iface)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            let error_text = e.to_string();
            if is_hotspot_mode_not_supported_error(&error_text) {
                return Err(anyhow!(HOTSPOT_UNSUPPORTED_TOAST));
            }
            return Err(anyhow!(
                "Wi-Fi interface {} is not ready for hotspot mode: {}",
                iface,
                error_text
            ));
        }
        Err(_) => {
            return Err(anyhow!(
                "Timed out waiting for Wi-Fi interface {} to become ready",
                iface
            ));
        }
    }
    debug!("ensure_wifi_device_ready: {} ms", start.elapsed().as_millis());

    // List devices and find interface path
    let start = Instant::now();
    let devices = match tokio::time::timeout(Duration::from_secs(3), client.list_devices()).await
    {
        Ok(Ok(list)) => list,
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err(anyhow!("Timed out listing devices")),
    };
    debug!("list_devices: {} ms", start.elapsed().as_millis());

    let iface_path = devices
        .into_iter()
        .find(|d| d.interface == iface)
        .map(|d| d.path)
        .ok_or_else(|| anyhow!("Wi-Fi interface {} not found", iface))?;

    // Deactivate other connections on this interface (best-effort)
    let start = Instant::now();
    let active = match tokio::time::timeout(Duration::from_secs(4), client.list_active_connections()).await {
        Ok(Ok(list)) => list,
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err(anyhow!("Timed out listing active connections")),
    };
    for connection in active {
        if connection.id == "Hotspot" {
            continue;
        }
        if connection.conn_type != "802-11-wireless" && connection.conn_type != "wifi" {
            continue;
        }
        if connection.devices.contains(&iface_path) {
            let _ = tokio::time::timeout(Duration::from_secs(3), client.deactivate_connection_by_uuid(&connection.uuid)).await;
        }
    }
    let _ = tokio::time::timeout(Duration::from_secs(3), client.deactivate_connection_by_id("Hotspot")).await;
    debug!("deactivated conflicting connections: {} ms", start.elapsed().as_millis());

    // Upsert hotspot connection with timeout
    let start = Instant::now();
    match tokio::time::timeout(Duration::from_secs(6), client.upsert_hotspot_connection(&effective_config, iface)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err(anyhow!("Timed out creating/updating hotspot connection")),
    }
    debug!("upsert_hotspot_connection: {} ms", start.elapsed().as_millis());

    // Activate connection if not already active; poll briefly for activation
    let start = Instant::now();
    if !is_hotspot_active().await.unwrap_or(false) {
        match tokio::time::timeout(Duration::from_secs(6), client.activate_connection_by_id("Hotspot", Some(iface))).await {
            Ok(Ok(_active_path)) => {
                // Poll for active state
                let mut waited = 0u64;
                while waited < 8000 {
                    if is_hotspot_active().await.unwrap_or(false) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    waited += 250;
                }
            }
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(anyhow!("Timed out activating hotspot connection")),
        }
    }
    debug!("activate_connection: {} ms", start.elapsed().as_millis());

    // Apply runtime rules (nft/tc) with timeout
    let start = Instant::now();
    match tokio::time::timeout(Duration::from_secs(10), apply_runtime_rules(&effective_config, iface)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(_) => warn!("Timed out applying runtime rules (nft/tc)"),
    }
    debug!("apply_runtime_rules: {} ms", start.elapsed().as_millis());

    debug!("hotspot overall: {} ms", overall_start.elapsed().as_millis());

    Ok(())
}

pub async fn stop_hotspot() -> Result<()> {
    let _guard = hotspot_lock()
        .try_lock()
        .map_err(|_| anyhow!("Hotspot operation already in progress"))?;

    let iface = get_hotspot_interface().await.ok().flatten();
    let client = crate::nm::dbus_client().await?;
    client.deactivate_connection_by_id("Hotspot").await?;
    client.delete_connection_by_id("Hotspot").await?;
    if let Some(iface) = iface.as_deref() {
        cleanup_runtime_rules(iface).await.ok();
    }
    let mut state = load_runtime_state_or_default();
    state.temporary_password = None;
    state.last_applied_signature = None;
    for client in &mut state.clients {
        client.last_connected_at = None;
        client.last_upload_counter_bytes = 0;
        client.last_download_counter_bytes = 0;
        client.blocked_reason = None;
    }
    save_runtime_state_safe(&state);
    Ok(())
}

pub async fn is_hotspot_active() -> Result<bool> {
    // Query NetworkManager via nm_dbus helper to determine active Hotspot connection
    crate::nm::is_hotspot_active().await
}

pub async fn get_wifi_devices() -> Result<Vec<String>> {
    let client = crate::nm::dbus_client().await?;
    let devices = client.get_wifi_devices().await?;
    let names = devices.into_iter().map(|d| d.interface).collect::<Vec<_>>();
    if names.is_empty() {
        return Err(anyhow!(
            "No WiFi devices found. Make sure you have a wireless network adapter."
        ));
    }
    Ok(names)
}

pub async fn get_hotspot_ip() -> Result<Option<String>> {
    crate::nm::get_hotspot_ip().await
}

pub async fn list_connected_clients() -> Result<Vec<HotspotClientDevice>> {
    let mut devices_by_mac: std::collections::BTreeMap<String, HotspotClientDevice> =
        std::collections::BTreeMap::new();
    let hotspot_iface = get_hotspot_interface().await.ok().flatten();
    let lease_entries = crate::leases::load_lease_entries_with_stats().await.entries;
    let lease_by_ip: HashMap<String, crate::leases::LeaseEntry> = lease_entries
        .iter()
        .cloned()
        .map(|entry| (entry.ip.clone(), entry))
        .collect();

    let mut neigh = Command::new("ip");
    neigh.args(["neigh", "show"]);
    if let Some(iface) = hotspot_iface.as_deref() {
        neigh.args(["dev", iface]);
    }

    match neigh.output().await {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let Some((ip, mac)) = parse_neighbor_client(line) else {
                    continue;
                };
                let Some(normalized_mac) = crate::config::normalize_mac_address(mac) else {
                    continue;
                };

                let lease_entry = lease_by_ip.get(ip);
                let hostname = match lease_entry.and_then(|entry| entry.hostname.clone()) {
                    Some(hostname) => Some(hostname),
                    None => reverse_lookup_hostname(ip).await,
                };

                let entry = devices_by_mac
                    .entry(normalized_mac.clone())
                    .or_insert_with(|| HotspotClientDevice {
                        ip: ip.to_string(),
                        mac: normalized_mac.clone(),
                        hostname: hostname.clone(),
                        lease_expiry: lease_entry.and_then(|entry| entry.expiry),
                    });

                if entry.hostname.is_none() && hostname.is_some() {
                    entry.hostname = hostname;
                }
                if entry.ip.contains(':') && !ip.contains(':') {
                    entry.ip = ip.to_string();
                }
                if entry.lease_expiry.is_none() {
                    entry.lease_expiry = lease_entry.and_then(|lease| lease.expiry);
                }
            }
        }
        Ok(output) => {
            warn!(
                "`ip neigh` returned non-zero status while loading hotspot clients: {}",
                output
                    .status
                    .code()
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "signal".to_string())
            );
        }
        Err(e) => {
            warn!("Failed to execute `ip neigh` for hotspot clients: {}", e);
        }
    }

    if devices_by_mac.is_empty() {
        for entry in lease_entries {
            let Some(normalized_mac) = crate::config::normalize_mac_address(&entry.mac) else {
                continue;
            };
            devices_by_mac.insert(
                normalized_mac.clone(),
                HotspotClientDevice {
                    ip: entry.ip,
                    mac: normalized_mac,
                    hostname: entry.hostname,
                    lease_expiry: entry.expiry,
                },
            );
        }
    }

    let mut devices: Vec<HotspotClientDevice> = devices_by_mac.into_values().collect();
    devices.sort_by(|left, right| {
        let left_name = left.hostname.as_deref().unwrap_or(left.ip.as_str());
        let right_name = right.hostname.as_deref().unwrap_or(right.ip.as_str());
        left_name
            .cmp(right_name)
            .then_with(|| left.mac.cmp(&right.mac))
    });
    Ok(devices)
}

pub async fn get_connected_device_count() -> Result<usize> {
    let hotspot_iface = get_hotspot_interface().await.ok().flatten();
    Ok(
        get_connected_device_count_info_for_iface(hotspot_iface.as_deref())
            .await?
            .count,
    )
}

pub async fn get_connected_device_count_info() -> Result<ConnectedClientCountInfo> {
    let hotspot_iface = get_hotspot_interface().await.ok().flatten();
    get_connected_device_count_info_for_iface(hotspot_iface.as_deref()).await
}

pub async fn get_hotspot_interface() -> Result<Option<String>> {
    let client = crate::nm::dbus_client().await?;
    let hotspot = crate::nm::get_active_hotspot_connection().await?;

    let Some(hotspot) = hotspot else {
        return Ok(None);
    };

    let Some(device_path) = hotspot.devices.first() else {
        return Ok(None);
    };

    let interface = client
        .list_devices()
        .await?
        .into_iter()
        .find(|d| d.path == *device_path)
        .map(|d| d.interface);

    Ok(interface)
}

pub async fn advanced_support() -> HotspotAdvancedSupport {
    HotspotAdvancedSupport {
        tc_available: command_available("tc").await,
        nft_available: command_available("nft").await,
    }
}

pub async fn sync_runtime_rules_from_disk() -> Result<()> {
    runtime_tick(false).await
}

// Helper: determine if an error string indicates hotspot mode unsupported by the adapter
pub fn is_hotspot_mode_not_supported_error(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("not supported") || lower.contains("hotspot mode") || lower.contains("operation not supported")
}

async fn get_connected_device_count_info_for_iface(iface: Option<&str>) -> Result<ConnectedClientCountInfo> {
    let neighbor_lines = if let Some(iface) = iface {
        match Command::new("ip").args(["neigh", "show", "dev", iface]).output().await {
            Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(ToString::to_string)
                .collect(),
            _ => Vec::new(),
        }
    } else {
        collect_neighbor_lines().await
    };

    let neighbor_count = neighbor_lines
        .iter()
        .filter_map(|line| parse_neighbor_client(line))
        .count();

    let lease_entries = crate::leases::load_lease_entries_with_stats().await.entries;
    let lease_count = lease_entries.len();

    let neighbor_available = !neighbor_lines.is_empty();
    let lease_available = !lease_entries.is_empty();

    Ok(resolve_connected_client_count(neighbor_count, neighbor_available, lease_count, lease_available))
}

async fn command_available(name: &str) -> bool {
    match Command::new(name).arg("--help").output().await {
        Ok(output) => {
            output.status.success() || !output.stderr.is_empty() || !output.stdout.is_empty()
        }
        Err(_) => false,
    }
}

fn apply_temporary_password_override(config: &crate::config::HotspotConfig) ->
    crate::config::HotspotConfig {
    let mut effective = config.clone();
    if let Some(password) = load_temporary_password() {
        effective.password = password;
    }
    effective
}

async fn runtime_tick(force_apply: bool) -> Result<()> {
    let _guard = match hotspot_lock().try_lock() {
        Ok(guard) => guard,
        Err(_) => return Ok(()),
    };

    let hotspot_active = is_hotspot_active().await.unwrap_or(false);
    let mut state = load_runtime_state_or_default();

    if !hotspot_active {
        let mut changed = false;
        if state.temporary_password.take().is_some() {
            changed = true;
        }
        if state.last_applied_signature.take().is_some() {
            changed = true;
        }
        for client in &mut state.clients {
            changed |= client.last_connected_at.take().is_some();
            if client.last_upload_counter_bytes != 0 {
                client.last_upload_counter_bytes = 0;
                changed = true;
            }
            if client.last_download_counter_bytes != 0 {
                client.last_download_counter_bytes = 0;
                changed = true;
            }
            if client.blocked_reason.take().is_some() {
                changed = true;
            }
        }
        if changed {
            save_runtime_state_safe(&state);
        }
        return Ok(());
    }

    let Some(iface) = get_hotspot_interface().await? else {
        return Ok(());
    };

    let settings =
        crate::config::load_app_settings(&crate::config::app_settings_path()).unwrap_or_default();
    let config = crate::config::load_config(&crate::config::hotspot_config_path())
        .map(|config| apply_temporary_password_override(&config))
        .unwrap_or_else(|_| crate::config::HotspotConfig::default());
    let clients = list_connected_clients().await.unwrap_or_default();

    let mut changed = false;
    changed |= reset_runtime_usage_window_if_needed(&mut state, &settings);
    changed |= update_runtime_activity_state(&mut state, &clients);
    let counters = read_runtime_counters().await;
    changed |= update_runtime_counter_state(&mut state, &counters);

    let plan = build_runtime_policy_plan(&config, &settings, &state, &clients).await;
    changed |= apply_blocked_reasons_to_state(&mut state, &plan.blocked_macs);

    let signature = build_runtime_signature(&config, &plan)?;
    let should_apply = force_apply || state.last_applied_signature.as_deref() != Some(&signature);

    if should_apply {
        apply_runtime_rules_with_plan(&config, &iface, &plan).await?;
        state.last_applied_signature = Some(signature);
        for client in &mut state.clients {
            client.last_upload_counter_bytes = 0;
            client.last_download_counter_bytes = 0;
        }
        changed = true;
    }

    if changed {
        save_runtime_state_safe(&state);
    }

    Ok(())
}

fn reset_runtime_usage_window_if_needed(
    state: &mut crate::hotspot_runtime::HotspotRuntimeState,
    settings: &crate::config::AppSettings,
) -> bool {
    let Some(window_key) = quota_window_key(settings) else {
        return false;
    };
    if state.quota_window_key.as_deref() == Some(window_key.as_str()) {
        return false;
    }

    state.quota_window_key = Some(window_key);
    for client in &mut state.clients {
        client.online_seconds = 0;
        client.upload_bytes = 0;
        client.download_bytes = 0;
        client.last_upload_counter_bytes = 0;
        client.last_download_counter_bytes = 0;
        client.blocked_reason = None;
    }
    true
}

fn quota_window_key(settings: &crate::config::AppSettings) -> Option<String> {
    match settings.hotspot_quota_reset_policy {
        crate::config::HotspotQuotaResetPolicy::Never => None,
        crate::config::HotspotQuotaResetPolicy::DailyMidnight => {
            Some(chrono::Local::now().format("%Y-%m-%d").to_string())
        }
    }
}

fn update_runtime_activity_state(
    state: &mut crate::hotspot_runtime::HotspotRuntimeState,
    clients: &[HotspotClientDevice],
) -> bool {
    let now = chrono::Local::now().timestamp();
    let mut changed = false;
    let active_macs: std::collections::HashSet<String> = clients
        .iter()
        .filter_map(|device| crate::config::normalize_mac_address(&device.mac))
        .collect();

    for device in clients {
        let Some(mac) = crate::config::normalize_mac_address(&device.mac) else {
            continue;
        };

        if state.client_mut(&mac).is_none() {
            state.clients.push(crate::hotspot_runtime::HotspotRuntimeClient {
                mac_address: mac.clone(),
                first_seen_at: now,
                ..crate::hotspot_runtime::HotspotRuntimeClient::default()
            });
            state.normalize();
            changed = true;
        }

        if let Some(client) = state.client_mut(&mac) {
            if client.first_seen_at == 0 {
                client.first_seen_at = now;
                changed = true;
            }
            if client.last_seen_at != now {
                client.last_seen_at = now;
                changed = true;
            }
            if client.ip_address.as_deref() != Some(device.ip.as_str()) {
                client.ip_address = Some(device.ip.clone());
                changed = true;
            }
            let preferred_name = device
                .hostname
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty());
            if preferred_name.is_some() && client.display_name.as_deref() != preferred_name {
                client.display_name = preferred_name.map(ToString::to_string);
                changed = true;
            }

            if let Some(last_connected_at) = client.last_connected_at {
                let delta = now.saturating_sub(last_connected_at) as u64;
                if delta > 0 {
                    client.online_seconds = client.online_seconds.saturating_add(delta);
                    changed = true;
                }
            }
            client.last_connected_at = Some(now);
        }
    }

    for client in &mut state.clients {
        if !active_macs.contains(&client.mac_address) && client.last_connected_at.take().is_some() {
            changed = true;
        }
    }

    changed
}

fn update_runtime_counter_state(
    state: &mut crate::hotspot_runtime::HotspotRuntimeState,
    counters: &CounterSnapshot,
) -> bool {
    let mut changed = false;

    for client in &mut state.clients {
        if let Some(current_upload) = counters.upload.get(&client.mac_address) {
            let delta = if *current_upload >= client.last_upload_counter_bytes {
                current_upload.saturating_sub(client.last_upload_counter_bytes)
            } else {
                *current_upload
            };
            if delta > 0 {
                client.upload_bytes = client.upload_bytes.saturating_add(delta);
                changed = true;
            }
            if client.last_upload_counter_bytes != *current_upload {
                client.last_upload_counter_bytes = *current_upload;
                changed = true;
            }
        }

        if let Some(current_download) = counters.download.get(&client.mac_address) {
            let delta = if *current_download >= client.last_download_counter_bytes {
                current_download.saturating_sub(client.last_download_counter_bytes)
            } else {
                *current_download
            };
            if delta > 0 {
                client.download_bytes = client.download_bytes.saturating_add(delta);
                changed = true;
            }
            if client.last_download_counter_bytes != *current_download {
                client.last_download_counter_bytes = *current_download;
                changed = true;
            }
        }
    }

    changed
}

fn apply_blocked_reasons_to_state(
    state: &mut crate::hotspot_runtime::HotspotRuntimeState,
    blocked_macs: &std::collections::BTreeMap<String, String>,
) -> bool {
    let mut changed = false;
    for client in &mut state.clients {
        let next_reason = blocked_macs.get(&client.mac_address).cloned();
        if client.blocked_reason != next_reason {
            client.blocked_reason = next_reason;
            changed = true;
        }
    }
    changed
}

async fn build_runtime_policy_plan(
    config: &crate::config::HotspotConfig,
    settings: &crate::config::AppSettings,
    state: &crate::hotspot_runtime::HotspotRuntimeState,
    clients: &[HotspotClientDevice],
) -> RuntimePolicyPlan {
    let mut plan = RuntimePolicyPlan::default();
    let connected_macs: Vec<String> = clients
        .iter()
        .filter_map(|device| crate::config::normalize_mac_address(&device.mac))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    for mac in &connected_macs {
        plan.tracked_macs.insert(mac.clone());
    }
    for rule in &config.client_rules {
        if let Some(mac) = crate::config::normalize_mac_address(&rule.mac_address) {
            plan.tracked_macs.insert(mac);
        }
    }

    for rule in &config.client_rules {
        let Some(mac) = crate::config::normalize_mac_address(&rule.mac_address) else {
            continue;
        };

        if rule.blocked {
            plan.blocked_macs
                .insert(mac.clone(), "Blocked manually".to_string());
        }

        if let Some(limit_minutes) = rule.time_limit_minutes {
            if let Some(runtime_client) = state
                .clients
                .iter()
                .find(|client| client.mac_address == mac)
            {
                if runtime_client.online_seconds >= (limit_minutes as u64) * 60 {
                    plan.blocked_macs.insert(
                        mac.clone(),
                        format!("Time limit reached ({})", quota_policy_label(settings)),
                    );
                }
            }
        }

        if let Some(limit_mb) = rule.upload_quota_mb {
            if let Some(runtime_client) = state
                .clients
                .iter()
                .find(|client| client.mac_address == mac)
            {
                if runtime_client.upload_bytes >= megabytes_to_bytes(limit_mb) {
                    plan.blocked_macs.insert(
                        mac.clone(),
                        format!("Upload quota reached ({})", quota_policy_label(settings)),
                    );
                }
            }
        }

        if let Some(limit_mb) = rule.download_quota_mb {
            if let Some(runtime_client) = state
                .clients
                .iter()
                .find(|client| client.mac_address == mac)
            {
                if runtime_client.download_bytes >= megabytes_to_bytes(limit_mb) {
                    plan.blocked_macs.insert(
                        mac.clone(),
                        format!("Download quota reached ({})", quota_policy_label(settings)),
                    );
                }
            }
        }

        if !rule.blocked_domains.is_empty() {
            let resolved = resolve_blocked_domains(&rule.blocked_domains).await;
            if !resolved.domains.is_empty() {
                plan.domain_blocks.insert(mac.clone(), resolved);
            }
        }
    }

    if let Some(limit) = config.max_connected_devices {
        let mut connected_devices: Vec<(String, i64)> = connected_macs
            .iter()
            .map(|mac| {
                let first_seen = state
                    .clients
                    .iter()
                    .find(|client| client.mac_address == *mac)
                    .map(|client| client.first_seen_at)
                    .unwrap_or(i64::MAX);
                (mac.clone(), first_seen)
            })
            .collect();
        connected_devices
            .sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
        for (index, (mac, _)) in connected_devices.into_iter().enumerate() {
            if index >= limit as usize {
                plan.blocked_macs
                    .entry(mac)
                    .or_insert_with(|| "Maximum connected devices reached".to_string());
            }
        }
    }

    plan.resolved_client_ips = resolved_client_ips(config).await.into_iter().collect();
    plan
}

fn quota_policy_label(settings: &crate::config::AppSettings) -> &'static str {
    match settings.hotspot_quota_reset_policy {
        crate::config::HotspotQuotaResetPolicy::Never => "never resets",
        crate::config::HotspotQuotaResetPolicy::DailyMidnight => "resets daily at 00:00",
    }
}

fn megabytes_to_bytes(value: u64) -> u64 {
    value.saturating_mul(1024).saturating_mul(1024)
}

fn build_runtime_signature(config: &crate::config::HotspotConfig, plan: &RuntimePolicyPlan) -> Result<String> {
    let signature = RuntimeRulesSignature {
        tracked_macs: plan.tracked_macs.iter().cloned().collect(),
        blocked_macs: plan.blocked_macs.keys().cloned().collect(),
        global_upload_limit_kbps: config.upload_limit_kbps,
        global_download_limit_kbps: config.download_limit_kbps,
        max_connected_devices: config.max_connected_devices,
        mac_filter_mode: config.mac_filter_mode.clone(),
        resolved_client_ips: plan
            .resolved_client_ips
            .iter()
            .map(|(mac, ip)| (mac.clone(), ip.clone()))
            .collect(),
        client_rules: config
            .client_rules
            .iter()
            .map(|rule| ClientRuleSignature {
                mac_address: rule.mac_address.clone(),
                blocked: rule.blocked,
                upload_limit_kbps: rule.upload_limit_kbps,
                download_limit_kbps: rule.download_limit_kbps,
                time_limit_minutes: rule.time_limit_minutes,
                upload_quota_mb: rule.upload_quota_mb,
                download_quota_mb: rule.download_quota_mb,
            })
            .collect(),
        domain_blocks: plan
            .domain_blocks
            .iter()
            .map(|(mac, block)| DomainBlockSignature {
                mac_address: mac.clone(),
                domains: block.domains.clone(),
                ipv4: block.ipv4.iter().cloned().collect(),
                ipv6: block.ipv6.iter().cloned().collect(),
            })
            .collect(),
    };

    Ok(serde_json::to_string(&signature)?)
}

async fn resolve_blocked_domains(domains: &[String]) -> ResolvedDomainBlock {
    let mut resolved = ResolvedDomainBlock::default();
    for domain in domains {
        let Some(normalized) = crate::config::normalize_blocked_domain(domain) else {
            continue;
        };
        if !resolved.domains.contains(&normalized) {
            resolved.domains.push(normalized.clone());
        }

        let lookup_target = normalized.clone();
        if let Ok(addresses) = tokio::net::lookup_host((lookup_target, 0)).await {
            for address in addresses {
                match address.ip() {
                    std::net::IpAddr::V4(v4) => {
                        resolved.ipv4.insert(v4.to_string());
                    }
                    std::net::IpAddr::V6(v6) => {
                        resolved.ipv6.insert(v6.to_string());
                    }
                }
            }
        }
    }
    resolved.domains.sort();
    resolved
}

async fn read_runtime_counters() -> CounterSnapshot {
    let mut snapshot = CounterSnapshot::default();
    let output = match Command::new("nft")
        .args(["-j", "list", "table", "inet", HOTSPOT_NFT_TABLE])
        .output()
        .await
    {
        Ok(output) if output.status.success() => output,
        _ => return snapshot,
    };

    let value: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(value) => value,
        Err(_) => return snapshot,
    };

    let Some(entries) = value.get("nftables").and_then(serde_json::Value::as_array) else {
        return snapshot;
    };

    for entry in entries {
        let Some(rule) = entry.get("rule") else {
            continue;
        };
        let Some(comment) = rule.get("comment").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(exprs) = rule.get("expr").and_then(serde_json::Value::as_array) else {
            continue;
        };
        let bytes = exprs.iter().find_map(|expr| {
            expr.get("counter")
                .and_then(|counter| counter.get("bytes"))
                .and_then(serde_json::Value::as_u64)
        });
        let Some(bytes) = bytes else {
            continue;
        };

        if let Some(mac) = comment.strip_prefix("adw-quota-up:") {
            snapshot.upload.insert(mac.to_string(), bytes);
        } else if let Some(mac) = comment.strip_prefix("adw-quota-down:") {
            snapshot.download.insert(mac.to_string(), bytes);
        }
    }

    snapshot
}

async fn apply_runtime_rules(config: &crate::config::HotspotConfig, iface: &str) -> Result<()> {
    let settings =
        crate::config::load_app_settings(&crate::config::app_settings_path()).unwrap_or_default();
    let state = load_runtime_state_or_default();
    let clients = list_connected_clients().await.unwrap_or_default();
    let plan = build_runtime_policy_plan(&config, &settings, &state, &clients).await;
    apply_runtime_rules_with_plan(config, iface, &plan).await
}

async fn apply_runtime_rules_with_plan(
    config: &crate::config::HotspotConfig,
    iface: &str,
    plan: &RuntimePolicyPlan,
) -> Result<()> {
    cleanup_runtime_rules(iface).await.ok();

    let support = advanced_support().await;
    let needs_tc = config.upload_limit_kbps.is_some()
        || config.download_limit_kbps.is_some()
        || config
            .client_rules
            .iter()
            .any(|rule| rule.upload_limit_kbps.is_some() || rule.download_limit_kbps.is_some());
    let needs_nft = !matches!(
        config.mac_filter_mode,
        crate::config::HotspotMacFilterMode::Disabled
    ) && !config.client_rules.is_empty()
        || config.max_connected_devices.is_some()
        || !plan.blocked_macs.is_empty()
        || !plan.domain_blocks.is_empty()
        || config.client_rules.iter().any(|rule| {
            rule.time_limit_minutes.is_some()
                || rule.upload_quota_mb.is_some()
                || rule.download_quota_mb.is_some()
        });

    if needs_tc && !support.tc_available {
        return Err(anyhow!("Bandwidth limiting requires the `tc` command"));
    }
    if needs_nft && !support.nft_available {
        return Err(anyhow!(
            "Hotspot policies require nftables (`nft`) for MAC, domain, and quota enforcement"
        ));
    }

    if needs_nft && needs_tc {
        // Run nft and tc setup concurrently to reduce total startup time.
        let (_nft_res, _tc_res) = tokio::try_join!(
            apply_nft_policies(config, iface, plan),
            apply_tc_limits(config, iface, &plan.resolved_client_ips)
        )?;
    } else if needs_nft {
        apply_nft_policies(config, iface, plan).await?;
    } else if needs_tc {
        apply_tc_limits(config, iface, &plan.resolved_client_ips).await?;
    }

    Ok(())
}

async fn cleanup_runtime_rules(iface: &str) -> Result<()> {
    let _ = run_command("tc", &["qdisc", "del", "dev", iface, "root"]).await;
    let _ = run_command("tc", &["qdisc", "del", "dev", iface, "ingress"]).await;
    let _ = run_command("nft", &["delete", "table", "inet", HOTSPOT_NFT_TABLE]).await;
    Ok(())
}

async fn apply_nft_policies(
    config: &crate::config::HotspotConfig,
    iface: &str,
    plan: &RuntimePolicyPlan,
) -> Result<()> {
    let allowlist_macs: Vec<String> = config
        .client_rules
        .iter()
        .filter_map(|rule| crate::config::normalize_mac_address(&rule.mac_address))
        .collect();
    if plan.tracked_macs.is_empty()
        && plan.blocked_macs.is_empty()
        && plan.domain_blocks.is_empty()
        && matches!(
            config.mac_filter_mode,
            crate::config::HotspotMacFilterMode::Disabled
        )
    {
        return Ok(());
    }

    let mut script = String::new();
    script.push_str(&format!("table inet {} {{\n", HOTSPOT_NFT_TABLE));
    script.push_str("  chain forward_filter {\n");
    script.push_str("    type filter hook forward priority 0; policy accept;\n");

    for mac in &plan.tracked_macs {
        script.push_str(&format!(
            "    iifname \"{}\" ether saddr {} counter comment \"adw-quota-up:{}\"\n",
            iface, mac, mac
        ));
        script.push_str(&format!(
            "    oifname \"{}\" ether daddr {} counter comment \"adw-quota-down:{}\"\n",
            iface, mac, mac
        ));
    }

    for mac in plan.blocked_macs.keys() {
        script.push_str(&format!(
            "    iifname \"{}\" ether saddr {} drop\n",
            iface, mac
        ));
        script.push_str(&format!(
            "    oifname \"{}\" ether daddr {} drop\n",
            iface, mac
        ));
    }

    for (mac, block) in &plan.domain_blocks {
        if !block.ipv4.is_empty() {
            script.push_str(&format!(
                "    iifname \"{}\" ether saddr {} ip daddr {{ {} }} drop\n",
                iface,
                mac,
                block.ipv4.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        if !block.ipv6.is_empty() {
            script.push_str(&format!(
                "    iifname \"{}\" ether saddr {} ip6 daddr {{ {} }} drop\n",
                iface,
                mac,
                block.ipv6.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
    }

    match config.mac_filter_mode {
        crate::config::HotspotMacFilterMode::Allowlist => {
            for mac in &allowlist_macs {
                script.push_str(&format!(
                    "    iifname \"{}\" ether saddr {} accept\n",
                    iface, mac
                ));
                script.push_str(&format!(
                    "    oifname \"{}\" ether daddr {} accept\n",
                    iface, mac
                ));
            }
            script.push_str(&format!("    iifname \"{}\" drop\n", iface));
            script.push_str(&format!("    oifname \"{}\" drop\n", iface));
        }
        crate::config::HotspotMacFilterMode::Blocklist => {
            for mac in &allowlist_macs {
                script.push_str(&format!(
                    "    iifname \"{}\" ether saddr {} drop\n",
                    iface, mac
                ));
                script.push_str(&format!(
                    "    oifname \"{}\" ether daddr {} drop\n",
                    iface, mac
                ));
            }
        }
        crate::config::HotspotMacFilterMode::Disabled => {}
    }

    script.push_str("  }\n}\n");

    let temp_path = std::env::temp_dir().join("adw-network-hotspot.nft");
    fs::write(&temp_path, script).await?;
    let result = run_command("nft", &["-f", temp_path.to_string_lossy().as_ref()]).await;
    let _ = fs::remove_file(&temp_path).await;
    result
}

async fn apply_tc_limits(
    config: &crate::config::HotspotConfig,
    iface: &str,
    resolved_clients: &std::collections::BTreeMap<String, String>,
) -> Result<()> {
    if config.download_limit_kbps.is_some()
        || config
            .client_rules
            .iter()
            .any(|rule| rule.download_limit_kbps.is_some())
    {
        let parent_rate = config.download_limit_kbps.unwrap_or(1_000_000);
        run_command(
            "tc",
            &[
                "qdisc", "replace", "dev", iface, "root", "handle", "1:", "htb",
                "default", "999",
            ],
        )
        .await?;
        run_command(
            "tc",
            &[
                "class",
                "replace",
                "dev",
                iface,
                "parent",
                "1:",
                "classid",
                "1:1",
                "htb",
                "rate",
                &format!("{}kbit", parent_rate),
                "ceil",
                &format!("{}kbit", parent_rate),
            ],
        )
        .await?;
        run_command(
            "tc",
            &[
                "class",
                "replace",
                "dev",
                iface,
                "parent",
                "1:1",
                "classid",
                "1:999",
                "htb",
                "rate",
                &format!("{}kbit", parent_rate),
                "ceil",
                &format!("{}kbit", parent_rate),
            ],
        )
        .await?;

        for (index, rule) in config.client_rules.iter().enumerate() {
            let Some(limit) = rule.download_limit_kbps else {
                continue;
            };
            let Some(ip) = resolved_clients.get(&rule.mac_address.to_uppercase()) else {
                continue;
            };
            if !is_ipv4(ip) {
                continue;
            }
            let classid = format!("1:{}", 10 + index);
            run_command(
                "tc",
                &[
                    "class",
                    "replace",
                    "dev",
                    iface,
                    "parent",
                    "1:1",
                    "classid",
                    &classid,
                    "htb",
                    "rate",
                    &format!("{}kbit", limit),
                    "ceil",
                    &format!("{}kbit", limit),
                ],
            )
            .await?;
            run_command(
                "tc",
                &[
                    "filter",
                    "replace",
                    "dev",
                    iface,
                    "protocol",
                    "ip",
                    "parent",
                    "1:0",
                    "prio",
                    &format!("{}", 10 + index),
                    "u32",
                    "match",
                    "ip",
                    "dst",
                    &format!("{}/32", ip),
                    "flowid",
                    &classid,
                ],
            )
            .await?;
        }
    }

    if config.upload_limit_kbps.is_some()
        || config
            .client_rules
            .iter()
            .any(|rule| rule.upload_limit_kbps.is_some())
    {
        run_command("tc", &["qdisc", "replace", "dev", iface, "ingress"]).await?;

        if let Some(limit) = config.upload_limit_kbps {
            run_command(
                "tc",
                &[
                    "filter",
                    "replace",
                    "dev",
                    iface,
                    "parent",
                    "ffff:",
                    "protocol",
                    "ip",
                    "prio",
                    "100",
                    "u32",
                    "match",
                    "u32",
                    "0",
                    "0",
                    "police",
                    "rate",
                    &format!("{}kbit", limit),
                    "burst",
                    "32k",
                    "drop",
                    "flowid",
                    ":1",
                ],
            )
            .await?;
        }

        for (index, rule) in config.client_rules.iter().enumerate() {
            let Some(limit) = rule.upload_limit_kbps else {
                continue;
            };
            let Some(ip) = resolved_clients.get(&rule.mac_address.to_uppercase()) else {
                continue;
            };
            if !is_ipv4(ip) {
                continue;
            }
            run_command(
                "tc",
                &[
                    "filter",
                    "replace",
                    "dev",
                    iface,
                    "parent",
                    "ffff:",
                    "protocol",
                    "ip",
                    "prio",
                    &format!("{}", 10 + index),
                    "u32",
                    "match",
                    "ip",
                    "src",
                    &format!("{}/32", ip),
                    "police",
                    "rate",
                    &format!("{}kbit", limit),
                    "burst",
                    "32k",
                    "drop",
                    "flowid",
                    ":1",
                ],
            )
            .await?;
        }
    }

    Ok(())
}

async fn resolved_client_ips(config: &crate::config::HotspotConfig) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let entries = crate::leases::load_lease_entries_with_stats().await.entries;
    for entry in entries {
        if let Some(mac) = crate::config::normalize_mac_address(&entry.mac) {
            map.insert(mac, entry.ip);
        }
    }

    for line in collect_neighbor_lines().await {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 || !parts.contains(&"lladdr") {
            continue;
        }
        let Some(mac_idx) = parts.iter().position(|value| *value == "lladdr") else {
            continue;
        };
        let Some(ip) = parts.first() else {
            continue;
        };
        let Some(mac) = parts.get(mac_idx + 1) else {
            continue;
        };
        if let Some(mac) = crate::config::normalize_mac_address(mac) {
            map.entry(mac).or_insert_with(|| (*ip).to_string());
        }
    }

    for rule in &config.client_rules {
        if let Some(mac) = crate::config::normalize_mac_address(&rule.mac_address) {
            map.entry(mac).or_default();
        }
    }

    map
}

async fn collect_neighbor_lines() -> Vec<String> {
    match Command::new("ip").args(["neigh", "show"]).output().await {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(ToString::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

async fn reverse_lookup_hostname(ip: &str) -> Option<String> {
    let addr = ip.parse::<std::net::IpAddr>().ok()?;
    let ip_text = ip.to_string();
    tokio::task::spawn_blocking(move || dns_lookup::lookup_addr(&addr).ok())
        .await
        .ok()
        .flatten()
        .map(|name| name.trim().trim_end_matches('.').to_string())
        .filter(|name| !name.is_empty() && *name != ip_text)
}

fn parse_neighbor_client(line: &str) -> Option<(&str, &str)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 5 {
        return None;
    }
    if parts.contains(&"FAILED") || parts.contains(&"INCOMPLETE") || parts.contains(&"NOARP") {
        return None;
    }
    if crate::leases::is_filtered_client_ip(parts[0]) {
        return None;
    }

    let idx = parts.iter().position(|value| *value == "lladdr")?;
    let mac = parts.get(idx + 1)?;
    Some((parts[0], mac))
}

async fn run_command(command: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(command).args(args).output().await?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(anyhow!("{} command failed", command))
    } else {
        Err(anyhow!(stderr))
    }
}

fn is_ipv4(value: &str) -> bool {
    value.parse::<std::net::Ipv4Addr>().is_ok()
}

fn resolve_connected_client_count(
    neighbor_count: usize,
    neighbor_available: bool,
    lease_count: usize,
    lease_available: bool,
) -> ConnectedClientCountInfo {
    let significant_mismatch =
        neighbor_available && lease_available && neighbor_count.abs_diff(lease_count) >= 2;

    if significant_mismatch {
        return ConnectedClientCountInfo {
            count: neighbor_count.max(lease_count),
            estimated: true,
        };
    }

    let count = if neighbor_available {
        neighbor_count
    } else if lease_available {
        lease_count
    } else {
        neighbor_count.max(lease_count)
    };

    ConnectedClientCountInfo {
        count,
        estimated: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_count_without_estimate_for_small_delta() {
        let info = resolve_connected_client_count(3, true, 4, true);
        assert_eq!(info.count, 3);
        assert!(!info.estimated);
    }

    #[test]
    fn resolves_count_as_estimated_for_significant_delta() {
        let info = resolve_connected_client_count(1, true, 5, true);
        assert_eq!(info.count, 5);
        assert!(info.estimated);
    }

    #[test]
    fn detects_hotspot_unsupported_errors() {
        assert!(is_hotspot_mode_not_supported_error(
            "Operation not supported by Wi-Fi adapter"
        ));
        assert!(is_hotspot_mode_not_supported_error(
            "hotspot mode not supported"
        ));
        assert!(!is_hotspot_mode_not_supported_error(
            "Wi-Fi interface is unmanaged"
        ));
    }
}
