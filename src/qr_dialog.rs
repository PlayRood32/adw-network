// File: qr_dialog.rs
// Location: /src/qr_dialog.rs

use gdk_pixbuf::Pixbuf;
use gtk4::prelude::*;
use libadwaita::{self as adw, prelude::*};
use std::time::Duration;

use crate::qr;

fn apply_dialog_size(
    dialog: &adw::Dialog,
    parent_window: Option<&gtk4::Window>,
    fallback_width: i32,
    fallback_height: i32,
) {
    let mut width = fallback_width.max(280);
    let mut height = fallback_height.max(260);

    if let Some(parent) = parent_window {
        let parent_width = parent.width();
        let parent_height = parent.height();
        if parent_width > 0 {
            let preferred_width = ((parent_width as f64) * 0.95).round() as i32;
            let max_width = parent_width.saturating_sub(24).max(280);
            width = preferred_width.clamp(280, max_width);
        }
        if parent_height > 0 {
            let preferred_height = ((parent_height as f64) * 0.95).round() as i32;
            let max_height = parent_height.saturating_sub(24).max(260);
            height = preferred_height.clamp(260, max_height);
        }
    }

    dialog.set_content_width(width);
    dialog.set_content_height(height);
}

fn make_dialog_responsive(
    dialog: &adw::Dialog,
    parent_window: Option<&gtk4::Window>,
    fallback_width: i32,
    fallback_height: i32,
) {
    // * Keep QR dialog responsive to the parent window size.
    dialog.set_follows_content_size(false);
    apply_dialog_size(dialog, parent_window, fallback_width, fallback_height);

    let dialog_weak = dialog.downgrade();
    let parent_weak = parent_window.map(|window| window.downgrade());
    glib::timeout_add_local(Duration::from_millis(200), move || {
        let Some(dialog) = dialog_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        if !dialog.is_visible() {
            return glib::ControlFlow::Break;
        }

        let parent = parent_weak.as_ref().and_then(|window| window.upgrade());
        apply_dialog_size(&dialog, parent.as_ref(), fallback_width, fallback_height);
        glib::ControlFlow::Continue
    });
}

pub async fn show_qr_dialog(
    ssid: &str,
    password: &str,
    security_type: Option<&str>,
    size: i32,
    toast_overlay: &adw::ToastOverlay,
) {
    let ssid_escaped = escape_wifi_field(ssid);
    let password_escaped = escape_wifi_field(password);
    let auth = wifi_auth_type(password, security_type);
    let wifi_string = if password.is_empty() {
        format!("WIFI:T:{};S:{};;", auth, ssid_escaped)
    } else {
        format!(
            "WIFI:T:{};S:{};P:{};;",
            auth, ssid_escaped, password_escaped
        )
    };

    let qr_result = qr::generate_bytes_for_pixbuf(&wifi_string[..]);

    match qr_result {
        Ok((bytes, width, height)) => {
            let pixbuf = Pixbuf::from_bytes(
                &glib::Bytes::from(&bytes),
                gdk_pixbuf::Colorspace::Rgb,
                false,
                8,
                width,
                height,
                width * 3,
            );

            // Use requested logical image size to compute dialog content size.
            let image_size = if size > 0 { size } else { 300 };
            let fallback_w = (image_size + 120).max(280);
            let fallback_h = (image_size + 120).max(260);

            let dialog = adw::Dialog::builder()
                .title(format!("QR Code for {}", ssid))
                .content_width(fallback_w)
                .content_height(fallback_h)
                .build();
            let parent_window = toast_overlay
                .root()
                .and_then(|root| root.downcast::<gtk4::Window>().ok());
            // Make the QR dialog track the parent window size but keep it reasonably small.
            make_dialog_responsive(&dialog, parent_window.as_ref(), fallback_w, fallback_h);

            let picture = gtk4::Picture::for_pixbuf(&pixbuf);
            picture.set_content_fit(gtk4::ContentFit::Contain);
            // Allow QR content to shrink/grow but prefer centering and keeping dialog compact.
            picture.set_hexpand(false);
            picture.set_vexpand(false);

            let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
            content.set_margin_top(12);
            content.set_margin_bottom(12);
            content.set_margin_start(12);
            content.set_margin_end(12);

            let subtitle = gtk4::Label::new(Some("Scan this QR code to connect to the network"));
            subtitle.set_xalign(0.0);
            subtitle.set_opacity(0.7);
            content.append(&subtitle);
            content.append(&picture);

            if !password.is_empty() {
                let pass_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);

                let password_label = gtk4::Label::new(Some(&format!("Password: {}", password)));
                password_label.set_selectable(true);
                password_label.add_css_class("dim-label");
                password_label.set_hexpand(true);

                let copy_btn = gtk4::Button::builder()
                    .icon_name("edit-copy-symbolic")
                    .tooltip_text("Copy password")
                    .css_classes(vec!["flat".to_string()])
                    .build();

                let password_copy = password.to_string();
                copy_btn.connect_clicked(move |btn| {
                    let display = btn.display();
                    let clipboard = display.clipboard();
                    clipboard.set_text(&password_copy);
                });

                pass_box.append(&password_label);
                pass_box.append(&copy_btn);
                content.append(&pass_box);
            }

            let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
            buttons.set_halign(gtk4::Align::End);
            buttons.set_margin_top(12);

            let close_btn = gtk4::Button::builder()
                .label("Close")
                .css_classes(vec!["flat".to_string()])
                .build();
            let dialog_close = dialog.clone();
            close_btn.connect_clicked(move |_| {
                dialog_close.close();
            });

            buttons.append(&close_btn);
            content.append(&buttons);

            dialog.set_child(Some(&content));
            if let Some(parent) = parent_window.as_ref() {
                dialog.present(Some(parent));
            } else {
                dialog.present(Some(toast_overlay));
            }
        }
        Err(_) => {
            let toast =
                adw::Toast::new("QR code generation failed—please check your network details.");
            toast.set_timeout(4);
            toast_overlay.add_toast(toast);
        }
    }
}

fn escape_wifi_field(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' | ';' | ',' | ':' | '"' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' | '\r' => out.push_str("\\n"),
            _ => out.push(ch),
        }
    }
    out
}

fn wifi_auth_type<'a>(password: &str, security_type: Option<&'a str>) -> &'a str {
    if password.is_empty() {
        return "nopass";
    }
    let sec = security_type.unwrap_or_default().to_ascii_lowercase();
    if sec.contains("wep") {
        "WEP"
    } else if sec.contains("wpa3") || sec.contains("sae") {
        "SAE"
    } else {
        "WPA"
    }
}

#[cfg(test)]
mod tests {
    use super::{escape_wifi_field, wifi_auth_type};

    #[test]
    fn escapes_wifi_payload_characters() {
        let input = "a\\b;c,d:e\"f\ng";
        assert_eq!(escape_wifi_field(input), "a\\\\b\\;c\\,d\\:e\\\"f\\ng");
    }

    #[test]
    fn maps_wpa3_to_sae() {
        assert_eq!(wifi_auth_type("12345678", Some("WPA3")), "SAE");
        assert_eq!(wifi_auth_type("12345678", Some("sae")), "SAE");
    }
}
