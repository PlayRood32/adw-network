// File: details.rs
// Location: /src/ui/wifi_page/details.rs

use std::net::IpAddr;

pub(super) fn get_signal_icon(signal: u8) -> &'static str {
    if signal >= 75 {
        "network-wireless-signal-excellent-symbolic"
    } else if signal >= 50 {
        "network-wireless-signal-good-symbolic"
    } else if signal >= 25 {
        "network-wireless-signal-ok-symbolic"
    } else {
        "network-wireless-signal-weak-symbolic"
    }
}

pub(super) fn get_signal_strength_text(signal: u8) -> String {
    let quality = if signal >= 75 {
        "Excellent connection (75%+)"
    } else if signal >= 50 {
        "Good connection (50-74%)"
    } else if signal >= 25 {
        "Fair connection (25-49%)"
    } else if signal >= 10 {
        "Weak connection (10-24%)"
    } else {
        "Very weak connection (<10%)"
    };
    format!("{} ({}%)", quality, signal)
}

pub(super) fn invalid_ip_entries(entries: &[String]) -> Vec<String> {
    entries
        .iter()
        .filter(|entry| entry.parse::<IpAddr>().is_err())
        .cloned()
        .collect()
}
