// File: actions.rs
// Location: /src/ui/wifi_page/actions.rs

use super::WifiPage;

pub(super) struct BusyGuard {
    pub(super) page: WifiPage,
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.page.set_busy(false, None);
    }
}
