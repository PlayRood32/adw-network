use std::collections::HashSet;
use std::net::IpAddr;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct LeaseLoadResult {
    pub entries: Vec<LeaseEntry>,
    pub files_read: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseEntry {
    pub expiry: Option<i64>,
    pub mac: String,
    pub ip: String,
    pub hostname: Option<String>,
}

pub fn parse_lease_content(content: &str) -> Vec<LeaseEntry> {
    let mut devices = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // dnsmasq format: <expiry> <mac> <ip> <hostname> <client-id>
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        let ip = parts[2].trim();
        if is_filtered_client_ip(ip) {
            continue;
        }

        let hostname = if parts.len() > 3 {
            let raw = parts[3].trim();
            if raw.is_empty() || raw == "*" {
                None
            } else {
                Some(raw.to_string())
            }
        } else {
            None
        };

        devices.push(LeaseEntry {
            expiry: parts[0].parse::<i64>().ok(),
            mac: parts[1].trim().to_string(),
            ip: ip.to_string(),
            hostname,
        });
    }

    devices
}

pub fn dedupe_by_ip(entries: Vec<LeaseEntry>) -> Vec<LeaseEntry> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for entry in entries {
        if seen.insert(entry.ip.clone()) {
            out.push(entry);
        }
    }

    out
}

pub fn collect_ips(entries: &[LeaseEntry], out: &mut HashSet<String>) {
    for entry in entries {
        if !is_filtered_client_ip(&entry.ip) {
            out.insert(entry.ip.clone());
        }
    }
}

pub async fn load_lease_entries_with_stats() -> LeaseLoadResult {
    let mut entries = Vec::new();
    let mut files_read = 0usize;
    let mut candidate_paths = HashSet::new();

    let nm_lease_dirs = [
        "/var/lib/NetworkManager",
        "/run/NetworkManager",
        "/var/run/NetworkManager",
    ];
    for dir in &nm_lease_dirs {
        let nm_lease_dir = Path::new(dir);
        if !nm_lease_dir.is_dir() {
            continue;
        }
        if let Ok(dir_entries) = std::fs::read_dir(nm_lease_dir) {
            for entry in dir_entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with("dnsmasq-") || !name.ends_with(".leases") {
                    continue;
                }
                candidate_paths.insert(entry.path());
            }
        }
    }

    let fallback_paths = [
        "/var/lib/dnsmasq/dnsmasq.leases",
        "/var/lib/misc/dnsmasq.leases",
        "/var/db/dnsmasq.leases",
        "/run/dnsmasq/dnsmasq.leases",
        "/var/run/dnsmasq/dnsmasq.leases",
        "/run/NetworkManager/dnsmasq.leases",
        "/tmp/dnsmasq.leases",
    ];
    for path in &fallback_paths {
        candidate_paths.insert(Path::new(path).to_path_buf());
    }

    for path in candidate_paths {
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            files_read += 1;
            entries.extend(parse_lease_content(&content));
        }
    }

    LeaseLoadResult {
        entries: dedupe_by_ip(entries),
        files_read,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dnsmasq_line_with_hostname() {
        let raw = "1718575077 aa:bb:cc:dd:ee:ff 192.168.50.12 android-123 *";
        let entries = parse_lease_content(raw);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ip, "192.168.50.12");
        assert_eq!(entries[0].hostname.as_deref(), Some("android-123"));
    }

    #[test]
    fn filters_non_client_ips() {
        let raw = "\
1718575077 aa:bb:cc:dd:ee:ff 127.0.0.1 gateway *\n\
1718575077 aa:bb:cc:dd:ee:00 192.168.50.77 phone *";
        let entries = parse_lease_content(raw);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ip, "192.168.50.77");
    }
}

pub fn is_filtered_client_ip(ip: &str) -> bool {
    if ip.ends_with(".0") || ip == "::1" {
        return true;
    }

    match ip.parse::<IpAddr>() {
        // * Treat multicast and invalid/host-local addresses as non-client IPs.
        Ok(IpAddr::V4(v4)) => {
            v4.is_loopback() || v4.is_link_local() || v4.is_unspecified() || v4.is_multicast()
        }
        // * Keep IPv6 link-local client addresses visible while filtering non-client scopes.
        Ok(IpAddr::V6(v6)) => v6.is_loopback() || v6.is_unspecified() || v6.is_multicast(),
        Err(_) => true,
    }
}

#[cfg(test)]
mod ip_filter_tests {
    use super::is_filtered_client_ip;

    #[test]
    fn keeps_ipv6_link_local_clients() {
        assert!(!is_filtered_client_ip("fe80::abcd"));
    }

    #[test]
    fn filters_ipv6_loopback_and_unspecified() {
        assert!(is_filtered_client_ip("::1"));
        assert!(is_filtered_client_ip("::"));
    }

    #[test]
    fn filters_ipv6_multicast() {
        assert!(is_filtered_client_ip("ff02::1"));
    }
}
