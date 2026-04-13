use crate::config::AppSettings;
use crate::nm::{Connection, WifiNetwork};
use crate::profiles::NetworkProfile;
use gtk4::glib;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Debug, Clone)]
pub struct PrefsState {
    pub auto_scan: bool,
    pub expand_connected_details: bool,
    pub icons_only_navigation: bool,
}

impl From<&AppSettings> for PrefsState {
    fn from(value: &AppSettings) -> Self {
        Self {
            auto_scan: value.auto_scan,
            expand_connected_details: value.expand_connected_details,
            icons_only_navigation: value.icons_only_navigation,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PageKind {
    Wifi,
    Hotspot,
    Devices,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WifiFilterState {
    #[default]
    All,
    Band24,
    Band5,
    Saved,
}

#[derive(Debug, Default)]
struct VisibilityState {
    wifi_visible: AtomicBool,
    hotspot_visible: AtomicBool,
    devices_visible: AtomicBool,
}

#[derive(Debug)]
struct WifiSharedState {
    busy_count: AtomicU32,
    search_text: RwLock<String>,
    all_networks: RwLock<Vec<WifiNetwork>>,
    saved_ssids: RwLock<HashSet<String>>,
    filter_state: RwLock<WifiFilterState>,
    connected_network: RwLock<Option<WifiNetwork>>,
    refresh_source: RwLock<Option<glib::SourceId>>,
    search_debounce_source: RwLock<Option<glib::SourceId>>,
}

impl Default for WifiSharedState {
    fn default() -> Self {
        Self {
            busy_count: AtomicU32::new(0),
            search_text: RwLock::new(String::new()),
            all_networks: RwLock::new(Vec::new()),
            saved_ssids: RwLock::new(HashSet::new()),
            filter_state: RwLock::new(WifiFilterState::All),
            connected_network: RwLock::new(None),
            refresh_source: RwLock::new(None),
            search_debounce_source: RwLock::new(None),
        }
    }
}

#[derive(Debug, Default)]
#[allow(dead_code)]
struct HotspotSharedState {
    devices: RwLock<Vec<String>>,
    is_active: AtomicBool,
    wifi_present: AtomicBool,
    wifi_enabled: AtomicBool,
    operation_in_progress: AtomicBool,
    status_refresh_source: RwLock<Option<glib::SourceId>>,
    interface_refresh_source: RwLock<Option<glib::SourceId>>,
}

#[derive(Debug, Default)]
struct DevicesSharedState {
    auto_refresh_active: AtomicBool,
    refresh_in_flight: AtomicBool,
    refresh_source: RwLock<Option<glib::SourceId>>,
}

#[derive(Debug, Default)]
#[allow(dead_code)]
struct EthernetSharedState {
    connections: RwLock<Vec<Connection>>,
    connected_connection: RwLock<Option<Connection>>,
    ethernet_devices: RwLock<Vec<String>>,
}

#[derive(Debug, Default)]
#[allow(dead_code)]
struct ProfilesSharedState {
    profiles: RwLock<Vec<NetworkProfile>>,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct AppState {
    prefs: Arc<RwLock<PrefsState>>,
    visibility: Arc<VisibilityState>,
    wifi: Arc<WifiSharedState>,
    hotspot: Arc<HotspotSharedState>,
    devices: Arc<DevicesSharedState>,
    ethernet: Arc<EthernetSharedState>,
    profiles: Arc<ProfilesSharedState>,
}

#[allow(dead_code)]
impl AppState {
    pub fn new(settings: &AppSettings) -> Self {
        Self {
            prefs: Arc::new(RwLock::new(PrefsState::from(settings))),
            visibility: Arc::new(VisibilityState::default()),
            wifi: Arc::new(WifiSharedState::default()),
            hotspot: Arc::new(HotspotSharedState::default()),
            devices: Arc::new(DevicesSharedState::default()),
            ethernet: Arc::new(EthernetSharedState::default()),
            profiles: Arc::new(ProfilesSharedState::default()),
        }
    }

    fn read_guard<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
        match lock.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn write_guard<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
        match lock.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub fn update_prefs<F>(&self, f: F)
    where
        F: FnOnce(&mut PrefsState),
    {
        let mut prefs = Self::write_guard(&self.prefs);
        f(&mut prefs);
    }

    pub fn prefs_state(&self) -> PrefsState {
        Self::read_guard(&self.prefs).clone()
    }

    pub fn auto_scan_enabled(&self) -> bool {
        Self::read_guard(&self.prefs).auto_scan
    }

    pub fn expand_connected_details(&self) -> bool {
        Self::read_guard(&self.prefs).expand_connected_details
    }

    pub fn icons_only_navigation(&self) -> bool {
        Self::read_guard(&self.prefs).icons_only_navigation
    }

    pub fn set_page_visible(&self, page: PageKind, visible: bool) {
        match page {
            PageKind::Wifi => self
                .visibility
                .wifi_visible
                .store(visible, Ordering::Relaxed),
            PageKind::Hotspot => self
                .visibility
                .hotspot_visible
                .store(visible, Ordering::Relaxed),
            PageKind::Devices => self
                .visibility
                .devices_visible
                .store(visible, Ordering::Relaxed),
        }
    }

    pub fn is_page_visible(&self, page: PageKind) -> bool {
        match page {
            PageKind::Wifi => self.visibility.wifi_visible.load(Ordering::Relaxed),
            PageKind::Hotspot => self.visibility.hotspot_visible.load(Ordering::Relaxed),
            PageKind::Devices => self.visibility.devices_visible.load(Ordering::Relaxed),
        }
    }

    pub fn wifi_busy_count_inc(&self) -> u32 {
        self.wifi.busy_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn wifi_busy_count_dec(&self) -> u32 {
        loop {
            let current = self.wifi.busy_count.load(Ordering::Relaxed);
            if current == 0 {
                return 0;
            }
            if self
                .wifi
                .busy_count
                .compare_exchange(current, current - 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return current - 1;
            }
        }
    }

    pub fn wifi_busy_count_reset(&self) {
        self.wifi.busy_count.store(0, Ordering::Relaxed);
    }

    pub fn wifi_busy_count(&self) -> u32 {
        self.wifi.busy_count.load(Ordering::Relaxed)
    }

    pub fn wifi_search_text(&self) -> String {
        Self::read_guard(&self.wifi.search_text).clone()
    }

    pub fn set_wifi_search_text(&self, value: String) {
        *Self::write_guard(&self.wifi.search_text) = value;
    }

    pub fn wifi_all_networks(&self) -> Vec<WifiNetwork> {
        Self::read_guard(&self.wifi.all_networks).clone()
    }

    pub fn set_wifi_all_networks(&self, networks: Vec<WifiNetwork>) {
        *Self::write_guard(&self.wifi.all_networks) = networks;
    }

    pub fn clear_wifi_all_networks(&self) {
        Self::write_guard(&self.wifi.all_networks).clear();
    }

    pub fn wifi_saved_ssids(&self) -> HashSet<String> {
        Self::read_guard(&self.wifi.saved_ssids).clone()
    }

    pub fn set_wifi_saved_ssids(&self, value: HashSet<String>) {
        *Self::write_guard(&self.wifi.saved_ssids) = value;
    }

    pub fn clear_wifi_saved_ssids(&self) {
        Self::write_guard(&self.wifi.saved_ssids).clear();
    }

    pub fn wifi_filter_state(&self) -> WifiFilterState {
        *Self::read_guard(&self.wifi.filter_state)
    }

    pub fn set_wifi_filter_state(&self, value: WifiFilterState) {
        *Self::write_guard(&self.wifi.filter_state) = value;
    }

    pub fn wifi_connected_network(&self) -> Option<WifiNetwork> {
        Self::read_guard(&self.wifi.connected_network).clone()
    }

    pub fn set_wifi_connected_network(&self, value: Option<WifiNetwork>) {
        *Self::write_guard(&self.wifi.connected_network) = value;
    }

    pub fn take_wifi_refresh_source(&self) -> Option<glib::SourceId> {
        Self::write_guard(&self.wifi.refresh_source).take()
    }

    pub fn set_wifi_refresh_source(&self, source: Option<glib::SourceId>) {
        *Self::write_guard(&self.wifi.refresh_source) = source;
    }

    pub fn wifi_has_refresh_source(&self) -> bool {
        Self::read_guard(&self.wifi.refresh_source).is_some()
    }

    pub fn take_wifi_search_debounce_source(&self) -> Option<glib::SourceId> {
        Self::write_guard(&self.wifi.search_debounce_source).take()
    }

    pub fn set_wifi_search_debounce_source(&self, source: Option<glib::SourceId>) {
        *Self::write_guard(&self.wifi.search_debounce_source) = source;
    }

    pub fn hotspot_devices(&self) -> Vec<String> {
        Self::read_guard(&self.hotspot.devices).clone()
    }

    pub fn set_hotspot_devices(&self, devices: Vec<String>) {
        *Self::write_guard(&self.hotspot.devices) = devices;
    }

    pub fn clear_hotspot_devices(&self) {
        Self::write_guard(&self.hotspot.devices).clear();
    }

    pub fn hotspot_active(&self) -> bool {
        self.hotspot.is_active.load(Ordering::Relaxed)
    }

    pub fn set_hotspot_active(&self, value: bool) {
        self.hotspot.is_active.store(value, Ordering::Relaxed);
    }

    pub fn hotspot_wifi_present(&self) -> bool {
        self.hotspot.wifi_present.load(Ordering::Relaxed)
    }

    pub fn set_hotspot_wifi_present(&self, value: bool) {
        self.hotspot.wifi_present.store(value, Ordering::Relaxed);
    }

    pub fn hotspot_wifi_enabled(&self) -> bool {
        self.hotspot.wifi_enabled.load(Ordering::Relaxed)
    }

    pub fn set_hotspot_wifi_enabled(&self, value: bool) {
        self.hotspot.wifi_enabled.store(value, Ordering::Relaxed);
    }

    pub fn hotspot_operation_in_progress(&self) -> bool {
        self.hotspot.operation_in_progress.load(Ordering::Relaxed)
    }

    pub fn set_hotspot_operation_in_progress(&self, value: bool) {
        self.hotspot
            .operation_in_progress
            .store(value, Ordering::Relaxed);
    }

    pub fn take_hotspot_status_refresh_source(&self) -> Option<glib::SourceId> {
        Self::write_guard(&self.hotspot.status_refresh_source).take()
    }

    pub fn set_hotspot_status_refresh_source(&self, source: Option<glib::SourceId>) {
        *Self::write_guard(&self.hotspot.status_refresh_source) = source;
    }

    pub fn hotspot_has_status_refresh_source(&self) -> bool {
        Self::read_guard(&self.hotspot.status_refresh_source).is_some()
    }

    pub fn take_hotspot_interface_refresh_source(&self) -> Option<glib::SourceId> {
        Self::write_guard(&self.hotspot.interface_refresh_source).take()
    }

    pub fn set_hotspot_interface_refresh_source(&self, source: Option<glib::SourceId>) {
        *Self::write_guard(&self.hotspot.interface_refresh_source) = source;
    }

    pub fn hotspot_has_interface_refresh_source(&self) -> bool {
        Self::read_guard(&self.hotspot.interface_refresh_source).is_some()
    }

    pub fn devices_auto_refresh_active(&self) -> bool {
        self.devices.auto_refresh_active.load(Ordering::Relaxed)
    }

    pub fn set_devices_auto_refresh_active(&self, value: bool) {
        self.devices
            .auto_refresh_active
            .store(value, Ordering::Relaxed);
    }

    pub fn devices_refresh_in_flight(&self) -> bool {
        self.devices.refresh_in_flight.load(Ordering::Relaxed)
    }

    pub fn set_devices_refresh_in_flight(&self, value: bool) {
        self.devices
            .refresh_in_flight
            .store(value, Ordering::Relaxed);
    }

    pub fn take_devices_refresh_source(&self) -> Option<glib::SourceId> {
        Self::write_guard(&self.devices.refresh_source).take()
    }

    pub fn set_devices_refresh_source(&self, source: Option<glib::SourceId>) {
        *Self::write_guard(&self.devices.refresh_source) = source;
    }

    pub fn devices_has_refresh_source(&self) -> bool {
        Self::read_guard(&self.devices.refresh_source).is_some()
    }

    pub fn ethernet_connections(&self) -> Vec<Connection> {
        Self::read_guard(&self.ethernet.connections).clone()
    }

    pub fn set_ethernet_connections(&self, connections: Vec<Connection>) {
        *Self::write_guard(&self.ethernet.connections) = connections;
    }

    pub fn ethernet_connected_connection(&self) -> Option<Connection> {
        Self::read_guard(&self.ethernet.connected_connection).clone()
    }

    pub fn set_ethernet_connected_connection(&self, connection: Option<Connection>) {
        *Self::write_guard(&self.ethernet.connected_connection) = connection;
    }

    pub fn ethernet_devices(&self) -> Vec<String> {
        Self::read_guard(&self.ethernet.ethernet_devices).clone()
    }

    pub fn set_ethernet_devices(&self, devices: Vec<String>) {
        *Self::write_guard(&self.ethernet.ethernet_devices) = devices;
    }

    pub fn profiles_list(&self) -> Vec<NetworkProfile> {
        Self::read_guard(&self.profiles.profiles).clone()
    }

    pub fn set_profiles_list(&self, profiles: Vec<NetworkProfile>) {
        *Self::write_guard(&self.profiles.profiles) = profiles;
    }
}
