// File: secrets.rs
// Location: /src/secrets.rs

use anyhow::{Result, anyhow};
use keyring::Error as KeyringError;

const KEYRING_SERVICE: &str = "adw-network";
const KEYRING_USERNAME: &str = "hotspot-password";

pub fn store_hotspot_password(password: &str) -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USERNAME)?;
    entry
        .set_password(password)
        .map_err(|e| anyhow!("Keyring save failed: {}", e))?;
    Ok(())
}

pub fn load_hotspot_password() -> Result<Option<String>> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USERNAME)?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(_) => Ok(None),
    }
}

pub fn delete_hotspot_password() -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USERNAME)?;
    match entry.delete_credential() {
        Ok(()) => {}
        Err(KeyringError::NoEntry) => {}
        Err(e) => return Err(anyhow!("Keyring clear failed: {}", e)),
    }
    Ok(())
}
