// File: mod.rs
// Location: /src/ui/mod.rs

pub mod common;
pub mod devices_page;
pub mod ethernet_page;
pub mod hotspot_page;
pub mod profiles_page;
pub mod wifi_page;

pub fn icon_name<'a>(primary: &'a str, fallbacks: &'a [&'a str]) -> &'a str {
    let Some(display) = gtk4::gdk::Display::default() else {
        return primary;
    };
    let theme = gtk4::IconTheme::for_display(&display);

    if theme.has_icon(primary) {
        return primary;
    }

    // Try higher-resolution symbolic variants
    let large_variant = format!("{}.large", primary);
    if theme.has_icon(&large_variant) {
        return primary;
    }

    for &name in fallbacks {
        if theme.has_icon(name) {
            return name;
        }
    }

    primary
}
