use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::{self as adw, prelude::*};
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use uuid::Uuid;

use crate::nm::{
    self, Connection, OpenVpnConnectionConfig, VpnConnection, VpnKind, WireGuardConnectionConfig,
};
use crate::profiles::{self, NetworkProfile};
use crate::ui::{common, icon_name};

pub struct ProfilesPage {
    pub widget: gtk4::Box,
    toast_overlay: adw::ToastOverlay,
    profile_list_box: gtk4::ListBox,
    profile_empty_state: adw::StatusPage,
    vpn_list_box: gtk4::ListBox,
    vpn_empty_state: adw::StatusPage,
    new_profile_button: gtk4::Button,
    new_vpn_button: gtk4::Button,
    import_vpn_button: gtk4::Button,
    refresh_button: gtk4::Button,
    spinner: gtk4::Spinner,
    operation_status_label: gtk4::Label,
    profiles: Rc<RefCell<Vec<NetworkProfile>>>,
    vpn_connections: Rc<RefCell<Vec<VpnConnection>>>,
}

impl Clone for ProfilesPage {
    fn clone(&self) -> Self {
        Self {
            widget: self.widget.clone(),
            toast_overlay: self.toast_overlay.clone(),
            profile_list_box: self.profile_list_box.clone(),
            profile_empty_state: self.profile_empty_state.clone(),
            vpn_list_box: self.vpn_list_box.clone(),
            vpn_empty_state: self.vpn_empty_state.clone(),
            new_profile_button: self.new_profile_button.clone(),
            new_vpn_button: self.new_vpn_button.clone(),
            import_vpn_button: self.import_vpn_button.clone(),
            refresh_button: self.refresh_button.clone(),
            spinner: self.spinner.clone(),
            operation_status_label: self.operation_status_label.clone(),
            profiles: self.profiles.clone(),
            vpn_connections: self.vpn_connections.clone(),
        }
    }
}

impl ProfilesPage {
    pub fn new() -> Self {
        let widget = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let toast_overlay = adw::ToastOverlay::new();

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .build();

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(16);
        content.set_margin_end(16);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);

        let title = gtk4::Label::builder()
            .label("Profiles")
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .build();
        title.add_css_class("title-4");

        let new_profile_button = gtk4::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text("New profile")
            .css_classes(vec!["flat".to_string(), "circular".to_string()])
            .build();

        let new_vpn_button = gtk4::Button::builder()
            .icon_name(icon_name(
                "network-vpn-symbolic",
                &[
                    "network-wireless-encrypted-symbolic",
                    "network-workgroup-symbolic",
                ][..],
            ))
            .tooltip_text("Create VPN")
            .css_classes(vec!["flat".to_string(), "circular".to_string()])
            .build();

        let import_vpn_button = gtk4::Button::builder()
            .icon_name(icon_name(
                "document-open-symbolic",
                &["folder-open-symbolic", "folder-symbolic"][..],
            ))
            .tooltip_text("Import VPN")
            .css_classes(vec!["flat".to_string(), "circular".to_string()])
            .build();

        let refresh_button = gtk4::Button::builder()
            .icon_name(icon_name(
                "view-refresh-symbolic",
                &["view-refresh", "reload-symbolic"][..],
            ))
            .tooltip_text("Refresh profiles and VPN connections")
            .css_classes(vec!["flat".to_string(), "circular".to_string()])
            .build();

        let spinner = gtk4::Spinner::new();
        spinner.add_css_class("big-spinner");
        spinner.set_size_request(22, 22);
        spinner.set_visible(false);

        let operation_status_label = gtk4::Label::new(None);
        operation_status_label.set_halign(gtk4::Align::Start);
        operation_status_label.set_opacity(0.7);
        operation_status_label.set_visible(false);

        header.append(&title);
        header.append(&new_profile_button);
        header.append(&new_vpn_button);
        header.append(&import_vpn_button);
        header.append(&spinner);
        header.append(&refresh_button);
        content.append(&header);
        content.append(&operation_status_label);

        let profile_title = gtk4::Label::builder()
            .label("Profile Sets")
            .halign(gtk4::Align::Start)
            .build();
        profile_title.add_css_class("heading");
        content.append(&profile_title);

        let profile_list_box = gtk4::ListBox::builder()
            .css_classes(vec!["boxed-list".to_string()])
            .selection_mode(gtk4::SelectionMode::None)
            .build();
        profile_list_box.set_visible(false);

        let profile_empty_state = adw::StatusPage::builder()
            .icon_name(icon_name(
                "network-workgroup-symbolic",
                &["folder-symbolic", "applications-system-symbolic"][..],
            ))
            .title("No profile sets")
            .description("Create a profile to manage auto-connect behavior")
            .build();
        profile_empty_state.set_visible(true);

        content.append(&profile_list_box);
        content.append(&profile_empty_state);

        let vpn_title = gtk4::Label::builder()
            .label("VPN Connections")
            .halign(gtk4::Align::Start)
            .build();
        vpn_title.add_css_class("heading");
        content.append(&vpn_title);

        let vpn_list_box = gtk4::ListBox::builder()
            .css_classes(vec!["boxed-list".to_string()])
            .selection_mode(gtk4::SelectionMode::None)
            .build();
        vpn_list_box.set_visible(false);

        let vpn_empty_state = adw::StatusPage::builder()
            .icon_name(icon_name(
                "network-vpn-symbolic",
                &[
                    "network-wireless-encrypted-symbolic",
                    "network-workgroup-symbolic",
                ][..],
            ))
            .title("No supported VPN connections")
            .description("Create a WireGuard VPN or import an OpenVPN/WireGuard profile")
            .build();
        vpn_empty_state.set_visible(true);

        content.append(&vpn_list_box);
        content.append(&vpn_empty_state);

        scrolled.set_child(Some(&content));
        toast_overlay.set_child(Some(&scrolled));
        widget.append(&toast_overlay);

        let page = Self {
            widget,
            toast_overlay,
            profile_list_box,
            profile_empty_state,
            vpn_list_box,
            vpn_empty_state,
            new_profile_button: new_profile_button.clone(),
            new_vpn_button: new_vpn_button.clone(),
            import_vpn_button: import_vpn_button.clone(),
            refresh_button: refresh_button.clone(),
            spinner: spinner.clone(),
            operation_status_label: operation_status_label.clone(),
            profiles: Rc::new(RefCell::new(Vec::new())),
            vpn_connections: Rc::new(RefCell::new(Vec::new())),
        };

        let page_ref = page.clone();
        glib::spawn_future_local(async move {
            page_ref.refresh_profiles().await;
        });

        let page_ref = page.clone();
        new_profile_button.connect_clicked(move |_| {
            let page = page_ref.clone();
            glib::spawn_future_local(async move {
                page.create_profile().await;
            });
        });

        let page_ref = page.clone();
        new_vpn_button.connect_clicked(move |_| {
            let page = page_ref.clone();
            glib::spawn_future_local(async move {
                page.create_vpn().await;
            });
        });

        let page_ref = page.clone();
        import_vpn_button.connect_clicked(move |_| {
            let page = page_ref.clone();
            glib::spawn_future_local(async move {
                page.import_vpn().await;
            });
        });

        let page_ref = page.clone();
        refresh_button.connect_clicked(move |_| {
            let page = page_ref.clone();
            glib::spawn_future_local(async move {
                page.refresh_profiles().await;
            });
        });

        page
    }

    pub async fn refresh_profiles(&self) {
        common::set_busy(
            &self.spinner,
            &self.operation_status_label,
            Some(&self.refresh_button),
            true,
            Some("Refreshing..."),
        );
        self.new_profile_button.set_sensitive(false);
        self.new_vpn_button.set_sensitive(false);
        self.import_vpn_button.set_sensitive(false);
        self.profile_list_box.add_css_class("list-loading");
        self.vpn_list_box.add_css_class("list-loading");

        let path = profiles::profiles_path();
        match profiles::load_profiles(&path) {
            Ok(loaded) => {
                if let Ok(mut profiles_state) = self.profiles.try_borrow_mut() {
                    *profiles_state = loaded.clone();
                } else {
                    log::error!("Borrow conflict in UI state");
                    self.finish_refresh();
                    return;
                }
                self.populate_profiles(loaded);
            }
            Err(e) => {
                log::error!("Failed to load profiles: {}", e);
                self.show_toast(&format!("Failed to load profiles: {}", e));
                self.populate_profiles(Vec::new());
            }
        }

        match nm::list_supported_vpn_connections().await {
            Ok(loaded) => {
                if let Ok(mut vpn_state) = self.vpn_connections.try_borrow_mut() {
                    *vpn_state = loaded.clone();
                } else {
                    log::error!("Borrow conflict in UI state");
                    self.finish_refresh();
                    return;
                }
                self.populate_vpns(loaded);
            }
            Err(e) => {
                log::error!("Failed to load VPN connections: {}", e);
                self.show_toast(&format!("Failed to load VPN connections: {}", e));
                self.populate_vpns(Vec::new());
            }
        }

        self.finish_refresh();
    }

    fn finish_refresh(&self) {
        self.profile_list_box.remove_css_class("list-loading");
        self.vpn_list_box.remove_css_class("list-loading");
        common::set_busy(
            &self.spinner,
            &self.operation_status_label,
            Some(&self.refresh_button),
            false,
            None,
        );
        self.new_profile_button.set_sensitive(true);
        self.new_vpn_button.set_sensitive(true);
        self.import_vpn_button.set_sensitive(true);
    }

    fn populate_profiles(&self, profiles_list: Vec<NetworkProfile>) {
        while let Some(child) = self.profile_list_box.first_child() {
            self.profile_list_box.remove(&child);
        }

        if profiles_list.is_empty() {
            self.profile_list_box.set_visible(false);
            self.profile_empty_state.set_visible(true);
            return;
        }

        for profile in profiles_list {
            self.profile_list_box
                .append(&self.create_profile_row(&profile));
        }

        self.profile_empty_state.set_visible(false);
        self.profile_list_box.set_visible(true);
    }

    fn populate_vpns(&self, vpn_connections: Vec<VpnConnection>) {
        while let Some(child) = self.vpn_list_box.first_child() {
            self.vpn_list_box.remove(&child);
        }

        if vpn_connections.is_empty() {
            self.vpn_list_box.set_visible(false);
            self.vpn_empty_state.set_visible(true);
            return;
        }

        for vpn in vpn_connections {
            self.vpn_list_box.append(&self.create_vpn_row(&vpn));
        }

        self.vpn_empty_state.set_visible(false);
        self.vpn_list_box.set_visible(true);
    }

    fn create_profile_row(&self, profile: &NetworkProfile) -> adw::ActionRow {
        let row = adw::ActionRow::new();
        row.set_title(&profile.name);

        let subtitle = if profile.active {
            format!("Active • {} connections", profile.connections.len())
        } else {
            format!("{} connections", profile.connections.len())
        };
        row.set_subtitle(&subtitle);

        let icon = gtk4::Image::new();
        icon.set_icon_name(Some(icon_name(
            "network-workgroup-symbolic",
            &["folder-symbolic", "applications-system-symbolic"][..],
        )));
        icon.set_pixel_size(24);
        row.add_prefix(&icon);

        if profile.active {
            let active_badge = gtk4::Image::from_icon_name(icon_name(
                "emblem-ok-symbolic",
                &["object-select-symbolic", "emblem-default-symbolic"][..],
            ));
            active_badge.set_pixel_size(14);
            active_badge.set_tooltip_text(Some("Active profile"));
            row.add_suffix(&active_badge);
        }

        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);

        let activate_btn = gtk4::Button::builder()
            .label("Activate")
            .tooltip_text("Activate profile")
            .css_classes(vec!["flat".to_string()])
            .sensitive(!profile.active)
            .build();

        let edit_btn = gtk4::Button::builder()
            .label("Edit")
            .tooltip_text("Edit profile")
            .css_classes(vec!["flat".to_string()])
            .build();

        let delete_btn = gtk4::Button::builder()
            .label("Delete")
            .tooltip_text("Delete profile")
            .css_classes(vec!["flat".to_string(), "destructive-action".to_string()])
            .build();

        if !profile.active {
            actions.append(&activate_btn);
        }
        actions.append(&edit_btn);
        actions.append(&delete_btn);
        row.add_suffix(&actions);
        if !profile.active {
            row.set_activatable_widget(Some(&activate_btn));
        }

        if !profile.active {
            let page_activate = self.clone();
            let profile_name_activate = profile.name.clone();
            activate_btn.connect_clicked(move |_| {
                let page = page_activate.clone();
                let profile_name = profile_name_activate.clone();
                glib::spawn_future_local(async move {
                    page.activate_profile(&profile_name).await;
                });
            });
        }

        let page_edit = self.clone();
        let profile_name_edit = profile.name.clone();
        let row_for_edit = row.clone();
        edit_btn.connect_clicked(move |_| {
            row_for_edit.add_css_class("card");
            let page = page_edit.clone();
            let profile_name = profile_name_edit.clone();
            let row_done = row_for_edit.clone();
            glib::spawn_future_local(async move {
                page.edit_profile(&profile_name).await;
                row_done.remove_css_class("card");
            });
        });

        let page_delete = self.clone();
        let profile_name_delete = profile.name.clone();
        delete_btn.connect_clicked(move |_| {
            let page = page_delete.clone();
            let profile_name = profile_name_delete.clone();
            glib::spawn_future_local(async move {
                page.delete_profile(&profile_name).await;
            });
        });

        if !profile.active {
            let page_row = self.clone();
            let profile_name_row = profile.name.clone();
            row.set_activatable(true);
            row.connect_activated(move |_| {
                let page = page_row.clone();
                let profile_name = profile_name_row.clone();
                glib::spawn_future_local(async move {
                    page.activate_profile(&profile_name).await;
                });
            });
        }

        row
    }

    fn create_vpn_row(&self, vpn: &VpnConnection) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(&vpn.name)
            .subtitle(if vpn.active {
                format!("{} • Connected", vpn.kind.label())
            } else {
                format!("{} • Disconnected", vpn.kind.label())
            })
            .build();

        let icon = gtk4::Image::from_icon_name(vpn_icon_name(vpn.kind));
        icon.set_pixel_size(22);
        row.add_prefix(&icon);

        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);

        let toggle_btn = gtk4::Button::builder()
            .label(if vpn.active { "Disconnect" } else { "Connect" })
            .tooltip_text(if vpn.active {
                "Disconnect VPN"
            } else {
                "Connect VPN"
            })
            .css_classes(vec!["flat".to_string()])
            .build();

        let edit_btn = gtk4::Button::builder()
            .label("Edit")
            .tooltip_text("Edit VPN")
            .css_classes(vec!["flat".to_string()])
            .build();

        let delete_btn = gtk4::Button::builder()
            .label("Delete")
            .tooltip_text("Delete VPN")
            .css_classes(vec!["flat".to_string(), "destructive-action".to_string()])
            .build();

        actions.append(&toggle_btn);
        actions.append(&edit_btn);
        actions.append(&delete_btn);
        row.add_suffix(&actions);
        row.set_activatable_widget(Some(&toggle_btn));

        let page_toggle = self.clone();
        let vpn_uuid = vpn.uuid.clone();
        let vpn_name = vpn.name.clone();
        let vpn_active = vpn.active;
        toggle_btn.connect_clicked(move |_| {
            let page = page_toggle.clone();
            let vpn_uuid = vpn_uuid.clone();
            let vpn_name = vpn_name.clone();
            glib::spawn_future_local(async move {
                page.toggle_vpn(&vpn_uuid, &vpn_name, vpn_active).await;
            });
        });

        let page_edit = self.clone();
        let vpn_uuid = vpn.uuid.clone();
        let vpn_kind = vpn.kind;
        edit_btn.connect_clicked(move |_| {
            let page = page_edit.clone();
            let vpn_uuid = vpn_uuid.clone();
            glib::spawn_future_local(async move {
                page.edit_vpn(&vpn_uuid, vpn_kind).await;
            });
        });

        let page_delete = self.clone();
        let vpn_uuid = vpn.uuid.clone();
        let vpn_name = vpn.name.clone();
        delete_btn.connect_clicked(move |_| {
            let page = page_delete.clone();
            let vpn_uuid = vpn_uuid.clone();
            let vpn_name = vpn_name.clone();
            glib::spawn_future_local(async move {
                page.delete_vpn(&vpn_uuid, &vpn_name).await;
            });
        });

        row
    }

    async fn create_profile(&self) {
        match self.show_profile_editor(None).await {
            Ok(Some(profile)) => {
                let mut current = self.profiles.borrow().clone();
                if current
                    .iter()
                    .any(|existing| existing.name.eq_ignore_ascii_case(&profile.name))
                {
                    self.show_toast("Profile name already exists");
                    return;
                }
                current.push(profile);
                let path = profiles::profiles_path();
                if let Err(e) = profiles::save_profiles(&path, &current) {
                    log::error!("Failed to save profiles: {}", e);
                    self.show_toast(&format!("Failed to save profiles: {}", e));
                    return;
                }
                self.refresh_profiles().await;
            }
            Ok(None) => {}
            Err(e) => {
                log::error!("Failed to create profile: {}", e);
                self.show_toast(&format!("Failed to create profile: {}", e));
            }
        }
    }

    async fn edit_profile(&self, profile_name: &str) {
        let snapshot_before_edit = self.profiles.borrow().clone();
        let existing = self
            .profiles
            .borrow()
            .iter()
            .find(|profile| profile.name == profile_name)
            .cloned();

        let Some(existing_profile) = existing else {
            self.show_toast("Profile not found");
            return;
        };

        match self
            .show_profile_editor(Some(existing_profile.clone()))
            .await
        {
            Ok(Some(updated)) => {
                let mut current = self.profiles.borrow().clone();
                if current.iter().any(|profile| {
                    profile.name != profile_name && profile.name.eq_ignore_ascii_case(&updated.name)
                }) {
                    self.show_toast("Profile name already exists");
                    return;
                }
                if existing_profile.active && !self.confirm_active_profile_save().await {
                    self.restore_profiles_snapshot(&snapshot_before_edit);
                    return;
                }

                if let Some(slot) = current
                    .iter_mut()
                    .find(|profile| profile.name == profile_name)
                {
                    let was_active = slot.active;
                    *slot = updated;
                    slot.active = was_active;
                }

                let path = profiles::profiles_path();
                if let Err(e) = profiles::save_profiles(&path, &current) {
                    log::error!("Failed to save profiles: {}", e);
                    self.show_toast(&format!("Failed to save profiles: {}", e));
                    return;
                }

                self.refresh_profiles().await;
            }
            Ok(None) => {
                self.restore_profiles_snapshot(&snapshot_before_edit);
            }
            Err(e) => {
                log::error!("Failed to edit profile: {}", e);
                self.show_toast(&format!("Failed to edit profile: {}", e));
                self.restore_profiles_snapshot(&snapshot_before_edit);
            }
        }
    }

    async fn delete_profile(&self, profile_name: &str) {
        let dialog = adw::AlertDialog::builder()
            .heading("Delete Profile?")
            .body(format!("This will delete profile \"{}\".", profile_name))
            .default_response("delete")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete")]);
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);

        let response = if let Some(parent) = self.widget.root().and_downcast_ref::<gtk4::Window>() {
            dialog.choose_future(Some(parent)).await
        } else {
            dialog.choose_future(None::<&gtk4::Window>).await
        };

        if response.as_str() != "delete" {
            return;
        }

        let mut current = self.profiles.borrow().clone();
        current.retain(|profile| profile.name != profile_name);
        let path = profiles::profiles_path();
        if let Err(e) = profiles::save_profiles(&path, &current) {
            log::error!("Failed to save profiles: {}", e);
            self.show_toast(&format!("Failed to save profiles: {}", e));
            return;
        }

        self.refresh_profiles().await;
    }

    async fn activate_profile(&self, profile_name: &str) {
        let path = profiles::profiles_path();
        match profiles::activate_profile_by_name(&path, profile_name).await {
            Ok(()) => {
                self.show_toast(&format!("Activated profile: {}", profile_name));
                self.refresh_profiles().await;
            }
            Err(e) => {
                log::error!("Failed to activate profile: {}", e);
                self.show_toast(&format!(
                    "Failed to activate profile \"{}\" in NetworkManager: {}",
                    profile_name, e
                ));
            }
        }
    }

    async fn create_vpn(&self) {
        let dialog = adw::AlertDialog::builder()
            .heading("Create VPN")
            .body("Choose how you want to create the VPN connection")
            .default_response("wireguard")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[
            ("cancel", "Cancel"),
            ("wireguard", "WireGuard"),
            ("openvpn", "OpenVPN"),
        ]);

        let response = if let Some(parent) = self.widget.root().and_downcast_ref::<gtk4::Window>() {
            dialog.choose_future(Some(parent)).await
        } else {
            dialog.choose_future(None::<&gtk4::Window>).await
        };

        match response.as_str() {
            "wireguard" => match self.show_wireguard_editor(None).await {
                Ok(Some(config)) => match nm::create_wireguard_connection(&config).await {
                    Ok(_) => {
                        self.show_toast("WireGuard VPN created");
                        self.refresh_profiles().await;
                    }
                    Err(e) => {
                        log::error!("Failed to create WireGuard VPN: {}", e);
                        self.show_toast(&format!("Failed to create WireGuard VPN: {}", e));
                    }
                },
                Ok(None) => {}
                Err(e) => {
                    log::error!("Failed to open WireGuard editor: {}", e);
                    self.show_toast(&format!("Failed to create WireGuard VPN: {}", e));
                }
            },
            "openvpn" => match self.show_openvpn_editor(None).await {
                Ok(Some(config)) => match nm::create_openvpn_connection(&config).await {
                    Ok(_) => {
                        self.show_toast("OpenVPN profile created");
                        self.refresh_profiles().await;
                    }
                    Err(e) => {
                        log::error!("Failed to create OpenVPN profile: {}", e);
                        let message = if nm::is_vpn_plugin_missing_error(&e.to_string()) {
                            "OpenVPN plugin is missing. Install the NetworkManager OpenVPN plugin and try again.".to_string()
                        } else {
                            format!("Failed to create OpenVPN profile: {}", e)
                        };
                        self.show_toast(&message);
                    }
                },
                Ok(None) => {}
                Err(e) => {
                    log::error!("Failed to open OpenVPN editor: {}", e);
                    self.show_toast(&format!("Failed to create OpenVPN profile: {}", e));
                }
            },
            _ => {}
        }
    }

    async fn import_vpn(&self) {
        match self.choose_import_file().await {
            Ok(Some(path)) => match nm::import_vpn_connection(&path).await {
                Ok(_) => {
                    self.show_toast("VPN imported");
                    self.refresh_profiles().await;
                }
                Err(e) => {
                    log::error!("Failed to import VPN: {}", e);
                    self.show_toast(&format!("Failed to import VPN: {}", e));
                }
            },
            Ok(None) => {}
            Err(e) => {
                log::error!("Failed to open import dialog: {}", e);
                self.show_toast(&format!("Failed to import VPN: {}", e));
            }
        }
    }

    async fn toggle_vpn(&self, uuid: &str, name: &str, active: bool) {
        let result = if active {
            nm::deactivate_vpn_connection(uuid).await
        } else {
            nm::activate_vpn_connection(uuid).await
        };

        match result {
            Ok(()) => {
                let message = if active {
                    format!("Disconnected VPN: {}", name)
                } else {
                    format!("Connected VPN: {}", name)
                };
                self.show_toast(&message);
                self.refresh_profiles().await;
            }
            Err(e) => {
                log::error!("Failed to toggle VPN {}: {}", name, e);
                self.show_toast(&format!("Failed to update VPN \"{}\": {}", name, e));
            }
        }
    }

    async fn edit_vpn(&self, uuid: &str, kind: VpnKind) {
        match kind {
            VpnKind::WireGuard => match nm::get_wireguard_connection_config(uuid).await {
                Ok(existing) => match self.show_wireguard_editor(Some(existing)).await {
                    Ok(Some(updated)) => {
                        match nm::update_wireguard_connection(uuid, &updated).await {
                            Ok(new_uuid_text) => {
                                if new_uuid_text != uuid {
                                    let old_uuid = Uuid::parse_str(uuid);
                                    let new_uuid = Uuid::parse_str(&new_uuid_text);
                                    if let (Ok(old_uuid), Ok(new_uuid)) = (old_uuid, new_uuid) {
                                        let _ = profiles::replace_connection_uuid_in_store(
                                            &profiles::profiles_path(),
                                            old_uuid,
                                            new_uuid,
                                        );
                                    }
                                }
                                self.show_toast("WireGuard VPN updated");
                                self.refresh_profiles().await;
                            }
                            Err(e) => {
                                log::error!("Failed to update WireGuard VPN: {}", e);
                                self.show_toast(&format!("Failed to update WireGuard VPN: {}", e));
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        log::error!("Failed to open WireGuard editor: {}", e);
                        self.show_toast(&format!("Failed to edit WireGuard VPN: {}", e));
                    }
                },
                Err(e) => {
                    log::error!("Failed to load WireGuard VPN: {}", e);
                    self.show_toast(&format!("Failed to load WireGuard VPN: {}", e));
                }
            },
            VpnKind::OpenVpn => match nm::get_openvpn_connection_config(uuid).await {
                Ok(existing) => match self.show_openvpn_editor(Some(existing)).await {
                    Ok(Some(updated)) => {
                        match nm::update_openvpn_connection(uuid, &updated).await {
                            Ok(()) => {
                                self.show_toast("OpenVPN profile updated");
                                self.refresh_profiles().await;
                            }
                            Err(e) => {
                                log::error!("Failed to update OpenVPN profile: {}", e);
                                self.show_toast(&format!(
                                    "Failed to update OpenVPN profile: {}",
                                    e
                                ));
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        log::error!("Failed to open OpenVPN editor: {}", e);
                        self.show_toast(&format!("Failed to edit OpenVPN profile: {}", e));
                    }
                },
                Err(e) => {
                    log::error!("Failed to load OpenVPN profile: {}", e);
                    self.show_toast(&format!("Failed to load OpenVPN profile: {}", e));
                }
            },
        }
    }

    async fn delete_vpn(&self, uuid: &str, name: &str) {
        let dialog = adw::AlertDialog::builder()
            .heading("Delete VPN?")
            .body(format!("This will delete VPN \"{}\".", name))
            .default_response("delete")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete")]);
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);

        let response = if let Some(parent) = self.widget.root().and_downcast_ref::<gtk4::Window>() {
            dialog.choose_future(Some(parent)).await
        } else {
            dialog.choose_future(None::<&gtk4::Window>).await
        };

        if response.as_str() != "delete" {
            return;
        }

        match nm::delete_vpn_connection(uuid).await {
            Ok(()) => {
                self.show_toast(&format!("Deleted VPN: {}", name));
                self.refresh_profiles().await;
            }
            Err(e) => {
                log::error!("Failed to delete VPN {}: {}", name, e);
                self.show_toast(&format!("Failed to delete VPN \"{}\": {}", name, e));
            }
        }
    }

    #[allow(deprecated)]
    async fn choose_import_file(&self) -> anyhow::Result<Option<PathBuf>> {
        let chooser = gtk4::FileChooserNative::builder()
            .title("Import VPN")
            .action(gtk4::FileChooserAction::Open)
            .accept_label("Import")
            .cancel_label("Cancel")
            .build();

        if let Some(parent) = self.widget.root().and_downcast_ref::<gtk4::Window>() {
            chooser.set_transient_for(Some(parent));
        }

        let response = chooser.run_future().await;
        let path = if response == gtk4::ResponseType::Accept {
            chooser.file().and_then(|file| file.path())
        } else {
            None
        };
        chooser.destroy();
        Ok(path)
    }

    async fn show_profile_editor(
        &self,
        existing: Option<NetworkProfile>,
    ) -> anyhow::Result<Option<NetworkProfile>> {
        let connections = profiles::get_profile_eligible_connections().await?;
        let selected_ids = Rc::new(RefCell::new(HashSet::<String>::new()));

        let heading = if existing.is_some() {
            "Edit Profile"
        } else {
            "New Profile"
        };

        let name_entry = adw::EntryRow::builder().title("Profile name").build();
        if let Some(profile) = existing.as_ref() {
            name_entry.set_text(&profile.name);
            if let Ok(mut selected_ids_state) = selected_ids.try_borrow_mut() {
                for uuid in &profile.connections {
                    selected_ids_state.insert(uuid.to_string());
                }
            } else {
                log::error!("Borrow conflict in UI state");
                return Ok(None);
            }
        }

        let filter_model = gtk4::StringList::new(&["All", "Wi-Fi", "Ethernet", "VPN"]);
        let filter_dropdown = gtk4::DropDown::new(Some(filter_model), None::<gtk4::Expression>);
        filter_dropdown.set_selected(0);

        let filter_row = adw::ActionRow::builder()
            .title("Connection type")
            .subtitle("Filter assignable connections")
            .build();
        filter_row.add_suffix(&filter_dropdown);
        filter_row.set_activatable_widget(Some(&filter_dropdown));

        let connections_list = gtk4::ListBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .css_classes(vec!["boxed-list".to_string()])
            .build();

        repopulate_connection_rows(
            &connections_list,
            &connections,
            &selected_ids,
            filter_dropdown.selected(),
        );

        let connections_for_filter = connections.clone();
        let list_for_filter = connections_list.clone();
        let selected_for_filter = selected_ids.clone();
        filter_dropdown.connect_selected_notify(move |dropdown| {
            repopulate_connection_rows(
                &list_for_filter,
                &connections_for_filter,
                &selected_for_filter,
                dropdown.selected(),
            );
        });

        let connections_group = adw::PreferencesGroup::new();
        connections_group.set_title("Assigned connections");
        connections_group.add(&connections_list);

        let content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        content_box.set_margin_top(12);
        content_box.set_margin_bottom(12);
        content_box.set_margin_start(12);
        content_box.set_margin_end(12);
        content_box.append(&name_entry);
        content_box.append(&filter_row);
        content_box.append(&connections_group);

        let dialog = adw::AlertDialog::builder()
            .heading(heading)
            .body("Set a profile name and choose connections for this profile")
            .extra_child(&content_box)
            .default_response("save")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("save", "Save")]);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);

        let response = if let Some(parent) = self.widget.root().and_downcast_ref::<gtk4::Window>() {
            dialog.choose_future(Some(parent)).await
        } else {
            dialog.choose_future(None::<&gtk4::Window>).await
        };

        if response.as_str() != "save" {
            return Ok(None);
        }

        let name = name_entry.text().trim().to_string();
        if name.is_empty() {
            self.show_toast("Profile name is required");
            return Ok(None);
        }

        let mut uuids: Vec<Uuid> = Vec::new();
        for uuid_text in selected_ids.borrow().iter() {
            uuids.push(profiles::parse_uuid(uuid_text)?);
        }
        uuids.sort();
        uuids.dedup();

        let active = existing
            .as_ref()
            .map(|profile| profile.active)
            .unwrap_or(false);
        Ok(Some(NetworkProfile {
            name,
            connections: uuids,
            active,
        }))
    }

    async fn show_wireguard_editor(
        &self,
        existing: Option<WireGuardConnectionConfig>,
    ) -> anyhow::Result<Option<WireGuardConnectionConfig>> {
        let name_entry = adw::EntryRow::builder().title("Connection name").build();
        let interface_entry = adw::EntryRow::builder().title("Interface name").build();
        let addresses_entry = adw::EntryRow::builder().title("Addresses").build();
        addresses_entry.set_text("10.0.0.2/32");
        let dns_entry = adw::EntryRow::builder().title("DNS servers").build();
        let private_key_entry = adw::PasswordEntryRow::builder()
            .title("Private key")
            .build();
        let public_key_entry = adw::EntryRow::builder().title("Peer public key").build();
        let endpoint_entry = adw::EntryRow::builder().title("Endpoint").build();
        let allowed_ips_entry = adw::EntryRow::builder().title("Allowed IPs").build();
        allowed_ips_entry.set_text("0.0.0.0/0, ::/0");
        let preshared_key_entry = adw::PasswordEntryRow::builder()
            .title("Preshared key")
            .build();

        let keepalive_row = adw::ActionRow::builder()
            .title("Persistent keepalive")
            .subtitle("Seconds, optional")
            .build();
        let keepalive_adjustment = gtk4::Adjustment::new(25.0, 0.0, 3600.0, 1.0, 10.0, 0.0);
        let keepalive_spin = gtk4::SpinButton::builder()
            .adjustment(&keepalive_adjustment)
            .numeric(true)
            .digits(0)
            .build();
        keepalive_row.add_suffix(&keepalive_spin);

        let mtu_row = adw::ActionRow::builder()
            .title("MTU")
            .subtitle("Optional")
            .build();
        let mtu_adjustment = gtk4::Adjustment::new(1420.0, 0.0, 9000.0, 1.0, 10.0, 0.0);
        let mtu_spin = gtk4::SpinButton::builder()
            .adjustment(&mtu_adjustment)
            .numeric(true)
            .digits(0)
            .build();
        mtu_row.add_suffix(&mtu_spin);

        if let Some(existing) = existing.as_ref() {
            name_entry.set_text(&existing.name);
            interface_entry.set_text(&existing.interface_name);
            addresses_entry.set_text(&existing.addresses.join(", "));
            dns_entry.set_text(&existing.dns_servers.join(", "));
            private_key_entry.set_text(&existing.private_key);
            public_key_entry.set_text(&existing.public_key);
            endpoint_entry.set_text(&existing.endpoint);
            allowed_ips_entry.set_text(&existing.allowed_ips.join(", "));
            preshared_key_entry.set_text(existing.preshared_key.as_deref().unwrap_or(""));
            keepalive_spin.set_value(existing.persistent_keepalive.unwrap_or_default() as f64);
            mtu_spin.set_value(existing.mtu.unwrap_or_default() as f64);
        } else {
            interface_entry.set_text("wg0");
            keepalive_spin.set_value(25.0);
        }

        let group = adw::PreferencesGroup::new();
        group.add(&name_entry);
        group.add(&interface_entry);
        group.add(&addresses_entry);
        group.add(&dns_entry);
        group.add(&private_key_entry);
        group.add(&public_key_entry);
        group.add(&endpoint_entry);
        group.add(&allowed_ips_entry);
        group.add(&preshared_key_entry);
        group.add(&keepalive_row);
        group.add(&mtu_row);

        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        body.set_margin_top(12);
        body.set_margin_bottom(12);
        body.set_margin_start(12);
        body.set_margin_end(12);
        body.append(&group);

        let dialog = adw::AlertDialog::builder()
            .heading(if existing.is_some() {
                "Edit WireGuard VPN"
            } else {
                "New WireGuard VPN"
            })
            .body("Enter the basic WireGuard interface and peer details")
            .extra_child(&body)
            .default_response("save")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("save", "Save")]);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);

        let response = if let Some(parent) = self.widget.root().and_downcast_ref::<gtk4::Window>() {
            dialog.choose_future(Some(parent)).await
        } else {
            dialog.choose_future(None::<&gtk4::Window>).await
        };

        if response.as_str() != "save" {
            return Ok(None);
        }

        let name = name_entry.text().trim().to_string();
        let interface_name = interface_entry.text().trim().to_string();
        let private_key = private_key_entry.text().trim().to_string();
        let public_key = public_key_entry.text().trim().to_string();
        let endpoint = endpoint_entry.text().trim().to_string();
        if name.is_empty()
            || interface_name.is_empty()
            || private_key.is_empty()
            || public_key.is_empty()
            || endpoint.is_empty()
        {
            self.show_toast("Name, interface, keys, and endpoint are required");
            return Ok(None);
        }

        let addresses = split_csv(addresses_entry.text().as_str());
        let allowed_ips = split_csv(allowed_ips_entry.text().as_str());
        if addresses.is_empty() || allowed_ips.is_empty() {
            self.show_toast("Addresses and allowed IPs are required");
            return Ok(None);
        }

        let keepalive = spin_value_to_option(&keepalive_spin);
        let mtu = spin_value_to_option(&mtu_spin);

        Ok(Some(WireGuardConnectionConfig {
            name,
            interface_name,
            addresses,
            dns_servers: split_csv(dns_entry.text().as_str()),
            private_key,
            public_key,
            preshared_key: optional_text(preshared_key_entry.text().as_str()),
            endpoint,
            allowed_ips,
            mtu,
            persistent_keepalive: keepalive,
        }))
    }

    async fn show_openvpn_editor(
        &self,
        existing: Option<OpenVpnConnectionConfig>,
    ) -> anyhow::Result<Option<OpenVpnConnectionConfig>> {
        let name_entry = adw::EntryRow::builder().title("Connection name").build();
        let remote_entry = adw::EntryRow::builder().title("Server / remote").build();
        let username_entry = adw::EntryRow::builder().title("Username").build();

        if let Some(existing) = existing.as_ref() {
            name_entry.set_text(&existing.name);
            remote_entry.set_text(&existing.remote);
            username_entry.set_text(existing.username.as_deref().unwrap_or(""));
        }

        let group = adw::PreferencesGroup::new();
        group.add(&name_entry);
        group.add(&remote_entry);
        group.add(&username_entry);

        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        body.set_margin_top(12);
        body.set_margin_bottom(12);
        body.set_margin_start(12);
        body.set_margin_end(12);
        body.append(&group);

        let dialog = adw::AlertDialog::builder()
            .heading(if existing.is_some() {
                "Edit OpenVPN Profile"
            } else {
                "New OpenVPN Profile"
            })
            .body("This creates a basic OpenVPN profile. Certificates and advanced options can still be imported from a .ovpn file.")
            .extra_child(&body)
            .default_response("save")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("save", "Save")]);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);

        let response = if let Some(parent) = self.widget.root().and_downcast_ref::<gtk4::Window>() {
            dialog.choose_future(Some(parent)).await
        } else {
            dialog.choose_future(None::<&gtk4::Window>).await
        };

        if response.as_str() != "save" {
            return Ok(None);
        }

        let name = name_entry.text().trim().to_string();
        let remote = remote_entry.text().trim().to_string();
        if name.is_empty() || remote.is_empty() {
            self.show_toast("Connection name and remote server are required");
            return Ok(None);
        }

        Ok(Some(OpenVpnConnectionConfig {
            name,
            remote,
            username: optional_text(username_entry.text().as_str()),
        }))
    }

    async fn confirm_active_profile_save(&self) -> bool {
        let dialog = adw::AlertDialog::builder()
            .heading("This profile is active. Save changes?")
            .default_response("save")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("save", "Save")]);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);

        let response = if let Some(parent) = self.widget.root().and_downcast_ref::<gtk4::Window>() {
            dialog.choose_future(Some(parent)).await
        } else {
            dialog.choose_future(None::<&gtk4::Window>).await
        };

        response == "save"
    }

    fn restore_profiles_snapshot(&self, snapshot: &[NetworkProfile]) {
        if let Ok(mut profiles_state) = self.profiles.try_borrow_mut() {
            *profiles_state = snapshot.to_vec();
        } else {
            log::error!("Borrow conflict in UI state");
            return;
        }
        self.populate_profiles(snapshot.to_vec());
    }

    fn show_toast(&self, message: &str) {
        common::show_toast(&self.toast_overlay, message);
    }
}

fn repopulate_connection_rows(
    list_box: &gtk4::ListBox,
    connections: &[Connection],
    selected_ids: &Rc<RefCell<HashSet<String>>>,
    filter: u32,
) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    for connection in connections {
        if !matches_connection_filter(connection, filter) {
            continue;
        }

        let row = adw::ActionRow::new();
        row.set_title(&connection.name);
        row.set_subtitle(&format!(
            "{} • {}",
            connection_type_label(connection),
            connection.uuid
        ));

        let icon = gtk4::Image::new();
        icon.set_icon_name(Some(connection_icon(connection)));
        icon.set_pixel_size(20);
        row.add_prefix(&icon);

        let checkbox = gtk4::CheckButton::new();
        checkbox.set_active(selected_ids.borrow().contains(&connection.uuid));
        row.add_suffix(&checkbox);
        row.set_activatable_widget(Some(&checkbox));

        let uuid = connection.uuid.clone();
        let selected_for_toggle = selected_ids.clone();
        checkbox.connect_toggled(move |btn| {
            if let Ok(mut selected) = selected_for_toggle.try_borrow_mut() {
                if btn.is_active() {
                    selected.insert(uuid.clone());
                } else {
                    selected.remove(&uuid);
                }
            } else {
                log::error!("Borrow conflict in UI state");
            }
        });

        list_box.append(&row);
    }
}

fn matches_connection_filter(connection: &Connection, filter: u32) -> bool {
    match filter {
        1 => matches!(connection.conn_type.as_str(), "802-11-wireless" | "wifi"),
        2 => matches!(connection.conn_type.as_str(), "802-3-ethernet" | "ethernet"),
        3 => matches!(connection.conn_type.as_str(), "wireguard" | "vpn"),
        _ => true,
    }
}

fn connection_type_label(connection: &Connection) -> &'static str {
    match connection.conn_type.as_str() {
        "802-11-wireless" | "wifi" => "Wi-Fi",
        "802-3-ethernet" | "ethernet" => "Ethernet",
        "wireguard" => "WireGuard",
        "vpn" => "OpenVPN",
        _ => "Other",
    }
}

fn connection_icon(connection: &Connection) -> &'static str {
    match connection.conn_type.as_str() {
        "802-11-wireless" | "wifi" => icon_name(
            "network-wireless-symbolic",
            &[
                "network-wireless",
                "network-wireless-signal-excellent-symbolic",
            ],
        ),
        "802-3-ethernet" | "ethernet" => icon_name(
            "network-wired-symbolic",
            &["network-wired", "network-transmit-receive-symbolic"],
        ),
        "wireguard" | "vpn" => icon_name(
            "network-vpn-symbolic",
            &[
                "network-wireless-encrypted-symbolic",
                "network-workgroup-symbolic",
            ],
        ),
        _ => icon_name("network-workgroup-symbolic", &["folder-symbolic"]),
    }
}

fn vpn_icon_name(kind: VpnKind) -> &'static str {
    match kind {
        VpnKind::WireGuard | VpnKind::OpenVpn => icon_name(
            "network-vpn-symbolic",
            &[
                "network-wireless-encrypted-symbolic",
                "network-workgroup-symbolic",
            ],
        ),
    }
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn optional_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn spin_value_to_option(spin: &gtk4::SpinButton) -> Option<u32> {
    let value = spin.value_as_int();
    if value <= 0 {
        None
    } else {
        Some(value as u32)
    }
}
