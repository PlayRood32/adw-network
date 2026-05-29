// * ./src/models.rs

use glib::prelude::*;
use glib::subclass::prelude::*;
use std::cell::RefCell;

pub mod wifi_network_imp {
    use glib::Properties;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::WifiNetwork)]
    pub struct WifiNetwork {
        #[property(get, set, construct)]
        pub ssid: RefCell<String>,
        #[property(get, set, construct)]
        pub signal: RefCell<u8>,
        #[property(get, set, construct)]
        pub secured: RefCell<bool>,
        #[property(get, set, construct)]
        pub connected: RefCell<bool>,
        #[property(get, set, construct)]
        pub band: RefCell<String>,
        #[property(get, set, construct)]
        pub channel: RefCell<u32>,
        #[property(get, set, construct)]
        pub freq_mhz: RefCell<u32>,
        #[property(get, set, construct)]
        pub security_type: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WifiNetwork {
        const NAME: &'static str = "AdwNetworkWifiNetwork";
        type Type = super::WifiNetwork;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for WifiNetwork {}
}

glib::wrapper! {
    pub struct WifiNetwork(ObjectSubclass<wifi_network_imp::WifiNetwork>);
}

impl WifiNetwork {
    pub fn new(
        ssid: &str,
        signal: u8,
        secured: bool,
        connected: bool,
        band: &str,
        channel: u32,
        freq_mhz: u32,
        security_type: &str,
    ) -> Self {
        glib::Object::builder()
            .property("ssid", ssid)
            .property("signal", signal)
            .property("secured", secured)
            .property("connected", connected)
            .property("band", band)
            .property("channel", channel)
            .property("freq-mhz", freq_mhz)
            .property("security-type", security_type)
            .build()
    }
}

impl From<crate::nm::WifiNetwork> for WifiNetwork {
    fn from(n: crate::nm::WifiNetwork) -> Self {
        Self::new(
            &n.ssid,
            n.signal,
            n.secured,
            n.connected,
            &n.band,
            n.channel,
            n.freq_mhz,
            &n.security_type,
        )
    }
}

pub mod connection_imp {
    use glib::Properties;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::Connection)]
    pub struct Connection {
        #[property(get, set, construct)]
        pub name: RefCell<String>,
        #[property(get, set, construct)]
        pub uuid: RefCell<String>,
        #[property(get, set, construct)]
        pub conn_type: RefCell<String>,
        #[property(get, set, construct)]
        pub device: RefCell<Option<String>>,
        #[property(get, set, construct)]
        pub active: RefCell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Connection {
        const NAME: &'static str = "AdwNetworkConnection";
        type Type = super::Connection;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Connection {}
}

glib::wrapper! {
    pub struct Connection(ObjectSubclass<connection_imp::Connection>);
}

impl Connection {
    pub fn new(
        name: &str,
        uuid: &str,
        conn_type: &str,
        device: Option<String>,
        active: bool,
    ) -> Self {
        glib::Object::builder()
            .property("name", name)
            .property("uuid", uuid)
            .property("conn-type", conn_type)
            .property("device", device)
            .property("active", active)
            .build()
    }
}

impl From<crate::nm::Connection> for Connection {
    fn from(c: crate::nm::Connection) -> Self {
        Self::new(&c.name, &c.uuid, &c.conn_type, c.device, c.active)
    }
}

pub mod device_imp {
    use glib::Properties;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::Device)]
    pub struct Device {
        #[property(get, set, construct)]
        pub name: RefCell<String>,
        #[property(get, set, construct)]
        pub device_type: RefCell<String>,
        #[property(get, set, construct)]
        pub state: RefCell<String>,
        #[property(get, set, construct)]
        pub connection: RefCell<Option<String>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Device {
        const NAME: &'static str = "AdwNetworkDevice";
        type Type = super::Device;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Device {}
}

glib::wrapper! {
    pub struct Device(ObjectSubclass<device_imp::Device>);
}

impl Device {
    pub fn new(
        name: &str,
        device_type: &str,
        state: &str,
        connection: Option<String>,
    ) -> Self {
        glib::Object::builder()
            .property("name", name)
            .property("device-type", device_type)
            .property("state", state)
            .property("connection", connection)
            .build()
    }
}

impl From<crate::nm::Device> for Device {
    fn from(d: crate::nm::Device) -> Self {
        Self::new(
            &d.name,
            &d.device_type.to_string(),
            &d.state,
            d.connection,
        )
    }
}
