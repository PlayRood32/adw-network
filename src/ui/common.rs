use gtk4::prelude::*;
use libadwaita::{self as adw, prelude::*};
use std::time::Duration;

pub fn show_toast(overlay: &adw::ToastOverlay, message: &str) {
    let toast = adw::Toast::new(message);
    toast.set_timeout(5);
    overlay.add_toast(toast);
}

pub fn set_busy(
    spinner: &gtk4::Spinner,
    status_label: &gtk4::Label,
    refresh_button: Option<&gtk4::Button>,
    busy: bool,
    status: Option<&str>,
) {
    if busy {
        spinner.set_visible(true);
        spinner.start();
        status_label.set_visible(true);
        status_label.set_text(status.unwrap_or("Working..."));
        status_label.add_css_class("dim-label");
        status_label.add_css_class("large-text");
        if let Some(button) = refresh_button {
            button.set_sensitive(false);
        }
        return;
    }

    spinner.stop();
    spinner.set_visible(false);
    status_label.set_text("");
    status_label.set_visible(false);
    status_label.remove_css_class("dim-label");
    status_label.remove_css_class("large-text");
    if let Some(button) = refresh_button {
        button.set_sensitive(true);
    }
}

fn apply_dialog_size(
    dialog: &adw::Dialog,
    parent_window: Option<&gtk4::Window>,
    fallback_width: i32,
    fallback_height: i32,
) {
    let mut width = fallback_width.max(320);
    let mut height = fallback_height.max(280);

    if let Some(parent) = parent_window {
        let parent_width = parent.width();
        let parent_height = parent.height();

        if parent_width > 0 {
            let preferred_width = ((parent_width as f64) * 0.92).round() as i32;
            let _max_width = parent_width.saturating_sub(24).max(320);
            width = preferred_width.clamp(320, 600);
        }
        if parent_height > 0 {
            let preferred_height = ((parent_height as f64) * 0.90).round() as i32;
            let _max_height = parent_height.saturating_sub(24).max(280);
            height = preferred_height.clamp(280, 500);
        }
    }

    dialog.set_content_width(width);
    dialog.set_content_height(height);
}

pub fn make_dialog_responsive(
    dialog: &adw::Dialog,
    parent_window: Option<&gtk4::Window>,
    fallback_width: i32,
    fallback_height: i32,
) {
    // * Keep inner dialogs responsive to the parent window size.
    // Handles tablet modes by checking display size.
    dialog.set_follows_content_size(false);
    apply_dialog_size(dialog, parent_window, fallback_width, fallback_height);

    let dialog_weak = dialog.downgrade();
    let parent_weak = parent_window.map(|window| window.downgrade());
    glib::timeout_add_local(Duration::from_millis(150), move || {
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
