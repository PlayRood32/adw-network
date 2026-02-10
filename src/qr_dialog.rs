// File: qr_dialog.rs
// Location: /src/qr_dialog.rs

use gtk4::prelude::*;
use libadwaita::{self as adw, prelude::*};
use gdk_pixbuf::Pixbuf;

use crate::qr;

pub async fn show_qr_dialog(
    ssid: &str,
    password: &str,
    _size: i32,
    toast_overlay: &adw::ToastOverlay,
) {
    let wifi_string = if password.is_empty() {
        format!("WIFI:T:nopass;S:{};P:;;", ssid)
    } else {
        format!("WIFI:T:WPA;S:{};P:{};;", ssid, password)
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

            let dialog = adw::Dialog::builder()
                .title(format!("QR Code for {}", ssid))
                .content_width(420)
                .content_height(420)
                .build();

            let picture = gtk4::Picture::for_pixbuf(&pixbuf);
            picture.set_content_fit(gtk4::ContentFit::Contain);
            picture.set_size_request(300, 300);
            
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
            dialog.present(Some(toast_overlay));
        }
        Err(e) => {
            let toast = adw::Toast::new(&format!("Failed to generate QR code: {}", e));
            toast_overlay.add_toast(toast);
        }
    }
}
