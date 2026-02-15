// File: profiles_page.rs
// Location: /src/ui/profiles_page.rs

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::{self as adw, prelude::*};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use uuid::Uuid;

use crate::nm::Connection;
use crate::profiles::{self, NetworkProfile};
use crate::ui::icon_name;

pub struct ProfilesPage {
    pub widget: gtk4::Box,
    toast_overlay: adw::ToastOverlay,
    list_box: gtk4::ListBox,
    empty_state: adw::StatusPage,
    new_button: gtk4::Button,
    refresh_button: gtk4::Button,
    profiles: Rc<RefCell<Vec<NetworkProfile>>>,
}

impl ProfilesPage {
    pub fn new() -> Self {
        let widget = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let toast_overlay = adw::ToastOverlay::new();

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .build();

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.set_margin_top(8);
        content.set_margin_bottom(8);
        content.set_margin_start(8);
        content.set_margin_end(8);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        header.set_margin_bottom(10);

        let title = gtk4::Label::builder()
            .label("Profiles")
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .build();
        title.add_css_class("title-4");

        let new_button = gtk4::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text("New profile")
            .css_classes(vec!["flat".to_string(), "circular".to_string()])
            .build();

        let refresh_button = gtk4::Button::builder()
            .icon_name(icon_name(
                "view-refresh-symbolic",
                &["view-refresh", "reload-symbolic"][..],
            ))
            .tooltip_text("Refresh profiles")
            .css_classes(vec![
                "flat".to_string(),
                "circular".to_string(),
            ])
            .build();

        header.append(&title);
        header.append(&new_button);
        header.append(&refresh_button);
        content.append(&header);

        let list_box = gtk4::ListBox::builder()
            .css_classes(vec!["boxed-list".to_string()])
            .selection_mode(gtk4::SelectionMode::None)
            .build();
        list_box.set_visible(false);

        let empty_state = adw::StatusPage::builder()
            .icon_name(icon_name(
                "network-workgroup-symbolic",
                &["folder-symbolic", "applications-system-symbolic"][..],
            ))
            .title("No Profiles")
            .description("Create a profile to manage auto-connect behavior")
            .build();
        empty_state.set_visible(true);

        content.append(&list_box);
        content.append(&empty_state);

        scrolled.set_child(Some(&content));
        toast_overlay.set_child(Some(&scrolled));
        widget.append(&toast_overlay);

        let page = Self {
            widget,
            toast_overlay,
            list_box,
            empty_state,
            new_button: new_button.clone(),
            refresh_button: refresh_button.clone(),
            profiles: Rc::new(RefCell::new(Vec::new())),
        };

        let page_ref = page.clone_ref();
        glib::spawn_future_local(async move {
            page_ref.refresh_profiles().await;
        });

        let page_ref = page.clone_ref();
        new_button.connect_clicked(move |_| {
            let page = page_ref.clone_ref();
            glib::spawn_future_local(async move {
                page.create_profile().await;
            });
        });

        let page_ref = page.clone_ref();
        refresh_button.connect_clicked(move |_| {
            let page = page_ref.clone_ref();
            glib::spawn_future_local(async move {
                page.refresh_profiles().await;
            });
        });

        page
    }

    pub fn clone_ref(&self) -> Self {
        Self {
            widget: self.widget.clone(),
            toast_overlay: self.toast_overlay.clone(),
            list_box: self.list_box.clone(),
            empty_state: self.empty_state.clone(),
            new_button: self.new_button.clone(),
            refresh_button: self.refresh_button.clone(),
            profiles: self.profiles.clone(),
        }
    }

    pub async fn refresh_profiles(&self) {
        self.refresh_button.set_sensitive(false);
        self.new_button.set_sensitive(false);
        self.list_box.add_css_class("list-loading");

        let path = profiles::profiles_path();
        match profiles::load_profiles(&path) {
            Ok(loaded) => {
                *self.profiles.borrow_mut() = loaded.clone();
                self.populate_profiles(loaded);
            }
            Err(e) => {
                self.show_toast(&format!("Failed to load profiles: {}", e));
                self.populate_profiles(Vec::new());
            }
        }

        self.list_box.remove_css_class("list-loading");
        self.refresh_button.set_sensitive(true);
        self.new_button.set_sensitive(true);
    }

    fn populate_profiles(&self, profiles_list: Vec<NetworkProfile>) {
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        if profiles_list.is_empty() {
            self.list_box.set_visible(false);
            self.empty_state.set_visible(true);
            return;
        }

        for profile in profiles_list {
            let row = self.create_profile_row(&profile);
            self.list_box.append(&row);
        }

        self.empty_state.set_visible(false);
        self.list_box.set_visible(true);
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

        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);

        let activate_btn = gtk4::Button::builder()
            .icon_name(if profile.active {
                "emblem-ok-symbolic"
            } else {
                "media-playback-start-symbolic"
            })
            .tooltip_text(if profile.active {
                "Profile is active"
            } else {
                "Activate profile"
            })
            .css_classes(vec!["flat".to_string(), "circular".to_string()])
            .sensitive(!profile.active)
            .build();

        let edit_btn = gtk4::Button::builder()
            .icon_name("document-edit-symbolic")
            .tooltip_text("Edit profile")
            .css_classes(vec!["flat".to_string(), "circular".to_string()])
            .build();

        let delete_btn = gtk4::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text("Delete profile")
            .css_classes(vec![
                "flat".to_string(),
                "circular".to_string(),
                "destructive-action".to_string(),
            ])
            .build();

        actions.append(&activate_btn);
        actions.append(&edit_btn);
        actions.append(&delete_btn);
        row.add_suffix(&actions);
        row.set_activatable_widget(Some(&activate_btn));

        let page_activate = self.clone_ref();
        let profile_name_activate = profile.name.clone();
        activate_btn.connect_clicked(move |_| {
            let page = page_activate.clone_ref();
            let profile_name = profile_name_activate.clone();
            glib::spawn_future_local(async move {
                page.activate_profile(&profile_name).await;
            });
        });

        let page_edit = self.clone_ref();
        let profile_name_edit = profile.name.clone();
        edit_btn.connect_clicked(move |_| {
            let page = page_edit.clone_ref();
            let profile_name = profile_name_edit.clone();
            glib::spawn_future_local(async move {
                page.edit_profile(&profile_name).await;
            });
        });

        let page_delete = self.clone_ref();
        let profile_name_delete = profile.name.clone();
        delete_btn.connect_clicked(move |_| {
            let page = page_delete.clone_ref();
            let profile_name = profile_name_delete.clone();
            glib::spawn_future_local(async move {
                page.delete_profile(&profile_name).await;
            });
        });

        if !profile.active {
            let page_row = self.clone_ref();
            let profile_name_row = profile.name.clone();
            row.set_activatable(true);
            row.connect_activated(move |_| {
                let page = page_row.clone_ref();
                let profile_name = profile_name_row.clone();
                glib::spawn_future_local(async move {
                    page.activate_profile(&profile_name).await;
                });
            });
        }

        row
    }

    async fn create_profile(&self) {
        match self.show_profile_editor(None).await {
            Ok(Some(profile)) => {
                let mut current = self.profiles.borrow().clone();
                if current
                    .iter()
                    .any(|p| p.name.eq_ignore_ascii_case(&profile.name))
                {
                    self.show_toast("Profile name already exists");
                    return;
                }
                current.push(profile);
                let path = profiles::profiles_path();
                if let Err(e) = profiles::save_profiles(&path, &current) {
                    self.show_toast(&format!("Failed to save profiles: {}", e));
                    return;
                }
                self.refresh_profiles().await;
            }
            Ok(std::prelude::v1::None) => {}
            Err(e) => self.show_toast(&format!("Failed to create profile: {}", e)),
        }
    }

    async fn edit_profile(&self, profile_name: &str) {
        let existing = self
            .profiles
            .borrow()
            .iter()
            .find(|p| p.name == profile_name)
            .cloned();

        let Some(existing_profile) = existing else {
            self.show_toast("Profile not found");
            return;
        };

        match self.show_profile_editor(Some(existing_profile.clone())).await {
            Ok(Some(updated)) => {
                let mut current = self.profiles.borrow().clone();
                if current.iter().any(|p| {
                    p.name != profile_name && p.name.eq_ignore_ascii_case(&updated.name)
                }) {
                    self.show_toast("Profile name already exists");
                    return;
                }

                if let Some(slot) = current.iter_mut().find(|p| p.name == profile_name) {
                    let was_active = slot.active;
                    *slot = updated;
                    slot.active = was_active;
                }

                let path = profiles::profiles_path();
                if let Err(e) = profiles::save_profiles(&path, &current) {
                    self.show_toast(&format!("Failed to save profiles: {}", e));
                    return;
                }

                self.refresh_profiles().await;
            }
            Ok(std::prelude::v1::None) => {}
            Err(e) => self.show_toast(&format!("Failed to edit profile: {}", e)),
        }
    }

    async fn delete_profile(&self, profile_name: &str) {
        let dialog = adw::AlertDialog::builder()
            .heading("Delete Profile?")
            .body(format!("This will delete profile \"{}\".", profile_name))
            .default_response("delete")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete")][..]);
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
        current.retain(|p| p.name != profile_name);
        let path = profiles::profiles_path();
        if let Err(e) = profiles::save_profiles(&path, &current) {
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
            Err(e) => self.show_toast(&format!("Failed to activate profile: {}", e)),
        }
    }

    async fn show_profile_editor(&self, existing: Option<NetworkProfile>) -> anyhow::Result<Option<NetworkProfile>> {
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
            for uuid in &profile.connections {
                selected_ids.borrow_mut().insert(uuid.to_string());
            }
        }

        let filter_model = gtk4::StringList::new(&["All", "Wi-Fi", "Ethernet"][..]);
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
        dialog.add_responses(&[("cancel", "Cancel"), ("save", "Save")][..]);
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

        let active = existing.as_ref().map(|p| p.active).unwrap_or(false);
        Ok(Some(NetworkProfile {
            name,
            connections: uuids,
            active,
        }))
    }

    fn show_toast(&self, message: &str) {
        let toast = adw::Toast::new(message);
        self.toast_overlay.add_toast(toast);
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
            if btn.is_active() {
                selected_for_toggle.borrow_mut().insert(uuid.clone());
            } else {
                selected_for_toggle.borrow_mut().remove(&uuid);
            }
        });

        list_box.append(&row);
    }
}

fn matches_connection_filter(connection: &Connection, filter: u32) -> bool {
    match filter {
        1 => matches!(connection.conn_type.as_str(), "802-11-wireless" | "wifi"),
        2 => matches!(connection.conn_type.as_str(), "802-3-ethernet" | "ethernet"),
        _ => true,
    }
}

fn connection_type_label(connection: &Connection) -> &'static str {
    match connection.conn_type.as_str() {
        "802-11-wireless" | "wifi" => "Wi-Fi",
        "802-3-ethernet" | "ethernet" => "Ethernet",
        _ => "Other",
    }
}

fn connection_icon(connection: &Connection) -> &'static str {
    match connection.conn_type.as_str() {
        "802-11-wireless" | "wifi" => icon_name(
            "network-wireless-symbolic",
            &["network-wireless", "network-wireless-signal-excellent-symbolic"][..],
        ),
        "802-3-ethernet" | "ethernet" => icon_name(
            "network-wired-symbolic",
            &["network-wired", "network-transmit-receive-symbolic"][..],
        ),
        _ => icon_name("network-workgroup-symbolic", &["folder-symbolic"][..]),
    }
}
