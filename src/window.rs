// * Credits and inspirations:
// * GNOME Settings Network panel for UI patterns.
// * airctl for the clean visual direction.

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::{self as adw, prelude::*};
use std::cell::{Cell, RefCell};
use std::fs;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::config;
use crate::hotspot;
use crate::nm;
use crate::state::AppState;
use crate::ui::{
    common, devices_page::DevicesPage, ethernet_page::EthernetPage, hotspot_page::HotspotPage,
    icon_name, profiles_page::ProfilesPage, wifi_page::WifiPage,
};

pub struct AppPrefs {
    pub auto_scan: bool,
    pub expand_connected_details: bool,
    pub icons_only_navigation: bool,
}

impl Default for AppPrefs {
    fn default() -> Self {
        Self {
            auto_scan: true,
            expand_connected_details: false,
            icons_only_navigation: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ModuleAvailability {
    wifi: bool,
    ethernet: bool,
    hotspot: bool,
    devices: bool,
    profiles: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct ModuleFlags {
    wifi: bool,
    ethernet: bool,
    hotspot: bool,
    devices: bool,
    profiles: bool,
}

impl ModuleFlags {
    fn any_visible(self) -> bool {
        self.wifi || self.ethernet || self.hotspot || self.devices || self.profiles
    }
}

#[derive(Debug, Clone)]
struct ModuleLayoutState {
    customized: bool,
    visible: ModuleFlags,
    order: Vec<ModuleKind>,
}

impl ModuleLayoutState {
    fn from_settings(settings: &config::AppSettings) -> Self {
        Self {
            customized: settings.module_layout_customized,
            visible: ModuleFlags {
                wifi: settings.show_wifi_module,
                ethernet: settings.show_ethernet_module,
                hotspot: settings.show_hotspot_module,
                devices: settings.show_devices_module,
                profiles: settings.show_profiles_module,
            },
            order: Self::order_from_settings(settings),
        }
    }

    fn apply_to_settings(&self, settings: &mut config::AppSettings) {
        settings.module_layout_customized = self.customized;
        settings.show_wifi_module = self.visible.wifi;
        settings.show_ethernet_module = self.visible.ethernet;
        settings.show_hotspot_module = self.visible.hotspot;
        settings.show_devices_module = self.visible.devices;
        settings.show_profiles_module = self.visible.profiles;
        settings.module_order = self
            .order
            .iter()
            .map(|kind| kind.label().to_string())
            .collect();
    }

    fn default_visible(availability: ModuleAvailability) -> ModuleFlags {
        if availability.ethernet {
            ModuleFlags {
                ethernet: true,
                wifi: false,
                hotspot: false,
                devices: false,
                profiles: true,
            }
        } else {
            ModuleFlags {
                ethernet: false,
                wifi: availability.wifi,
                hotspot: false,
                devices: false,
                profiles: true,
            }
        }
    }

    fn resolve_visible(&self, availability: ModuleAvailability) -> ModuleFlags {
        let resolved = if self.customized {
            ModuleFlags {
                wifi: self.visible.wifi && availability.wifi,
                ethernet: self.visible.ethernet && availability.ethernet,
                hotspot: self.visible.hotspot && availability.hotspot,
                devices: self.visible.devices && availability.devices,
                profiles: self.visible.profiles && availability.profiles,
            }
        } else {
            Self::default_visible(availability)
        };

        if resolved.any_visible() {
            resolved
        } else {
            Self::default_visible(availability)
        }
    }

    fn ordered_visible_modules(&self, visible: ModuleFlags) -> Vec<ModuleKind> {
        self.order
            .iter()
            .copied()
            .filter(|kind| kind.is_visible(visible))
            .collect()
    }

    fn order_from_settings(settings: &config::AppSettings) -> Vec<ModuleKind> {
        let mut order: Vec<ModuleKind> = settings
            .module_order
            .iter()
            .filter_map(|item| ModuleKind::from_label(item))
            .collect();
        for kind in ModuleKind::ORDER {
            if !order.contains(&kind) {
                order.push(kind);
            }
        }
        order
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModuleKind {
    Wifi,
    Ethernet,
    Hotspot,
    Device,
    Profiles,
}

impl ModuleKind {
    const ORDER: [Self; 5] = [
        Self::Wifi,
        Self::Ethernet,
        Self::Hotspot,
        Self::Device,
        Self::Profiles,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Ethernet => "Ethernet",
            Self::Wifi => "Wi-Fi",
            Self::Hotspot => "Hotspot",
            Self::Device => "Devices",
            Self::Profiles => "Profiles",
        }
    }

    fn from_label(label: &str) -> Option<Self> {
        match label {
            "Ethernet" => Some(Self::Ethernet),
            "Wi-Fi" => Some(Self::Wifi),
            "Hotspot" => Some(Self::Hotspot),
            "Device" | "Devices" => Some(Self::Device),
            "Profiles" => Some(Self::Profiles),
            _ => None,
        }
    }

    fn dnd_id(self) -> &'static str {
        match self {
            Self::Ethernet => "ethernet",
            Self::Wifi => "wifi",
            Self::Hotspot => "hotspot",
            Self::Device => "devices",
            Self::Profiles => "profiles",
        }
    }

    fn from_dnd_id(value: &str) -> Option<Self> {
        match value {
            "ethernet" => Some(Self::Ethernet),
            "wifi" => Some(Self::Wifi),
            "hotspot" => Some(Self::Hotspot),
            "devices" => Some(Self::Device),
            "profiles" => Some(Self::Profiles),
            _ => None,
        }
    }

    fn is_available(self, availability: ModuleAvailability) -> bool {
        match self {
            Self::Wifi => availability.wifi,
            Self::Ethernet => availability.ethernet,
            Self::Hotspot => availability.hotspot,
            Self::Device => availability.devices,
            Self::Profiles => availability.profiles,
        }
    }

    fn is_visible(self, visible: ModuleFlags) -> bool {
        match self {
            Self::Ethernet => visible.ethernet,
            Self::Wifi => visible.wifi,
            Self::Hotspot => visible.hotspot,
            Self::Device => visible.devices,
            Self::Profiles => visible.profiles,
        }
    }

    fn set_visible(self, visible: &mut ModuleFlags, value: bool) {
        match self {
            Self::Ethernet => visible.ethernet = value,
            Self::Wifi => visible.wifi = value,
            Self::Hotspot => visible.hotspot = value,
            Self::Device => visible.devices = value,
            Self::Profiles => visible.profiles = value,
        }
    }
}

pub struct AdwNetworkWindow {
    pub window: adw::ApplicationWindow,
}

impl AdwNetworkWindow {
    pub fn new(app: &adw::Application) -> Self {
        Self::load_css();
        Self::load_saved_theme();

        let settings_path = config::app_settings_path();
        let (app_settings, settings_repaired) =
            config::load_app_settings_with_status(&settings_path)
                .unwrap_or_else(|_| (config::AppSettings::default(), false));
        if settings_repaired {
            if let Err(e) = config::save_app_settings(&settings_path, &app_settings) {
                log::warn!("Failed to persist repaired app settings: {}", e);
            }
        }
        let prefs = Rc::new(RefCell::new(AppPrefs {
            auto_scan: app_settings.auto_scan,
            expand_connected_details: app_settings.expand_connected_details,
            icons_only_navigation: app_settings.icons_only_navigation,
        }));
        let app_state = AppState::new(&app_settings);

        let wifi_page = WifiPage::new(app_state.clone());
        let ethernet_page = EthernetPage::new();
        let hotspot_page = HotspotPage::new(app_state.clone());
        let devices_page = DevicesPage::new(app_state.clone());
        let profiles_page = ProfilesPage::new();

        let view_stack = adw::ViewStack::new();
        // Keep minimum width tied to the visible page, not the widest hidden page.
        view_stack.set_hhomogeneous(false);
        view_stack.set_vhomogeneous(false);
        let wifi_page_for_visibility = wifi_page.clone();
        let hotspot_page_for_visibility = hotspot_page.clone();
        let devices_page_for_visibility = devices_page.clone();
        view_stack.connect_visible_child_notify(move |stack| {
            if let Some(child) = stack.visible_child() {
                child.add_css_class("fade-in");
                let child_clone = child.clone();
                glib::timeout_add_local(Duration::from_millis(260), move || {
                    child_clone.remove_css_class("fade-in");
                    glib::ControlFlow::Break
                });

                let page_name = stack
                    .visible_child_name()
                    .map(|name| name.to_string())
                    .unwrap_or_default();
                wifi_page_for_visibility.set_page_visible(page_name == "wifi");
                hotspot_page_for_visibility.set_page_visible(page_name == "hotspot");
                devices_page_for_visibility.set_page_visible(page_name == "devices");
            }
        });
        let mut wifi_stack_page = None;
        let mut ethernet_stack_page = None;
        let mut hotspot_stack_page = None;
        let mut devices_stack_page = None;
        let mut profiles_stack_page = None;

        for kind in ModuleLayoutState::order_from_settings(&app_settings) {
            match kind {
                ModuleKind::Wifi => {
                    let page = view_stack.add_titled(&wifi_page.widget, Some("wifi"), "Wi-Fi");
                    page.set_icon_name(Some(icon_name(
                        "network-wireless-symbolic",
                        &[
                            "network-wireless-signal-excellent-symbolic",
                            "network-wireless",
                        ][..],
                    )));
                    wifi_stack_page = Some(page);
                }
                ModuleKind::Ethernet => {
                    let page =
                        view_stack.add_titled(&ethernet_page.widget, Some("ethernet"), "Ethernet");
                    page.set_icon_name(Some(icon_name(
                        "network-wired-symbolic",
                        &["network-wired", "network-transmit-receive-symbolic"][..],
                    )));
                    ethernet_stack_page = Some(page);
                }
                ModuleKind::Hotspot => {
                    let page =
                        view_stack.add_titled(&hotspot_page.widget, Some("hotspot"), "Hotspot");
                    page.set_icon_name(Some(icon_name(
                        "network-wireless-hotspot-symbolic",
                        &["network-wireless-symbolic", "network-wireless"][..],
                    )));
                    hotspot_stack_page = Some(page);
                }
                ModuleKind::Device => {
                    let page =
                        view_stack.add_titled(&devices_page.widget, Some("devices"), "Devices");
                    page.set_icon_name(Some(icon_name(
                        "computer-symbolic",
                        &["network-workgroup-symbolic", "computer"][..],
                    )));
                    devices_stack_page = Some(page);
                }
                ModuleKind::Profiles => {
                    let page =
                        view_stack.add_titled(&profiles_page.widget, Some("profiles"), "Profiles");
                    page.set_icon_name(Some(icon_name(
                        "network-workgroup-symbolic",
                        &["folder-symbolic", "applications-system-symbolic"][..],
                    )));
                    profiles_stack_page = Some(page);
                }
            }
        }

        let wifi_stack_page = wifi_stack_page.expect("view stack must contain Wi-Fi page");
        let ethernet_stack_page =
            ethernet_stack_page.expect("view stack must contain Ethernet page");
        let hotspot_stack_page = hotspot_stack_page.expect("view stack must contain Hotspot page");
        let devices_stack_page = devices_stack_page.expect("view stack must contain Devices page");
        let profiles_stack_page =
            profiles_stack_page.expect("view stack must contain Profiles page");

        let view_switcher = adw::ViewSwitcher::builder()
            .stack(&view_stack)
            .policy(adw::ViewSwitcherPolicy::Wide)
            .build();
        view_switcher.set_tooltip_text(Some("Mod: right-click to edit"));
        Self::apply_navigation_mode(
            &wifi_stack_page,
            &ethernet_stack_page,
            &hotspot_stack_page,
            &devices_stack_page,
            &profiles_stack_page,
            &view_switcher,
            app_settings.icons_only_navigation,
        );

        let module_layout_state = Rc::new(RefCell::new(ModuleLayoutState::from_settings(
            &app_settings,
        )));
        let module_availability_state = Rc::new(RefCell::new(ModuleAvailability::default()));
        let nav_stack = gtk4::Stack::new();
        // Avoid hidden edit controls forcing a wider minimum header width.
        nav_stack.set_hhomogeneous(false);
        nav_stack.set_vhomogeneous(false);
        nav_stack.set_halign(gtk4::Align::Center);
        nav_stack.set_hexpand(false);
        nav_stack.add_named(&view_switcher, Some("normal"));

        let edit_mode = Rc::new(Cell::new(false));
        let edit_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        edit_row.add_css_class("mod-editor-inline");
        edit_row.add_css_class("navigation-editor");
        edit_row.set_halign(gtk4::Align::Center);
        edit_row.set_hexpand(false);

        let edit_modules_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        edit_modules_box.add_css_class("navigation-editor-list");
        edit_modules_box.set_halign(gtk4::Align::Center);
        let done_edit_btn = gtk4::Button::with_label("Done");
        done_edit_btn.add_css_class("flat");
        done_edit_btn.add_css_class("nav-rect-button");
        let reset_layout_btn = gtk4::Button::builder()
            .icon_name(icon_name(
                "view-refresh-symbolic",
                &["view-refresh", "reload-symbolic"][..],
            ))
            .tooltip_text("Reset modules to defaults")
            .css_classes(vec![
                "flat".to_string(),
                "touch-target".to_string(),
                "nav-rect-button".to_string(),
            ])
            .build();
        let add_module_btn = gtk4::Button::with_label("Add");
        add_module_btn.add_css_class("suggested-action");
        add_module_btn.add_css_class("nav-rect-button");
        let add_module_popover = gtk4::Popover::builder()
            .has_arrow(true)
            .autohide(true)
            .build();
        add_module_popover.set_parent(&add_module_btn);
        let add_module_popover_for_btn = add_module_popover.clone();
        add_module_btn.connect_clicked(move |_| {
            add_module_popover_for_btn.popup();
        });
        edit_row.append(&edit_modules_box);
        edit_row.append(&reset_layout_btn);
        edit_row.append(&add_module_btn);
        edit_row.append(&done_edit_btn);
        nav_stack.add_named(&edit_row, Some("edit"));
        nav_stack.set_visible_child_name("normal");

        let nav_stack_for_done = nav_stack.clone();
        let edit_mode_for_done = edit_mode.clone();
        done_edit_btn.connect_clicked(move |_| {
            edit_mode_for_done.set(false);
            nav_stack_for_done.set_visible_child_name("normal");
        });

        let nav_stack_for_menu = nav_stack.clone();
        let edit_mode_for_menu = edit_mode.clone();
        let view_switcher_for_menu = view_switcher.clone();
        let mod_menu_click = gtk4::GestureClick::new();
        mod_menu_click.set_button(3);
        mod_menu_click.connect_pressed(move |_, _, x, y| {
            let menu_popover = gtk4::Popover::builder()
                .has_arrow(true)
                .autohide(true)
                .build();
            menu_popover.set_parent(&view_switcher_for_menu);
            menu_popover
                .set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));

            let menu_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            let edit_button = gtk4::Button::with_label("Edit");
            edit_button.add_css_class("flat");
            menu_box.append(&edit_button);
            menu_popover.set_child(Some(&menu_box));

            let popover_for_button = menu_popover.clone();
            let nav_stack_for_button = nav_stack_for_menu.clone();
            let edit_mode_for_button = edit_mode_for_menu.clone();
            edit_button.connect_clicked(move |_| {
                popover_for_button.popdown();
                edit_mode_for_button.set(true);
                nav_stack_for_button.set_visible_child_name("edit");
            });

            menu_popover.popup();
        });
        view_switcher.add_controller(mod_menu_click);

        let wifi_page_ref = wifi_stack_page.clone();
        let hotspot_page_ref = hotspot_stack_page.clone();
        let ethernet_page_ref = ethernet_stack_page.clone();
        let devices_page_ref = devices_stack_page.clone();
        let profiles_page_ref = profiles_stack_page.clone();
        let view_stack_ref = view_stack.clone();
        let module_layout_for_visibility = module_layout_state.clone();
        let module_availability_for_visibility = module_availability_state.clone();
        let edit_modules_box_for_visibility = edit_modules_box.clone();
        let reset_layout_btn_for_visibility = reset_layout_btn.clone();
        let add_module_btn_for_visibility = add_module_btn.clone();
        let add_module_popover_for_visibility = add_module_popover.clone();
        let update_visibility = move || {
            let wifi_page_ref = wifi_page_ref.clone();
            let hotspot_page_ref = hotspot_page_ref.clone();
            let ethernet_page_ref = ethernet_page_ref.clone();
            let devices_page_ref = devices_page_ref.clone();
            let profiles_page_ref = profiles_page_ref.clone();
            let view_stack_ref = view_stack_ref.clone();
            let module_layout_for_visibility = module_layout_for_visibility.clone();
            let module_availability_for_visibility = module_availability_for_visibility.clone();
            let edit_modules_box_for_visibility = edit_modules_box_for_visibility.clone();
            let reset_layout_btn_for_visibility = reset_layout_btn_for_visibility.clone();
            let add_module_btn_for_visibility = add_module_btn_for_visibility.clone();
            let add_module_popover_for_visibility = add_module_popover_for_visibility.clone();

            glib::spawn_future_local(async move {
                let availability = Self::detect_module_availability().await;
                if let Ok(mut state) = module_availability_for_visibility.try_borrow_mut() {
                    *state = availability;
                }

                let layout = module_layout_for_visibility.borrow().clone();
                let resolved = layout.resolve_visible(availability);
                Self::apply_module_order(
                    &view_stack_ref,
                    &wifi_page_ref,
                    &ethernet_page_ref,
                    &hotspot_page_ref,
                    &devices_page_ref,
                    &profiles_page_ref,
                    &layout.order,
                );
                Self::apply_module_visibility(
                    &wifi_page_ref,
                    &ethernet_page_ref,
                    &hotspot_page_ref,
                    &devices_page_ref,
                    &profiles_page_ref,
                    &view_stack_ref,
                    resolved,
                );
                Self::render_inline_module_editor(
                    &edit_modules_box_for_visibility,
                    &add_module_btn_for_visibility,
                    &add_module_popover_for_visibility,
                    module_layout_for_visibility.clone(),
                    availability,
                    &view_stack_ref,
                    &wifi_page_ref,
                    &ethernet_page_ref,
                    &hotspot_page_ref,
                    &devices_page_ref,
                    &profiles_page_ref,
                );

                let layout = module_layout_for_visibility.borrow().clone();
                let is_default = !layout.customized && layout.order == ModuleKind::ORDER.to_vec();
                reset_layout_btn_for_visibility.set_sensitive(!is_default);
            });
        };

        let module_layout_for_reset = module_layout_state.clone();
        let module_availability_for_reset = module_availability_state.clone();
        let view_stack_for_reset = view_stack.clone();
        let wifi_page_for_reset = wifi_stack_page.clone();
        let ethernet_page_for_reset = ethernet_stack_page.clone();
        let hotspot_page_for_reset = hotspot_stack_page.clone();
        let devices_page_for_reset = devices_stack_page.clone();
        let profiles_page_for_reset = profiles_stack_page.clone();
        let edit_modules_box_for_reset = edit_modules_box.clone();
        let add_module_btn_for_reset = add_module_btn.clone();
        let add_module_popover_for_reset = add_module_popover.clone();
        reset_layout_btn.connect_clicked(move |_| {
            let module_layout_for_reset = module_layout_for_reset.clone();
            let module_availability_for_reset = module_availability_for_reset.clone();
            let view_stack_for_reset = view_stack_for_reset.clone();
            let wifi_page_for_reset = wifi_page_for_reset.clone();
            let ethernet_page_for_reset = ethernet_page_for_reset.clone();
            let hotspot_page_for_reset = hotspot_page_for_reset.clone();
            let devices_page_for_reset = devices_page_for_reset.clone();
            let profiles_page_for_reset = profiles_page_for_reset.clone();
            let edit_modules_box_for_reset = edit_modules_box_for_reset.clone();
            let add_module_btn_for_reset = add_module_btn_for_reset.clone();
            let add_module_popover_for_reset = add_module_popover_for_reset.clone();
            glib::spawn_future_local(async move {
                let availability = Self::detect_module_availability().await;
                if let Ok(mut availability_state) = module_availability_for_reset.try_borrow_mut() {
                    *availability_state = availability;
                }

                let mut changed = false;
                if let Ok(mut layout) = module_layout_for_reset.try_borrow_mut() {
                    layout.customized = false;
                    layout.visible = ModuleLayoutState::default_visible(availability);
                    layout.order = ModuleKind::ORDER.to_vec();
                    Self::persist_module_layout(layout.clone());
                    let resolved = layout.resolve_visible(availability);
                    Self::apply_module_order(
                        &view_stack_for_reset,
                        &wifi_page_for_reset,
                        &ethernet_page_for_reset,
                        &hotspot_page_for_reset,
                        &devices_page_for_reset,
                        &profiles_page_for_reset,
                        &layout.order,
                    );
                    Self::apply_module_visibility(
                        &wifi_page_for_reset,
                        &ethernet_page_for_reset,
                        &hotspot_page_for_reset,
                        &devices_page_for_reset,
                        &profiles_page_for_reset,
                        &view_stack_for_reset,
                        resolved,
                    );
                    changed = true;
                }

                if changed {
                    Self::render_inline_module_editor(
                        &edit_modules_box_for_reset,
                        &add_module_btn_for_reset,
                        &add_module_popover_for_reset,
                        module_layout_for_reset.clone(),
                        availability,
                        &view_stack_for_reset,
                        &wifi_page_for_reset,
                        &ethernet_page_for_reset,
                        &hotspot_page_for_reset,
                        &devices_page_for_reset,
                        &profiles_page_for_reset,
                    );
                }
            });
        });

        update_visibility();
        glib::timeout_add_seconds_local(3, move || {
            update_visibility();
            glib::ControlFlow::Continue
        });

        let current_name = view_stack
            .visible_child_name()
            .map(|name| name.to_string())
            .unwrap_or_default();
        wifi_page.set_page_visible(current_name == "wifi");
        hotspot_page.set_page_visible(current_name == "hotspot");
        devices_page.set_page_visible(current_name == "devices");

        // Global connection status header
        let status_icon = gtk4::Image::from_icon_name(icon_name(
            "network-wireless-symbolic",
            &[
                "network-wireless-signal-excellent-symbolic",
                "network-wireless",
            ][..],
        ));
        status_icon.set_pixel_size(14);

        let status_label = gtk4::Label::new(Some("Checking status…"));
        status_label.add_css_class("status-text");
        status_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        status_label.set_single_line_mode(true);
        status_label.set_max_width_chars(24);

        let status_pill = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        status_pill.add_css_class("status-pill");
        status_pill.append(&status_icon);
        status_pill.append(&status_label);
        status_pill.set_tooltip_text(Some("Connection status"));

        let speed_down_label = gtk4::Label::new(Some("↓ 0 KB/s"));
        speed_down_label.add_css_class("status-speed-text");
        let speed_up_label = gtk4::Label::new(Some("↑ 0 KB/s"));
        speed_up_label.add_css_class("status-speed-text");
        let speed_sep_label = gtk4::Label::new(Some("|"));
        speed_sep_label.add_css_class("status-speed-sep");

        let speed_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        speed_box.add_css_class("status-speed-box");
        speed_box.append(&speed_down_label);
        speed_box.append(&speed_sep_label);
        speed_box.append(&speed_up_label);
        speed_box.set_halign(gtk4::Align::Center);

        let title_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        title_box.add_css_class("header-title");
        title_box.set_halign(gtk4::Align::Center);
        title_box.set_hexpand(false);
        status_pill.set_halign(gtk4::Align::Center);
        speed_box.set_halign(gtk4::Align::Center);
        view_switcher.set_hexpand(false);
        view_switcher.set_halign(gtk4::Align::Center);
        title_box.append(&status_pill);
        title_box.append(&speed_box);
        title_box.append(&nav_stack);

        let menu_button = gtk4::MenuButton::builder()
            .icon_name("emblem-system-symbolic")
            .tooltip_text("Settings & About")
            .build();
        menu_button.add_css_class("menu-button");
        menu_button.add_css_class("header-mini-button");
        menu_button.set_size_request(16, 16);
        menu_button.set_valign(gtk4::Align::Center);

        let menu = gio::Menu::new();
        menu.append(Some("Settings"), Some("app.settings"));
        menu.append(Some("About"), Some("app.about"));
        menu_button.set_menu_model(Some(&menu));

        let header = adw::HeaderBar::builder().title_widget(&title_box).build();
        header.set_centering_policy(adw::CenteringPolicy::Strict);

        // Restore default window close control.
        header.set_decoration_layout(Some(":close"));

        // Keep settings near the close button on the right.
        let right_controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        right_controls.append(&menu_button);

        // Add buttons to the header
        header.pack_end(&right_controls);

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_content(Some(&view_stack));

        // Periodically update the global connection status
        let status_icon_for_updates = status_icon.clone();
        let status_label_for_updates = status_label.clone();
        let status_pill_for_updates = status_pill.clone();
        let update_status = move || {
            let status_icon = status_icon_for_updates.clone();
            let status_label = status_label_for_updates.clone();
            let status_pill = status_pill_for_updates.clone();

            glib::spawn_future_local(async move {
                status_pill.remove_css_class("status-online");
                status_pill.remove_css_class("status-offline");
                status_pill.remove_css_class("status-hotspot");

                if hotspot::is_hotspot_active().await.unwrap_or(false) {
                    let ssid = config::load_config(&config::hotspot_config_path())
                        .ok()
                        .map(|c| c.ssid);
                    status_icon.set_icon_name(Some(icon_name(
                        "network-wireless-hotspot-symbolic",
                        &["network-wireless-symbolic", "network-wireless"][..],
                    )));
                    status_label.set_text("Hotspot active");
                    if let Some(ssid) = ssid {
                        status_pill.set_tooltip_text(Some(&format!("Hotspot: {}", ssid)));
                    } else {
                        status_pill.set_tooltip_text(Some("Hotspot active"));
                    }
                    status_pill.add_css_class("status-hotspot");
                    return;
                }

                match nm::get_active_wired_connection().await {
                    Ok(Some(conn_name)) => {
                        let connectivity = nm::get_internet_connectivity()
                            .await
                            .unwrap_or(nm::InternetConnectivity::Unknown);
                        let (suffix, css_class) = match connectivity {
                            nm::InternetConnectivity::Full => ("", "status-online"),
                            nm::InternetConnectivity::NoInternet => {
                                (" • No internet", "status-offline")
                            }
                            nm::InternetConnectivity::Portal => {
                                (" • Login required", "status-offline")
                            }
                            nm::InternetConnectivity::Limited => (" • Limited", "status-offline"),
                            nm::InternetConnectivity::Unknown => {
                                (" • Checking internet", "status-offline")
                            }
                        };
                        status_icon.set_icon_name(Some(icon_name(
                            "network-wired-symbolic",
                            &["network-wired", "network-transmit-receive-symbolic"][..],
                        )));
                        status_label.set_text(&format!("Connected (Wired){suffix}"));
                        status_pill.set_tooltip_text(Some(&format!(
                            "Wired connection: {} • {}",
                            conn_name,
                            connectivity.as_label()
                        )));
                        status_pill.add_css_class(css_class);
                        return;
                    }
                    Ok(std::prelude::v1::None) => {}
                    Err(e) => {
                        log::warn!("Failed to update wired status: {}", e);
                    }
                }

                match nm::get_active_wifi_ssid().await {
                    Ok(Some(ssid)) => {
                        let connectivity = nm::get_internet_connectivity()
                            .await
                            .unwrap_or(nm::InternetConnectivity::Unknown);
                        let (suffix, css_class) = match connectivity {
                            nm::InternetConnectivity::Full => ("", "status-online"),
                            nm::InternetConnectivity::NoInternet => {
                                (" • No internet", "status-offline")
                            }
                            nm::InternetConnectivity::Portal => {
                                (" • Login required", "status-offline")
                            }
                            nm::InternetConnectivity::Limited => (" • Limited", "status-offline"),
                            nm::InternetConnectivity::Unknown => {
                                (" • Checking internet", "status-offline")
                            }
                        };
                        status_icon.set_icon_name(Some(icon_name(
                            "network-wireless-signal-excellent-symbolic",
                            &["network-wireless-symbolic", "network-wireless"][..],
                        )));
                        status_label.set_text(&format!("{ssid}{suffix}"));
                        status_pill.set_tooltip_text(Some(&format!(
                            "Connected to {} • {}",
                            ssid,
                            connectivity.as_label()
                        )));
                        status_pill.add_css_class(css_class);
                    }
                    Ok(std::prelude::v1::None) => {
                        let wifi_enabled = nm::is_wifi_enabled().await.unwrap_or(false);
                        if wifi_enabled {
                            status_icon.set_icon_name(Some(icon_name(
                                "network-wireless-offline-symbolic",
                                &["network-wireless-symbolic", "network-wireless"][..],
                            )));
                            status_label.set_text("Not connected");
                            status_pill.set_tooltip_text(Some("Not connected"));
                        } else {
                            status_icon.set_icon_name(Some(icon_name(
                                "network-wireless-disabled-symbolic",
                                &["network-wireless-offline-symbolic", "network-wireless"][..],
                            )));
                            status_label.set_text("Not connected");
                            status_pill.set_tooltip_text(Some("Wi-Fi off"));
                        }
                        status_pill.add_css_class("status-offline");
                    }
                    Err(e) => {
                        log::warn!("Failed to update connection status: {}", e);
                        status_label.set_text("Status unavailable");
                        status_pill.set_tooltip_text(Some("Status unavailable"));
                        status_pill.add_css_class("status-offline");
                    }
                }
            });

            glib::ControlFlow::Continue
        };
        update_status();
        glib::timeout_add_seconds_local(5, update_status);

        let speed_state = Arc::new(Mutex::new((0u64, 0u64)));
        let speed_state_ui = Arc::clone(&speed_state);
        let speed_down_label = speed_down_label.clone();
        let speed_up_label = speed_up_label.clone();
        glib::timeout_add_seconds_local(1, move || {
            let (down_bytes, up_bytes) = speed_state_ui.lock().map(|v| *v).unwrap_or((0, 0));
            speed_down_label.set_text(&format!("↓ {}", format_speed(down_bytes)));
            speed_up_label.set_text(&format!("↑ {}", format_speed(up_bytes)));
            glib::ControlFlow::Continue
        });

        let speed_state_task = Arc::clone(&speed_state);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            let mut last_iface: Option<String> = None;
            let mut last_rx: Option<u64> = None;
            let mut last_tx: Option<u64> = None;

            loop {
                interval.tick().await;

                let iface = match nm::get_primary_connected_device().await {
                    Ok(Some(dev)) => dev,
                    _ => {
                        last_iface = None;
                        last_rx = None;
                        last_tx = None;
                        if let Ok(mut state) = speed_state_task.lock() {
                            *state = (0, 0);
                        }
                        continue;
                    }
                };

                if last_iface.as_deref() != Some(&iface) {
                    last_iface = Some(iface.clone());
                    last_rx = None;
                    last_tx = None;
                }

                let Some((rx, tx)) = read_interface_bytes(&iface) else {
                    last_rx = None;
                    last_tx = None;
                    if let Ok(mut state) = speed_state_task.lock() {
                        *state = (0, 0);
                    }
                    continue;
                };

                let down = if let Some(prev_rx) = last_rx {
                    rx.saturating_sub(prev_rx)
                } else {
                    0
                };
                let up = if let Some(prev_tx) = last_tx {
                    tx.saturating_sub(prev_tx)
                } else {
                    0
                };

                last_rx = Some(rx);
                last_tx = Some(tx);
                if let Ok(mut state) = speed_state_task.lock() {
                    *state = (down, up);
                }
            }
        });

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Adwaita Network")
            .resizable(true)
            .content(&toolbar_view)
            .default_width(700)
            .default_height(520)
            .build();

        let about_action = gio::SimpleAction::new("about", None);
        let window_weak = window.downgrade();
        about_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                Self::show_about_dialog(&window);
            }
        });
        app.add_action(&about_action);

        let settings_action = gio::SimpleAction::new("settings", None);
        let window_weak = window.downgrade();
        let prefs_for_settings = prefs.clone();
        let app_state_for_settings = app_state.clone();
        let wifi_for_settings = wifi_page.clone();
        let wifi_page_for_settings = wifi_stack_page.clone();
        let ethernet_page_for_settings = ethernet_stack_page.clone();
        let hotspot_page_for_settings = hotspot_stack_page.clone();
        let devices_page_for_settings = devices_stack_page.clone();
        let profiles_page_for_settings = profiles_stack_page.clone();
        let view_switcher_for_settings = view_switcher.clone();
        settings_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                Self::show_settings_window(
                    &window,
                    prefs_for_settings.clone(),
                    app_state_for_settings.clone(),
                    wifi_for_settings.clone(),
                    wifi_page_for_settings.clone(),
                    ethernet_page_for_settings.clone(),
                    hotspot_page_for_settings.clone(),
                    devices_page_for_settings.clone(),
                    profiles_page_for_settings.clone(),
                    view_switcher_for_settings.clone(),
                    module_layout_state.clone(),
                    module_availability_state.clone(),
                    view_stack.clone(),
                    edit_modules_box.clone(),
                    add_module_btn.clone(),
                    add_module_popover.clone(),
                );
            }
        });
        app.add_action(&settings_action);

        Self { window }
    }

    fn show_about_dialog(window: &adw::ApplicationWindow) {
        let about = adw::AboutDialog::builder()
            .application_name("Adwaita Network")
            .application_icon("icon")
            .developer_name("PlayRood")
            .version(env!("CARGO_PKG_VERSION"))
            .comments("A modern network management application built with libadwaita")
            .website("https://github.com/PlayRood/adw-network")
            .license_type(gtk4::License::Gpl30)
            .build();

        about.present(Some(window));
    }

    #[allow(clippy::too_many_arguments)]
    fn show_settings_window(
        window: &adw::ApplicationWindow,
        prefs: Rc<RefCell<AppPrefs>>,
        app_state: AppState,
        wifi_page: WifiPage,
        wifi_stack_page: adw::ViewStackPage,
        ethernet_stack_page: adw::ViewStackPage,
        hotspot_stack_page: adw::ViewStackPage,
        devices_stack_page: adw::ViewStackPage,
        profiles_stack_page: adw::ViewStackPage,
        view_switcher: adw::ViewSwitcher,
        module_layout_state: Rc<RefCell<ModuleLayoutState>>,
        module_availability_state: Rc<RefCell<ModuleAvailability>>,
        view_stack: adw::ViewStack,
        edit_modules_box: gtk4::Box,
        add_module_btn: gtk4::Button,
        add_module_popover: gtk4::Popover,
    ) {
        let style_manager = adw::StyleManager::default();
        let settings_path = config::app_settings_path();
        let (loaded_settings, repaired_settings) =
            config::load_app_settings_with_status(&settings_path)
                .unwrap_or_else(|_| (config::AppSettings::default(), false));
        if repaired_settings {
            if let Err(e) = config::save_app_settings(&settings_path, &loaded_settings) {
                log::warn!("Failed to persist repaired app settings: {}", e);
            }
        }
        let settings_state = Rc::new(RefCell::new(loaded_settings));
        let show_plain_json_warning_on_load =
            config::plain_json_warning_active(&settings_state.borrow());

        let theme_model = gtk4::StringList::new(&["System", "Light", "Dark"][..]);
        let theme_combo = adw::ComboRow::builder()
            .title("Appearance")
            .subtitle("System, light, or dark")
            .model(&theme_model)
            .build();

        let selected = match style_manager.color_scheme() {
            adw::ColorScheme::ForceLight => 1,
            adw::ColorScheme::ForceDark => 2,
            _ => 0,
        };
        theme_combo.set_selected(selected);

        let style_manager_for_theme = style_manager.clone();
        let settings_state_for_theme = settings_state.clone();
        theme_combo.connect_selected_notify(move |row: &adw::ComboRow| {
            let scheme = Self::color_scheme_from_selection(row.selected());
            style_manager_for_theme.set_color_scheme(scheme);

            debug_assert!(
                settings_state_for_theme.try_borrow_mut().is_ok(),
                "Shared state borrow conflict: settings_state_for_theme"
            );
            if let Ok(mut settings) = settings_state_for_theme.try_borrow_mut() {
                settings.color_scheme = Self::setting_from_selection(row.selected()).to_string();
                if let Err(e) = config::save_app_settings(&config::app_settings_path(), &settings) {
                    log::warn!("Failed to save app settings: {}", e);
                }
            } else {
                log::error!("Borrow conflict in UI state");
            }
        });

        let group = adw::PreferencesGroup::new();
        group.set_title("Appearance");
        group.add(&theme_combo);

        let storage_group = adw::PreferencesGroup::new();
        storage_group.set_title("Hotspot Password");

        let storage_model = gtk4::StringList::new(
            &[
                "System Keyring (Recommended)",
                "NetworkManager keyring",
                "Plain JSON (Legacy)",
            ][..],
        );
        let storage_safe_subtitle = "Where hotspot passwords are stored";
        let storage_row = adw::ComboRow::builder()
            .title("Hotspot password storage")
            .subtitle(storage_safe_subtitle)
            .model(&storage_model)
            .build();
        storage_row.set_selected(Self::selection_from_password_storage(
            &settings_state.borrow().hotspot_password_storage,
        ));
        let plain_json_selected = matches!(
            settings_state.borrow().hotspot_password_storage,
            config::HotspotPasswordStorage::PlainJson
        );
        storage_row.set_subtitle(if plain_json_selected {
            "(Highly insecure! Debug only)"
        } else {
            storage_safe_subtitle
        });

        let settings_state_for_storage = settings_state.clone();
        let storage_update_guard = Rc::new(Cell::new(false));
        let storage_update_guard_for_signal = storage_update_guard.clone();
        storage_row.connect_selected_notify(move |row| {
            if storage_update_guard_for_signal.get() {
                return;
            }

            let selected = Self::password_storage_from_selection(row.selected());
            if selected == config::HotspotPasswordStorage::PlainJson {
                let dialog = adw::AlertDialog::builder()
                    .heading("Severe Warning – Insecure Storage")
                    .body("Storing the password in a plain JSON file exposes it as clear text to any user on the computer. This is highly insecure and not recommended at all. Continue only for debugging.")
                    .default_response("cancel")
                    .close_response("cancel")
                    .build();
                dialog.add_responses(&[
                    ("cancel", "Cancel"),
                    ("continue", "Continue Anyway (debug only)"),
                ][..]);
                dialog.set_response_appearance("continue", adw::ResponseAppearance::Destructive);

                let row_for_dialog = row.clone();
                let settings_state_for_dialog = settings_state_for_storage.clone();
                let storage_update_guard_for_dialog = storage_update_guard_for_signal.clone();
                glib::spawn_future_local(async move {
                    let response = if let Some(parent) =
                        row_for_dialog.root().and_downcast::<gtk4::Window>()
                    {
                        dialog.choose_future(Some(&parent)).await
                    } else {
                        dialog.choose_future(None::<&gtk4::Window>).await
                    };

                    if response.as_str() != "continue" {
                        storage_update_guard_for_dialog.set(true);
                        row_for_dialog.set_selected(Self::selection_from_password_storage(
                            &config::HotspotPasswordStorage::Keyring,
                        ));
                        row_for_dialog.set_subtitle(storage_safe_subtitle);
                        storage_update_guard_for_dialog.set(false);
                        return;
                    }

                    debug_assert!(
                        settings_state_for_dialog.try_borrow_mut().is_ok(),
                        "Shared state borrow conflict: settings_state_for_dialog_continue"
                    );
                    if let Ok(mut settings) = settings_state_for_dialog.try_borrow_mut() {
                        settings.hotspot_password_storage = config::HotspotPasswordStorage::PlainJson;
                        settings.plain_json_debug_opt_in = true;
                        if let Err(e) =
                            config::save_app_settings(&config::app_settings_path(), &settings)
                        {
                            log::warn!("Failed to save app settings: {}", e);
                        }
                    } else {
                        log::error!("Borrow conflict in UI state");
                    }

                    storage_update_guard_for_dialog.set(true);
                    row_for_dialog.set_selected(Self::selection_from_password_storage(
                        &config::HotspotPasswordStorage::PlainJson,
                    ));
                    row_for_dialog.set_subtitle("(Highly insecure! Debug only)");
                    storage_update_guard_for_dialog.set(false);

                    if let Some(parent) = row_for_dialog.root().and_downcast::<gtk4::Window>() {
                        // * Warn immediately when plain-text hotspot storage is enabled.
                        Self::show_plain_json_warning_dialog(&parent);
                    }
                });
                return;
            }

            debug_assert!(
                settings_state_for_storage.try_borrow_mut().is_ok(),
                "Shared state borrow conflict: settings_state_for_storage"
            );
            if let Ok(mut settings) = settings_state_for_storage.try_borrow_mut() {
                settings.hotspot_password_storage = selected.clone();
                settings.plain_json_debug_opt_in = false;
                if let Err(e) = config::save_app_settings(&config::app_settings_path(), &settings) {
                    log::warn!("Failed to save app settings: {}", e);
                }
            } else {
                log::error!("Borrow conflict in UI state");
                return;
            }

            row.set_subtitle(if selected == config::HotspotPasswordStorage::PlainJson {
                "(Highly insecure! Debug only)"
            } else {
                storage_safe_subtitle
            });
        });

        storage_group.add(&storage_row);

        let quota_model = gtk4::StringList::new(&["Never reset", "Reset daily at 00:00"][..]);
        let quota_reset_row = adw::ComboRow::builder()
            .title("Hotspot quota reset")
            .subtitle("Applies to time and upload/download quotas for device rules")
            .model(&quota_model)
            .build();
        quota_reset_row.set_selected(Self::selection_from_quota_reset_policy(
            &settings_state.borrow().hotspot_quota_reset_policy,
        ));

        let settings_state_for_quota_reset = settings_state.clone();
        quota_reset_row.connect_selected_notify(move |row| {
            debug_assert!(
                settings_state_for_quota_reset.try_borrow_mut().is_ok(),
                "Shared state borrow conflict: settings_state_for_quota_reset"
            );
            if let Ok(mut settings) = settings_state_for_quota_reset.try_borrow_mut() {
                settings.hotspot_quota_reset_policy =
                    Self::quota_reset_policy_from_selection(row.selected());
                if let Err(e) = config::save_app_settings(&config::app_settings_path(), &settings) {
                    log::warn!("Failed to save app settings: {}", e);
                }
            } else {
                log::error!("Borrow conflict in UI state");
            }
        });

        storage_group.add(&quota_reset_row);

        let settings_state_for_switches = settings_state.clone();
        let auto_scan_row = adw::SwitchRow::builder()
            .title("Auto refresh networks")
            .subtitle("Rescan nearby networks every 10 seconds")
            .active(settings_state_for_switches.borrow().auto_scan)
            .build();

        let settings_state_for_switches = settings_state.clone();
        let expand_details_row = adw::SwitchRow::builder()
            .title("Always show connection details")
            .subtitle("Keep the active network card expanded")
            .active(
                settings_state_for_switches
                    .borrow()
                    .expand_connected_details,
            )
            .build();

        let settings_state_for_switches = settings_state.clone();
        let nav_icons_only_row = adw::SwitchRow::builder()
            .title("Use icons only in top navigation")
            .subtitle("Keep modules visible, but hide the text labels")
            .active(settings_state_for_switches.borrow().icons_only_navigation)
            .build();

        let prefs_for_auto_scan = prefs.clone();
        let app_state_for_auto_scan = app_state.clone();
        let settings_state_for_auto_scan = settings_state.clone();
        auto_scan_row.connect_active_notify(move |row| {
            let active = row.is_active();
            debug_assert!(
                prefs_for_auto_scan.try_borrow_mut().is_ok(),
                "Shared state borrow conflict: prefs_for_auto_scan"
            );
            if let Ok(mut prefs) = prefs_for_auto_scan.try_borrow_mut() {
                prefs.auto_scan = active;
            } else {
                log::error!("Borrow conflict in UI state");
                return;
            }
            app_state_for_auto_scan.update_prefs(|prefs| {
                prefs.auto_scan = active;
            });

            debug_assert!(
                settings_state_for_auto_scan.try_borrow_mut().is_ok(),
                "Shared state borrow conflict: settings_state_for_auto_scan"
            );
            if let Ok(mut settings) = settings_state_for_auto_scan.try_borrow_mut() {
                settings.auto_scan = active;
                if let Err(e) = config::save_app_settings(&config::app_settings_path(), &settings) {
                    log::warn!("Failed to save app settings: {}", e);
                }
            } else {
                log::error!("Borrow conflict in UI state");
            }
        });

        let prefs_for_expand = prefs.clone();
        let app_state_for_expand = app_state.clone();
        let settings_state_for_expand = settings_state.clone();
        let wifi_for_expand = wifi_page.clone();
        expand_details_row.connect_active_notify(move |row| {
            let active = row.is_active();
            debug_assert!(
                prefs_for_expand.try_borrow_mut().is_ok(),
                "Shared state borrow conflict: prefs_for_expand"
            );
            if let Ok(mut prefs) = prefs_for_expand.try_borrow_mut() {
                prefs.expand_connected_details = active;
            } else {
                log::error!("Borrow conflict in UI state");
                return;
            }
            app_state_for_expand.update_prefs(|prefs| {
                prefs.expand_connected_details = active;
            });
            wifi_for_expand.apply_expand_details_setting(active);

            debug_assert!(
                settings_state_for_expand.try_borrow_mut().is_ok(),
                "Shared state borrow conflict: settings_state_for_expand"
            );
            if let Ok(mut settings) = settings_state_for_expand.try_borrow_mut() {
                settings.expand_connected_details = active;
                if let Err(e) = config::save_app_settings(&config::app_settings_path(), &settings) {
                    log::warn!("Failed to save app settings: {}", e);
                }
            } else {
                log::error!("Borrow conflict in UI state");
            }
        });

        let prefs_for_nav_mode = prefs.clone();
        let app_state_for_nav_mode = app_state.clone();
        let settings_state_for_nav_mode = settings_state.clone();
        let wifi_stack_page_for_nav = wifi_stack_page.clone();
        let ethernet_stack_page_for_nav = ethernet_stack_page.clone();
        let hotspot_stack_page_for_nav = hotspot_stack_page.clone();
        let devices_stack_page_for_nav = devices_stack_page.clone();
        let profiles_stack_page_for_nav = profiles_stack_page.clone();
        let view_switcher_for_nav = view_switcher.clone();
        nav_icons_only_row.connect_active_notify(move |row| {
            let active = row.is_active();
            debug_assert!(
                prefs_for_nav_mode.try_borrow_mut().is_ok(),
                "Shared state borrow conflict: prefs_for_nav_mode"
            );
            if let Ok(mut prefs) = prefs_for_nav_mode.try_borrow_mut() {
                prefs.icons_only_navigation = active;
            } else {
                log::error!("Borrow conflict in UI state");
                return;
            }
            app_state_for_nav_mode.update_prefs(|prefs| {
                prefs.icons_only_navigation = active;
            });
            Self::apply_navigation_mode(
                &wifi_stack_page_for_nav,
                &ethernet_stack_page_for_nav,
                &hotspot_stack_page_for_nav,
                &devices_stack_page_for_nav,
                &profiles_stack_page_for_nav,
                &view_switcher_for_nav,
                active,
            );

            debug_assert!(
                settings_state_for_nav_mode.try_borrow_mut().is_ok(),
                "Shared state borrow conflict: settings_state_for_nav_mode"
            );
            if let Ok(mut settings) = settings_state_for_nav_mode.try_borrow_mut() {
                settings.icons_only_navigation = active;
                if let Err(e) = config::save_app_settings(&config::app_settings_path(), &settings) {
                    log::warn!("Failed to save app settings: {}", e);
                }
            } else {
                log::error!("Borrow conflict in UI state");
            }
        });

        let personalization_group = adw::PreferencesGroup::new();
        personalization_group.set_title("Behavior");
        personalization_group.add(&auto_scan_row);
        personalization_group.add(&expand_details_row);
        personalization_group.add(&nav_icons_only_row);

        let modules_group = adw::PreferencesGroup::new();
        modules_group.set_title("Modules");
        modules_group.set_description(Some(
            "Choose which top navigation modules are shown. This is separate from icons-only mode.",
        ));

        let module_preset_model = gtk4::StringList::new(
            &[
                "Automatic (smart default)",
                "All modules",
                "Wi-Fi + Profiles",
                "Ethernet + Profiles",
                "Custom",
            ][..],
        );
        let module_preset_row = adw::ComboRow::builder()
            .title("Default layout")
            .subtitle("Choose which modules are shown in the top navigation")
            .model(&module_preset_model)
            .build();

        let module_order_model =
            gtk4::StringList::new(&["Fixed: Wi-Fi, Ethernet, Hotspot, Devices, Profiles"][..]);
        let module_order_row = adw::ComboRow::builder()
            .title("Module order")
            .subtitle("Module order is fixed across the app")
            .model(&module_order_model)
            .build();
        module_order_row.set_sensitive(false);

        let module_reset_factory_btn = gtk4::Button::builder()
            .label("Restore")
            .css_classes(vec!["flat".to_string()])
            .build();
        let module_reset_factory_row = adw::ActionRow::builder()
            .title("Restore top navigation")
            .subtitle("Bring back the safe default modules and order")
            .build();
        module_reset_factory_row.add_suffix(&module_reset_factory_btn);
        module_reset_factory_row.set_activatable_widget(Some(&module_reset_factory_btn));

        let initial_layout = module_layout_state.borrow().clone();
        module_preset_row.set_selected(Self::module_preset_selection(&initial_layout));
        module_order_row.set_selected(0);

        let module_rows_guard = Rc::new(Cell::new(false));
        let module_rows_guard_for_preset = module_rows_guard.clone();
        let module_layout_state_for_preset = module_layout_state.clone();
        let module_availability_state_for_preset = module_availability_state.clone();
        let settings_state_for_module_preset = settings_state.clone();
        let view_stack_for_module_preset = view_stack.clone();
        let wifi_page_for_module_preset = wifi_stack_page.clone();
        let ethernet_page_for_module_preset = ethernet_stack_page.clone();
        let hotspot_page_for_module_preset = hotspot_stack_page.clone();
        let devices_page_for_module_preset = devices_stack_page.clone();
        let profiles_page_for_module_preset = profiles_stack_page.clone();
        let edit_modules_box_for_module_preset = edit_modules_box.clone();
        let add_module_btn_for_module_preset = add_module_btn.clone();
        let add_module_popover_for_module_preset = add_module_popover.clone();
        let module_order_row_for_preset = module_order_row.clone();
        module_preset_row.connect_selected_notify(move |row| {
            if module_rows_guard_for_preset.get() {
                return;
            }

            let selected = row.selected();
            let availability = module_availability_state_for_preset.borrow().to_owned();
            let current_layout = module_layout_state_for_preset.borrow().clone();
            let fallback_visible = current_layout.resolve_visible(availability);
            let Some((customized, visible)) =
                Self::module_flags_for_preset(selected, availability, fallback_visible)
            else {
                return;
            };

            let mut next_layout = ModuleLayoutState {
                customized,
                visible,
                order: current_layout.order,
            };
            if module_order_row_for_preset.selected() != 3 {
                next_layout.order =
                    Self::module_order_from_selection(module_order_row_for_preset.selected());
            }

            if let Ok(mut layout_state) = module_layout_state_for_preset.try_borrow_mut() {
                *layout_state = next_layout.clone();
            } else {
                log::error!("Borrow conflict in UI state");
                return;
            }
            if let Ok(mut settings) = settings_state_for_module_preset.try_borrow_mut() {
                next_layout.apply_to_settings(&mut settings);
                if let Err(e) = config::save_app_settings(&config::app_settings_path(), &settings) {
                    log::warn!("Failed to save app settings: {}", e);
                }
            } else {
                log::error!("Borrow conflict in UI state");
                return;
            }

            let resolved = next_layout.resolve_visible(availability);
            Self::apply_module_order(
                &view_stack_for_module_preset,
                &wifi_page_for_module_preset,
                &ethernet_page_for_module_preset,
                &hotspot_page_for_module_preset,
                &devices_page_for_module_preset,
                &profiles_page_for_module_preset,
                &next_layout.order,
            );
            Self::apply_module_visibility(
                &wifi_page_for_module_preset,
                &ethernet_page_for_module_preset,
                &hotspot_page_for_module_preset,
                &devices_page_for_module_preset,
                &profiles_page_for_module_preset,
                &view_stack_for_module_preset,
                resolved,
            );
            Self::render_inline_module_editor(
                &edit_modules_box_for_module_preset,
                &add_module_btn_for_module_preset,
                &add_module_popover_for_module_preset,
                module_layout_state_for_preset.clone(),
                availability,
                &view_stack_for_module_preset,
                &wifi_page_for_module_preset,
                &ethernet_page_for_module_preset,
                &hotspot_page_for_module_preset,
                &devices_page_for_module_preset,
                &profiles_page_for_module_preset,
            );
        });

        let module_rows_guard_for_order = module_rows_guard.clone();
        let module_layout_state_for_order = module_layout_state.clone();
        let module_availability_state_for_order = module_availability_state.clone();
        let settings_state_for_module_order = settings_state.clone();
        let view_stack_for_module_order = view_stack.clone();
        let wifi_page_for_module_order = wifi_stack_page.clone();
        let ethernet_page_for_module_order = ethernet_stack_page.clone();
        let hotspot_page_for_module_order = hotspot_stack_page.clone();
        let devices_page_for_module_order = devices_stack_page.clone();
        let profiles_page_for_module_order = profiles_stack_page.clone();
        let edit_modules_box_for_module_order = edit_modules_box.clone();
        let add_module_btn_for_module_order = add_module_btn.clone();
        let add_module_popover_for_module_order = add_module_popover.clone();
        module_order_row.connect_selected_notify(move |row| {
            if module_rows_guard_for_order.get() {
                return;
            }

            let availability = module_availability_state_for_order.borrow().to_owned();
            let mut next_layout = module_layout_state_for_order.borrow().clone();
            next_layout.customized = true;
            if row.selected() != 3 {
                next_layout.order = Self::module_order_from_selection(row.selected());
            }

            if let Ok(mut layout_state) = module_layout_state_for_order.try_borrow_mut() {
                *layout_state = next_layout.clone();
            } else {
                log::error!("Borrow conflict in UI state");
                return;
            }
            if let Ok(mut settings) = settings_state_for_module_order.try_borrow_mut() {
                next_layout.apply_to_settings(&mut settings);
                if let Err(e) = config::save_app_settings(&config::app_settings_path(), &settings) {
                    log::warn!("Failed to save app settings: {}", e);
                }
            } else {
                log::error!("Borrow conflict in UI state");
                return;
            }

            let resolved = next_layout.resolve_visible(availability);
            Self::apply_module_order(
                &view_stack_for_module_order,
                &wifi_page_for_module_order,
                &ethernet_page_for_module_order,
                &hotspot_page_for_module_order,
                &devices_page_for_module_order,
                &profiles_page_for_module_order,
                &next_layout.order,
            );
            Self::apply_module_visibility(
                &wifi_page_for_module_order,
                &ethernet_page_for_module_order,
                &hotspot_page_for_module_order,
                &devices_page_for_module_order,
                &profiles_page_for_module_order,
                &view_stack_for_module_order,
                resolved,
            );
            Self::render_inline_module_editor(
                &edit_modules_box_for_module_order,
                &add_module_btn_for_module_order,
                &add_module_popover_for_module_order,
                module_layout_state_for_order.clone(),
                availability,
                &view_stack_for_module_order,
                &wifi_page_for_module_order,
                &ethernet_page_for_module_order,
                &hotspot_page_for_module_order,
                &devices_page_for_module_order,
                &profiles_page_for_module_order,
            );
        });

        let module_rows_guard_for_reset = module_rows_guard.clone();
        let module_layout_state_for_reset_defaults = module_layout_state.clone();
        let module_availability_state_for_reset_defaults = module_availability_state.clone();
        let settings_state_for_module_reset = settings_state.clone();
        let module_preset_row_for_reset = module_preset_row.clone();
        let module_order_row_for_reset = module_order_row.clone();
        let view_stack_for_module_reset = view_stack.clone();
        let wifi_page_for_module_reset = wifi_stack_page.clone();
        let ethernet_page_for_module_reset = ethernet_stack_page.clone();
        let hotspot_page_for_module_reset = hotspot_stack_page.clone();
        let devices_page_for_module_reset = devices_stack_page.clone();
        let profiles_page_for_module_reset = profiles_stack_page.clone();
        let edit_modules_box_for_module_reset = edit_modules_box.clone();
        let add_module_btn_for_module_reset = add_module_btn.clone();
        let add_module_popover_for_module_reset = add_module_popover.clone();
        module_reset_factory_btn.connect_clicked(move |_| {
            let defaults = config::AppSettings::default();
            let availability = module_availability_state_for_reset_defaults
                .borrow()
                .to_owned();
            let next_layout = ModuleLayoutState::from_settings(&defaults);

            if let Ok(mut layout_state) = module_layout_state_for_reset_defaults.try_borrow_mut() {
                *layout_state = next_layout.clone();
            } else {
                log::error!("Borrow conflict in UI state");
                return;
            }
            if let Ok(mut settings) = settings_state_for_module_reset.try_borrow_mut() {
                next_layout.apply_to_settings(&mut settings);
                if let Err(e) = config::save_app_settings(&config::app_settings_path(), &settings) {
                    log::warn!("Failed to save app settings: {}", e);
                }
            } else {
                log::error!("Borrow conflict in UI state");
                return;
            }

            module_rows_guard_for_reset.set(true);
            module_preset_row_for_reset.set_selected(Self::module_preset_selection(&next_layout));
            module_order_row_for_reset
                .set_selected(Self::module_order_selection(&next_layout.order));
            module_rows_guard_for_reset.set(false);

            let resolved = next_layout.resolve_visible(availability);
            Self::apply_module_order(
                &view_stack_for_module_reset,
                &wifi_page_for_module_reset,
                &ethernet_page_for_module_reset,
                &hotspot_page_for_module_reset,
                &devices_page_for_module_reset,
                &profiles_page_for_module_reset,
                &next_layout.order,
            );
            Self::apply_module_visibility(
                &wifi_page_for_module_reset,
                &ethernet_page_for_module_reset,
                &hotspot_page_for_module_reset,
                &devices_page_for_module_reset,
                &profiles_page_for_module_reset,
                &view_stack_for_module_reset,
                resolved,
            );
            Self::render_inline_module_editor(
                &edit_modules_box_for_module_reset,
                &add_module_btn_for_module_reset,
                &add_module_popover_for_module_reset,
                module_layout_state_for_reset_defaults.clone(),
                availability,
                &view_stack_for_module_reset,
                &wifi_page_for_module_reset,
                &ethernet_page_for_module_reset,
                &hotspot_page_for_module_reset,
                &devices_page_for_module_reset,
                &profiles_page_for_module_reset,
            );
        });

        modules_group.add(&module_preset_row);
        modules_group.add(&module_order_row);
        modules_group.add(&module_reset_factory_row);

        let reset_button = gtk4::Button::builder()
            .label("Reset to defaults")
            .css_classes(vec!["destructive-action".to_string()])
            .build();

        let reset_row = adw::ActionRow::builder()
            .title("Reset settings")
            .subtitle("Clear all changes and restore defaults")
            .build();
        reset_row.add_suffix(&reset_button);
        reset_row.set_activatable_widget(Some(&reset_button));

        let reset_group = adw::PreferencesGroup::new();
        reset_group.set_title("Reset");
        reset_group.add(&reset_row);

        let settings_state_for_reset = settings_state.clone();
        let prefs_for_reset = prefs.clone();
        let app_state_for_reset = app_state.clone();
        let wifi_for_reset = wifi_page.clone();
        let theme_combo_for_reset = theme_combo.clone();
        let storage_row_for_reset = storage_row.clone();
        let quota_reset_row_for_reset = quota_reset_row.clone();
        let auto_scan_for_reset = auto_scan_row.clone();
        let expand_details_for_reset = expand_details_row.clone();
        let nav_icons_only_for_reset = nav_icons_only_row.clone();
        let style_manager_for_reset = style_manager.clone();
        let wifi_stack_page_for_reset = wifi_stack_page.clone();
        let ethernet_stack_page_for_reset = ethernet_stack_page.clone();
        let hotspot_stack_page_for_reset = hotspot_stack_page.clone();
        let devices_stack_page_for_reset = devices_stack_page.clone();
        let profiles_stack_page_for_reset = profiles_stack_page.clone();
        let view_switcher_for_reset = view_switcher.clone();
        reset_button.connect_clicked(move |_| {
            let defaults = config::AppSettings::default();
            if let Err(e) = config::save_app_settings(&config::app_settings_path(), &defaults) {
                log::warn!("Failed to save app settings: {}", e);
            }

            debug_assert!(
                settings_state_for_reset.try_borrow_mut().is_ok(),
                "Shared state borrow conflict: settings_state_for_reset"
            );
            if let Ok(mut settings) = settings_state_for_reset.try_borrow_mut() {
                *settings = defaults.clone();
            } else {
                log::error!("Borrow conflict in UI state");
                return;
            }

            debug_assert!(
                prefs_for_reset.try_borrow_mut().is_ok(),
                "Shared state borrow conflict: prefs_for_reset"
            );
            if let Ok(mut prefs) = prefs_for_reset.try_borrow_mut() {
                prefs.auto_scan = defaults.auto_scan;
                prefs.expand_connected_details = defaults.expand_connected_details;
                prefs.icons_only_navigation = defaults.icons_only_navigation;
            } else {
                log::error!("Borrow conflict in UI state");
                return;
            }
            app_state_for_reset.update_prefs(|prefs| {
                prefs.auto_scan = defaults.auto_scan;
                prefs.expand_connected_details = defaults.expand_connected_details;
                prefs.icons_only_navigation = defaults.icons_only_navigation;
            });

            theme_combo_for_reset.set_selected(0);
            style_manager_for_reset.set_color_scheme(adw::ColorScheme::Default);
            storage_row_for_reset.set_selected(Self::selection_from_password_storage(
                &defaults.hotspot_password_storage,
            ));
            quota_reset_row_for_reset.set_selected(Self::selection_from_quota_reset_policy(
                &defaults.hotspot_quota_reset_policy,
            ));

            auto_scan_for_reset.set_active(defaults.auto_scan);
            expand_details_for_reset.set_active(defaults.expand_connected_details);
            nav_icons_only_for_reset.set_active(defaults.icons_only_navigation);
            Self::apply_navigation_mode(
                &wifi_stack_page_for_reset,
                &ethernet_stack_page_for_reset,
                &hotspot_stack_page_for_reset,
                &devices_stack_page_for_reset,
                &profiles_stack_page_for_reset,
                &view_switcher_for_reset,
                defaults.icons_only_navigation,
            );
            wifi_for_reset.apply_expand_details_setting(defaults.expand_connected_details);
        });

        let page = adw::PreferencesPage::new();
        page.set_title("Settings");
        page.add(&group);
        page.add(&storage_group);
        page.add(&personalization_group);
        page.add(&modules_group);
        page.add(&reset_group);

        let settings = adw::PreferencesDialog::builder().title("Settings").build();
        settings.add(&page);
        // * Keep settings dialog responsive to main window resize.
        common::make_dialog_responsive(
            settings.upcast_ref::<adw::Dialog>(),
            Some(window.upcast_ref::<gtk4::Window>()),
            720,
            560,
        );
        settings.present(Some(window));
        if show_plain_json_warning_on_load {
            // * Warn when insecure plain-JSON storage is already active on settings load.
            Self::show_plain_json_warning_dialog(window.upcast_ref::<gtk4::Window>());
        }
    }

    fn show_plain_json_warning_dialog(parent: &gtk4::Window) {
        // * Reuse the required plain-text-storage warning message across load/change flows.
        let warning = adw::AlertDialog::builder()
            .heading("Warning")
            .body("Warning: Hotspot password will be stored in plain text (debug mode)")
            .default_response("ok")
            .close_response("ok")
            .build();
        warning.add_response("ok", "OK");

        let parent = parent.clone();
        glib::spawn_future_local(async move {
            let _ = warning.choose_future(Some(&parent)).await;
        });
    }

    fn persist_module_layout(layout: ModuleLayoutState) {
        let path = config::app_settings_path();
        let mut settings = config::load_app_settings(&path).unwrap_or_default();
        layout.apply_to_settings(&mut settings);
        if let Err(e) = config::save_app_settings(&path, &settings) {
            log::warn!("Failed to save module layout settings: {}", e);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_inline_module_editor(
        edit_modules_box: &gtk4::Box,
        add_module_btn: &gtk4::Button,
        add_module_popover: &gtk4::Popover,
        module_layout_state: Rc<RefCell<ModuleLayoutState>>,
        availability: ModuleAvailability,
        view_stack: &adw::ViewStack,
        wifi_page: &adw::ViewStackPage,
        ethernet_page: &adw::ViewStackPage,
        hotspot_page: &adw::ViewStackPage,
        devices_page: &adw::ViewStackPage,
        profiles_page: &adw::ViewStackPage,
    ) {
        while let Some(child) = edit_modules_box.first_child() {
            edit_modules_box.remove(&child);
        }

        let layout = module_layout_state.borrow().clone();
        let visible = layout.resolve_visible(availability);
        let visible_modules = layout.ordered_visible_modules(visible);
        let visible_count = visible_modules.len();
        let addable_modules: Vec<ModuleKind> = layout
            .order
            .iter()
            .copied()
            .filter(|kind| kind.is_available(availability) && !kind.is_visible(visible))
            .collect();

        for kind in visible_modules {
            let can_remove = visible_count > 1;
            let tile_button = gtk4::Button::new();
            tile_button.add_css_class("flat");
            tile_button.add_css_class("module-chip");
            tile_button.set_focus_on_click(false);
            tile_button.set_tooltip_text(Some("Drag to reorder"));

            let tile_content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            let drag_handle = gtk4::Image::from_icon_name(icon_name(
                "list-drag-handle-symbolic",
                &[
                    "view-list-symbolic",
                    "open-menu-symbolic",
                    "applications-system-symbolic",
                ][..],
            ));
            drag_handle.set_pixel_size(14);
            drag_handle.add_css_class("module-drag-handle");

            let title_label = gtk4::Label::new(Some(kind.label()));
            title_label.add_css_class("module-chip-label");

            tile_content.append(&drag_handle);
            tile_content.append(&title_label);
            tile_button.set_child(Some(&tile_content));

            let remove_button = gtk4::Button::builder()
                .icon_name(icon_name(
                    "list-remove-symbolic",
                    &["window-close-symbolic", "edit-delete-symbolic"][..],
                ))
                .build();
            remove_button.add_css_class("flat");
            remove_button.add_css_class("module-remove");
            remove_button.set_sensitive(can_remove);
            let remove_tooltip = if can_remove {
                format!("Hide {}", kind.label())
            } else {
                "At least one module must stay visible".to_string()
            };
            remove_button.set_tooltip_text(Some(&remove_tooltip));
            remove_button.set_focus_on_click(false);
            remove_button.set_valign(gtk4::Align::Center);

            let tile_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            tile_box.add_css_class("mod-edit-tile");
            tile_box.append(&tile_button);
            tile_box.append(&remove_button);
            edit_modules_box.append(&tile_box);

            let layout_state_for_remove = module_layout_state.clone();
            let availability_for_remove = availability;
            let view_stack_for_remove = view_stack.clone();
            let wifi_page_for_remove = wifi_page.clone();
            let ethernet_page_for_remove = ethernet_page.clone();
            let hotspot_page_for_remove = hotspot_page.clone();
            let devices_page_for_remove = devices_page.clone();
            let profiles_page_for_remove = profiles_page.clone();
            let edit_modules_box_for_remove = edit_modules_box.clone();
            let add_module_btn_for_remove = add_module_btn.clone();
            let add_module_popover_for_remove = add_module_popover.clone();
            remove_button.connect_clicked(move |_| {
                if !can_remove {
                    return;
                }
                let mut changed = false;
                if let Ok(mut layout) = layout_state_for_remove.try_borrow_mut() {
                    layout.customized = true;
                    kind.set_visible(&mut layout.visible, false);
                    Self::persist_module_layout(layout.clone());
                    let resolved = layout.resolve_visible(availability_for_remove);
                    Self::apply_module_order(
                        &view_stack_for_remove,
                        &wifi_page_for_remove,
                        &ethernet_page_for_remove,
                        &hotspot_page_for_remove,
                        &devices_page_for_remove,
                        &profiles_page_for_remove,
                        &layout.order,
                    );
                    Self::apply_module_visibility(
                        &wifi_page_for_remove,
                        &ethernet_page_for_remove,
                        &hotspot_page_for_remove,
                        &devices_page_for_remove,
                        &profiles_page_for_remove,
                        &view_stack_for_remove,
                        resolved,
                    );
                    changed = true;
                }
                if changed {
                    Self::render_inline_module_editor(
                        &edit_modules_box_for_remove,
                        &add_module_btn_for_remove,
                        &add_module_popover_for_remove,
                        layout_state_for_remove.clone(),
                        availability_for_remove,
                        &view_stack_for_remove,
                        &wifi_page_for_remove,
                        &ethernet_page_for_remove,
                        &hotspot_page_for_remove,
                        &devices_page_for_remove,
                        &profiles_page_for_remove,
                    );
                }
            });

            let drag_source = gtk4::DragSource::builder()
                .actions(gtk4::gdk::DragAction::MOVE)
                .build();
            drag_source.connect_prepare(move |_, _, _| {
                Some(gtk4::gdk::ContentProvider::for_value(
                    &kind.dnd_id().to_value(),
                ))
            });
            tile_button.add_controller(drag_source);

            let layout_state_for_drop = module_layout_state.clone();
            let edit_modules_box_for_drop = edit_modules_box.clone();
            let add_module_btn_for_drop = add_module_btn.clone();
            let add_module_popover_for_drop = add_module_popover.clone();
            let view_stack_for_drop = view_stack.clone();
            let wifi_page_for_drop = wifi_page.clone();
            let ethernet_page_for_drop = ethernet_page.clone();
            let hotspot_page_for_drop = hotspot_page.clone();
            let devices_page_for_drop = devices_page.clone();
            let profiles_page_for_drop = profiles_page.clone();
            let drop_target =
                gtk4::DropTarget::new(String::static_type(), gtk4::gdk::DragAction::MOVE);
            drop_target.connect_drop(move |_, value, _, _| {
                let Ok(source_id) = value.get::<String>() else {
                    return false;
                };
                let Some(source_kind) = ModuleKind::from_dnd_id(&source_id)
                    .or_else(|| ModuleKind::from_label(&source_id))
                else {
                    return false;
                };
                if source_kind == kind {
                    return false;
                }

                let mut changed = false;
                if let Ok(mut layout) = layout_state_for_drop.try_borrow_mut() {
                    if let Some(src_idx) = layout.order.iter().position(|item| *item == source_kind)
                    {
                        layout.order.remove(src_idx);
                    }
                    if let Some(dst_idx) = layout.order.iter().position(|item| *item == kind) {
                        layout.order.insert(dst_idx, source_kind);
                        layout.customized = true;
                        Self::persist_module_layout(layout.clone());
                        let resolved = layout.resolve_visible(availability);
                        Self::apply_module_order(
                            &view_stack_for_drop,
                            &wifi_page_for_drop,
                            &ethernet_page_for_drop,
                            &hotspot_page_for_drop,
                            &devices_page_for_drop,
                            &profiles_page_for_drop,
                            &layout.order,
                        );
                        Self::apply_module_visibility(
                            &wifi_page_for_drop,
                            &ethernet_page_for_drop,
                            &hotspot_page_for_drop,
                            &devices_page_for_drop,
                            &profiles_page_for_drop,
                            &view_stack_for_drop,
                            resolved,
                        );
                        changed = true;
                    }
                }

                if changed {
                    Self::render_inline_module_editor(
                        &edit_modules_box_for_drop,
                        &add_module_btn_for_drop,
                        &add_module_popover_for_drop,
                        layout_state_for_drop.clone(),
                        availability,
                        &view_stack_for_drop,
                        &wifi_page_for_drop,
                        &ethernet_page_for_drop,
                        &hotspot_page_for_drop,
                        &devices_page_for_drop,
                        &profiles_page_for_drop,
                    );
                }

                changed
            });
            tile_box.add_controller(drop_target);
        }

        add_module_btn.set_sensitive(!addable_modules.is_empty());
        add_module_btn.set_tooltip_text(Some("Add module"));

        let add_list = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        for kind in addable_modules {
            let row_button = gtk4::Button::with_label(kind.label());
            row_button.add_css_class("flat");
            let layout_state_for_add = module_layout_state.clone();
            let popover_for_add = add_module_popover.clone();
            let edit_modules_box_for_add = edit_modules_box.clone();
            let add_module_btn_for_add = add_module_btn.clone();
            let add_module_popover_for_add = add_module_popover.clone();
            let view_stack_for_add = view_stack.clone();
            let wifi_page_for_add = wifi_page.clone();
            let ethernet_page_for_add = ethernet_page.clone();
            let hotspot_page_for_add = hotspot_page.clone();
            let devices_page_for_add = devices_page.clone();
            let profiles_page_for_add = profiles_page.clone();
            row_button.connect_clicked(move |_| {
                let mut changed = false;
                if let Ok(mut layout) = layout_state_for_add.try_borrow_mut() {
                    layout.customized = true;
                    kind.set_visible(&mut layout.visible, true);
                    Self::persist_module_layout(layout.clone());
                    let resolved = layout.resolve_visible(availability);
                    Self::apply_module_order(
                        &view_stack_for_add,
                        &wifi_page_for_add,
                        &ethernet_page_for_add,
                        &hotspot_page_for_add,
                        &devices_page_for_add,
                        &profiles_page_for_add,
                        &layout.order,
                    );
                    Self::apply_module_visibility(
                        &wifi_page_for_add,
                        &ethernet_page_for_add,
                        &hotspot_page_for_add,
                        &devices_page_for_add,
                        &profiles_page_for_add,
                        &view_stack_for_add,
                        resolved,
                    );
                    changed = true;
                }
                popover_for_add.popdown();
                if changed {
                    Self::render_inline_module_editor(
                        &edit_modules_box_for_add,
                        &add_module_btn_for_add,
                        &add_module_popover_for_add,
                        layout_state_for_add.clone(),
                        availability,
                        &view_stack_for_add,
                        &wifi_page_for_add,
                        &ethernet_page_for_add,
                        &hotspot_page_for_add,
                        &devices_page_for_add,
                        &profiles_page_for_add,
                    );
                }
            });
            add_list.append(&row_button);
        }
        add_module_popover.set_child(Some(&add_list));
    }

    fn apply_module_visibility(
        wifi_page: &adw::ViewStackPage,
        ethernet_page: &adw::ViewStackPage,
        hotspot_page: &adw::ViewStackPage,
        devices_page: &adw::ViewStackPage,
        profiles_page: &adw::ViewStackPage,
        view_stack: &adw::ViewStack,
        visible: ModuleFlags,
    ) {
        wifi_page.set_visible(visible.wifi);
        ethernet_page.set_visible(visible.ethernet);
        hotspot_page.set_visible(visible.hotspot);
        devices_page.set_visible(visible.devices);
        profiles_page.set_visible(visible.profiles);

        let current_visible = view_stack
            .visible_child()
            .map(|w| view_stack.page(&w).is_visible())
            .unwrap_or(false);

        if !current_visible {
            if wifi_page.is_visible() {
                let child = wifi_page.child();
                view_stack.set_visible_child(&child);
            } else if ethernet_page.is_visible() {
                let child = ethernet_page.child();
                view_stack.set_visible_child(&child);
            } else if hotspot_page.is_visible() {
                let child = hotspot_page.child();
                view_stack.set_visible_child(&child);
            } else if devices_page.is_visible() {
                let child = devices_page.child();
                view_stack.set_visible_child(&child);
            } else if profiles_page.is_visible() {
                let child = profiles_page.child();
                view_stack.set_visible_child(&child);
            }
        }
    }

    fn apply_module_order(
        view_stack: &adw::ViewStack,
        wifi_page: &adw::ViewStackPage,
        ethernet_page: &adw::ViewStackPage,
        hotspot_page: &adw::ViewStackPage,
        devices_page: &adw::ViewStackPage,
        profiles_page: &adw::ViewStackPage,
        order: &[ModuleKind],
    ) {
        let mut previous: Option<gtk4::Widget> = None;
        for kind in order {
            let child = match kind {
                ModuleKind::Ethernet => ethernet_page.child(),
                ModuleKind::Wifi => wifi_page.child(),
                ModuleKind::Hotspot => hotspot_page.child(),
                ModuleKind::Device => devices_page.child(),
                ModuleKind::Profiles => profiles_page.child(),
            };
            child.insert_after(view_stack, previous.as_ref());
            previous = Some(child);
        }
    }

    async fn detect_module_availability() -> ModuleAvailability {
        let wifi = match nm::has_wifi_device().await {
            Ok(v) => v,
            Err(e) => {
                log::warn!("Failed to detect Wi-Fi device: {}", e);
                false
            }
        };
        let ethernet = match nm::has_ethernet_device().await {
            Ok(v) => v,
            Err(e) => {
                log::warn!("Failed to detect ethernet device: {}", e);
                false
            }
        };
        let devices = match nm::NetworkManager::get_devices().await {
            Ok(list) => list
                .into_iter()
                .any(|device| !matches!(device.device_type, nm::DeviceType::Loopback)),
            Err(e) => {
                log::warn!("Failed to detect connected devices: {}", e);
                false
            }
        };

        ModuleAvailability {
            wifi,
            ethernet,
            hotspot: wifi,
            devices,
            profiles: true,
        }
    }

    fn load_saved_theme() {
        let style_manager = adw::StyleManager::default();
        if let Ok(settings) = config::load_app_settings(&config::app_settings_path()) {
            let scheme = match settings.color_scheme.as_str() {
                "light" => adw::ColorScheme::ForceLight,
                "dark" => adw::ColorScheme::ForceDark,
                _ => adw::ColorScheme::Default,
            };
            style_manager.set_color_scheme(scheme);
        }
    }

    fn color_scheme_from_selection(selected: u32) -> adw::ColorScheme {
        match selected {
            1 => adw::ColorScheme::ForceLight,
            2 => adw::ColorScheme::ForceDark,
            _ => adw::ColorScheme::Default,
        }
    }

    fn setting_from_selection(selected: u32) -> &'static str {
        match selected {
            1 => "light",
            2 => "dark",
            _ => "system",
        }
    }

    fn password_storage_from_selection(selected: u32) -> config::HotspotPasswordStorage {
        match selected {
            1 => config::HotspotPasswordStorage::NetworkManager,
            2 => config::HotspotPasswordStorage::PlainJson,
            _ => config::HotspotPasswordStorage::Keyring,
        }
    }

    fn selection_from_password_storage(storage: &config::HotspotPasswordStorage) -> u32 {
        match storage {
            config::HotspotPasswordStorage::Keyring => 0,
            config::HotspotPasswordStorage::NetworkManager => 1,
            config::HotspotPasswordStorage::PlainJson => 2,
        }
    }

    fn quota_reset_policy_from_selection(selected: u32) -> config::HotspotQuotaResetPolicy {
        match selected {
            1 => config::HotspotQuotaResetPolicy::DailyMidnight,
            _ => config::HotspotQuotaResetPolicy::Never,
        }
    }

    fn selection_from_quota_reset_policy(policy: &config::HotspotQuotaResetPolicy) -> u32 {
        match policy {
            config::HotspotQuotaResetPolicy::Never => 0,
            config::HotspotQuotaResetPolicy::DailyMidnight => 1,
        }
    }

    fn module_preset_selection(layout: &ModuleLayoutState) -> u32 {
        if !layout.customized {
            return 0;
        }

        if layout.visible.wifi
            && layout.visible.ethernet
            && layout.visible.hotspot
            && layout.visible.devices
            && layout.visible.profiles
        {
            return 1;
        }

        if layout.visible.wifi
            && !layout.visible.ethernet
            && !layout.visible.hotspot
            && !layout.visible.devices
            && layout.visible.profiles
        {
            return 2;
        }

        if !layout.visible.wifi
            && layout.visible.ethernet
            && !layout.visible.hotspot
            && !layout.visible.devices
            && layout.visible.profiles
        {
            return 3;
        }

        4
    }

    fn module_order_selection(order: &[ModuleKind]) -> u32 {
        let _ = order;
        0
    }

    fn module_order_from_selection(selected: u32) -> Vec<ModuleKind> {
        let _ = selected;
        ModuleKind::ORDER.to_vec()
    }

    fn module_flags_for_preset(
        selected: u32,
        availability: ModuleAvailability,
        fallback_visible: ModuleFlags,
    ) -> Option<(bool, ModuleFlags)> {
        match selected {
            0 => Some((false, ModuleLayoutState::default_visible(availability))),
            1 => Some((
                true,
                ModuleFlags {
                    wifi: true,
                    ethernet: true,
                    hotspot: true,
                    devices: true,
                    profiles: true,
                },
            )),
            2 => Some((
                true,
                ModuleFlags {
                    wifi: true,
                    ethernet: false,
                    hotspot: false,
                    devices: false,
                    profiles: true,
                },
            )),
            3 => Some((
                true,
                ModuleFlags {
                    wifi: false,
                    ethernet: true,
                    hotspot: false,
                    devices: false,
                    profiles: true,
                },
            )),
            4 => Some((true, fallback_visible)),
            _ => None,
        }
    }

    fn apply_navigation_mode(
        wifi_stack_page: &adw::ViewStackPage,
        ethernet_stack_page: &adw::ViewStackPage,
        hotspot_stack_page: &adw::ViewStackPage,
        devices_stack_page: &adw::ViewStackPage,
        profiles_stack_page: &adw::ViewStackPage,
        view_switcher: &adw::ViewSwitcher,
        icons_only: bool,
    ) {
        let wifi_title = if icons_only { "" } else { "Wi-Fi" };
        let ethernet_title = if icons_only { "" } else { "Ethernet" };
        let hotspot_title = if icons_only { "" } else { "Hotspot" };
        let devices_title = if icons_only { "" } else { "Devices" };
        let profiles_title = if icons_only { "" } else { "Profiles" };

        wifi_stack_page.set_title(Some(wifi_title));
        ethernet_stack_page.set_title(Some(ethernet_title));
        hotspot_stack_page.set_title(Some(hotspot_title));
        devices_stack_page.set_title(Some(devices_title));
        profiles_stack_page.set_title(Some(profiles_title));

        let tooltips: Vec<&'static str> = if icons_only {
            vec![
                "Wi-Fi Networks",
                "Ethernet",
                "Hotspot",
                "Connected Devices",
                "Profiles",
            ]
        } else {
            vec!["Wi-Fi", "Ethernet", "Hotspot", "Devices", "Profiles"]
        };

        Self::apply_view_switcher_tooltips(view_switcher, &tooltips[..]);
        let view_switcher_for_idle = view_switcher.clone();
        // * Re-apply tooltips after realization so icon-only navigation buttons always get them.
        glib::idle_add_local_once(move || {
            Self::apply_view_switcher_tooltips(&view_switcher_for_idle, &tooltips[..]);
        });
    }

    fn apply_view_switcher_tooltips(view_switcher: &adw::ViewSwitcher, tooltips: &[&str]) {
        fn walk(widget: &gtk4::Widget, tooltips: &[&str], idx: &mut usize) {
            if let Ok(button) = widget.clone().downcast::<gtk4::ToggleButton>() {
                if let Some(text) = tooltips.get(*idx) {
                    button.set_tooltip_text(Some(text));
                }
                *idx += 1;
            }

            let mut child = widget.first_child();
            while let Some(current) = child {
                walk(&current, tooltips, idx);
                child = current.next_sibling();
            }
        }

        let root: &gtk4::Widget = view_switcher.upcast_ref();
        let mut idx = 0usize;
        walk(root, tooltips, &mut idx);
    }

    fn load_css() {
        let provider = gtk4::CssProvider::new();

        let css = r#"
/* Modern network management design - Inspired by airctl and GNOME */

/* Network rows */
.network-row {
    padding: 8px;
    transition: background 200ms ease;
}

.network-row:hover {
    background: alpha(currentColor, 0.05);
}

/* List styling */
list,
.boxed-list {
    border: none;
    box-shadow: none;
}

.boxed-list {
    background: transparent;
}

.boxed-list row {
    margin: 4px 0;
    border-radius: 16px;
    background: alpha(@window_fg_color, 0.035);
    border: 1px solid alpha(@window_fg_color, 0.08);
}

.boxed-list row:hover {
    background: alpha(@window_fg_color, 0.055);
}

.list-loading {
    opacity: 0.6;
}

/* Buttons */
button.flat.circular {
    min-width: 32px;
    min-height: 32px;
    padding: 0;
}

button.touch-target {
    min-height: 44px;
    padding: 8px 14px;
}

button.circular.touch-target {
    min-width: 44px;
    min-height: 44px;
    padding: 0;
}

.mod-editor-panel {
    background: alpha(@window_bg_color, 0.98);
    border: 1px solid alpha(@window_fg_color, 0.12);
    border-radius: 10px;
    padding: 14px;
}

.mod-editor-inline {
    padding: 6px 8px;
    border-radius: 10px;
    background: alpha(@window_fg_color, 0.03);
    border: 1px solid alpha(@window_fg_color, 0.1);
}

.mod-edit-tile {
    margin: 2px;
    padding: 0 2px;
    border-radius: 10px;
    background: alpha(@window_fg_color, 0.045);
    border: 1px solid alpha(@window_fg_color, 0.1);
}

button.module-chip {
    min-height: 38px;
    padding: 0 14px;
    border-radius: 9px;
    background: transparent;
}

button.module-chip:hover {
    background: alpha(@window_fg_color, 0.06);
}

.module-chip-label {
    font-weight: 600;
}

.module-drag-handle {
    color: alpha(@window_fg_color, 0.55);
}

button.module-remove {
    min-width: 26px;
    min-height: 26px;
    padding: 0;
    border-radius: 8px;
    border: 1px solid alpha(@window_fg_color, 0.12);
    background: alpha(@window_fg_color, 0.08);
    box-shadow: none;
    color: alpha(@window_fg_color, 0.92);
}

button.module-remove:hover {
    background: alpha(@window_fg_color, 0.14);
    color: @window_fg_color;
}

button.module-remove:active {
    background: alpha(@window_fg_color, 0.18);
}

button.module-remove image {
    color: inherit;
    -gtk-icon-size: 14px;
}

button.module-remove:disabled {
    background: alpha(@window_fg_color, 0.04);
    border-color: alpha(@window_fg_color, 0.06);
    color: alpha(@window_fg_color, 0.35);
}

button.nav-rect-button {
    min-height: 38px;
    min-width: 38px;
    padding: 0 14px;
    border-radius: 9px;
}

/* Main window: custom close button (gray, light hover) */
button.window-close {
    background: alpha(@window_fg_color, 0.12);
    border: 1px solid alpha(@window_fg_color, 0.18);
    color: alpha(@window_fg_color, 0.85);
    transition: background 150ms ease, border-color 150ms ease, color 150ms ease;
}

button.window-close:hover {
    background: alpha(@window_fg_color, 0.2);
    border-color: alpha(@window_fg_color, 0.25);
}

button.window-close:active {
    background: alpha(@window_fg_color, 0.26);
    border-color: alpha(@window_fg_color, 0.3);
}

/* Main headerbar close button size */
headerbar windowcontrols button.close,
headerbar button.close,
.titlebar windowcontrols button.close,
.titlebar button.close {
    min-width: 18px;
    min-height: 18px;
    padding: 0;
    margin: 0;
    border-radius: 4px;
}

/* Hide dialog close buttons (only show close on main window) */
dialog headerbar button.close,
dialog headerbar windowcontrols,
dialog .titlebar button.close,
dialog .titlebar windowcontrols {
    min-width: 0;
    min-height: 0;
    padding: 0;
    margin: 0;
    border: 0;
    background: transparent;
    box-shadow: none;
    opacity: 0;
}


button.action-pill {
    padding: 8px 16px;
    border-radius: 999px;
    min-height: 40px;
    font-weight: 600;
}

button.action-pill image {
    margin-right: 6px;
}

button.qr-pill {
    padding: 8px 14px;
    min-height: 40px;
    font-weight: 600;
    font-size: 1.0em;
}

button.qr-pill image {
    margin-right: 6px;
}

button.action-pill.disconnect {
    background: @warning_bg_color;
    color: @warning_fg_color;
}

button.action-pill.forget {
    background: @error_bg_color;
    color: @error_fg_color;
}

button.action-pill.forget:hover {
    background: @error_color;
    color: @error_fg_color;
}

button.action-pill.forget image {
    color: @error_fg_color;
}

button.destructive-action {
    color: @error_color;
    font-weight: 700;
}

button.destructive-action:hover {
    background: alpha(@error_color, 0.32);
    color: @error_color;
}

button.destructive-action image,
button.destructive-action:hover image {
    color: @error_color;
    opacity: 1;
}

.forget-icon {
    color: @error_color;
    opacity: 1;
}

popover.menu button.destructive-action:hover {
    background: alpha(@error_color, 0.3);
}

/* Connected card */
.connected-card {
    background: alpha(@accent_bg_color, 0.18);
    border: 1px solid alpha(@accent_bg_color, 0.35);
    border-radius: 18px;
    padding: 18px;
    margin-bottom: 10px;
}

.hotspot-hero {
    background: alpha(@accent_bg_color, 0.14);
    border: 1px solid alpha(@accent_bg_color, 0.28);
    border-radius: 22px;
    padding: 24px 18px;
}

.hotspot-actions {
    margin-bottom: 10px;
}

.device-policy-row {
    padding-top: 4px;
    padding-bottom: 4px;
}

.connected-ssid {
    font-weight: 600;
    font-size: 1.1em;
}

.connected-subtitle {
    opacity: 0.8;
    font-size: 0.9em;
}

.detail-label {
    opacity: 0.8;
    font-size: 0.85em;
}

.detail-ip {
    font-weight: 600;
    opacity: 0.95;
    color: @accent_color;
    font-size: 0.9em;
}

.strength-label {
    opacity: 0.7;
    font-size: 0.85em;
}

progressbar.strength-weak trough {
    background: alpha(@error_color, 0.15);
}

progressbar.strength-weak progress {
    background: @error_color;
}

progressbar.strength-medium trough {
    background: alpha(@warning_color, 0.18);
}

progressbar.strength-medium progress {
    background: @warning_color;
}

progressbar.strength-strong trough {
    background: alpha(@success_color, 0.18);
}

progressbar.strength-strong progress {
    background: @success_color;
}

progressbar.strength-very-strong trough {
    background: alpha(@accent_color, 0.2);
}

progressbar.strength-very-strong progress {
    background: @accent_color;
}

.section-header {
    font-size: 0.8em;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    opacity: 0.7;
}

/* Signal strength colors */
.signal-excellent {
    color: @success_color;
}

.signal-good {
    color: @accent_color;
}

.signal-fair {
    color: @warning_color;
}

.signal-weak {
    color: @error_color;
}

/* Hotspot status */
.hotspot-active-header {
    color: @success_color;
    font-weight: 600;
}

.hotspot-icon {
    color: grey;
}

.hotspot-pulse {
    color: @success_color;
    animation: pulse 1.6s ease-in-out infinite;
}

/* Network item */
.network-item {
    padding: 12px 16px;
    border-radius: 8px;
    transition: background 150ms ease;
}

.network-item:hover {
    background: alpha(currentColor, 0.04);
}

/* Preferences groups */
.preferences-group {
    margin-top: 12px;
}

/* Search entry */
searchentry {
    min-height: 40px;
}

.big-spinner {
    min-width: 24px;
    min-height: 24px;
}

/* Filter buttons */
button.toggle {
    padding: 6px 12px;
    min-height: 30px;
    border-radius: 999px;
    font-size: 0.85em;
}

button.toggle:checked {
    background: @accent_bg_color;
    color: @accent_fg_color;
}

.filter-row {
    margin-top: 4px;
}

/* Connected indicator */
.connected-indicator {
    color: @success_color;
    font-weight: 600;
}

/* Focus handling */
*:focus {
    outline: none;
}

/* Cards */
.card {
    background: alpha(currentColor, 0.03);
    border-radius: 12px;
    border: none;
}

/* Menu styling */
popover.menu {
    padding: 0;
}

popover.menu box {
    margin: 0;
}

popover.menu button {
    padding: 8px 12px;
    border-radius: 0;
}

popover.menu button:first-child {
    border-radius: 6px 6px 0 0;
}

popover.menu button:last-child {
    border-radius: 0 0 6px 6px;
}

/* Header menu button hitbox */
button.menu-button {
    padding: 0;
    min-width: 18px;
    min-height: 18px;
    border-radius: 4px;
}

button.menu-button:hover {
    background: transparent;
}

button.header-mini-button {
    min-width: 18px;
    min-height: 18px;
    padding: 0;
    border-radius: 4px;
}

headerbar button.header-mini-button {
    min-width: 16px;
    min-height: 16px;
    padding: 0;
    margin: 0;
    border-radius: 4px;
}

headerbar button.window-close {
    border-radius: 8px;
}

/* Network subtitle styling */
.network-subtitle {
    font-size: 0.85em;
    opacity: 0.7;
}

/* Signal indicator spacing */
.signal-indicator {
    margin-right: 8px;
}

/* Spinner animation */
@keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
}

.spinning {
    animation: spin 1s linear infinite;
}

@keyframes fadeIn {
    from { opacity: 0; transform: translateY(4px); }
    to { opacity: 1; transform: translateY(0); }
}

.fade-in {
    animation: fadeIn 220ms ease-out;
}

@keyframes pulse {
    0% { transform: scale(1); opacity: 0.8; }
    50% { transform: scale(1.06); opacity: 1; }
    100% { transform: scale(1); opacity: 0.8; }
}

/* Toast styling */
toast {
    border-radius: 8px;
}

/* Dialog spacing */
dialog box {
    padding: 0;
}

/* Action row improvements */
row {
    transition: background 200ms ease;
}

row:hover {
    background: alpha(currentColor, 0.03);
}

/* Improve password entry visibility */
entry.password {
    min-height: 40px;
}

/* Toggle switch improvements */
switch {
    min-width: 48px;
}

/* Toolbar view */
toolbarview {
    background: @window_bg_color;
}

/* View switcher */
viewswitcher button {
    padding: 6px 12px;
    border-radius: 10px;
}

viewswitcher button:checked {
    background: alpha(@accent_bg_color, 0.25);
}

viewswitcher button:checked label {
    font-weight: 600;
    font-size: 1.02em;
}

.navigation-shell {
    margin-top: 6px;
    padding: 6px 8px;
    border-radius: 10px;
    background: alpha(@window_fg_color, 0.04);
    border: 1px solid alpha(@window_fg_color, 0.1);
}

.navigation-switcher button {
    min-height: 46px;
    min-width: 68px;
    padding: 10px 14px;
    margin: 0 3px;
    border-radius: 9px;
}

.navigation-switcher button image {
    -gtk-icon-size: 20px;
}

.navigation-switcher button:checked {
    background: alpha(@accent_bg_color, 0.92);
    color: @accent_fg_color;
    box-shadow: inset 0 0 0 1px alpha(@accent_fg_color, 0.08);
}

.navigation-switcher button:hover:not(:checked) {
    background: alpha(@window_fg_color, 0.06);
}

.navigation-editor {
    padding: 6px 8px;
    border-radius: 10px;
}

.navigation-editor-list button {
    min-height: 34px;
    padding: 6px 12px;
}

/* Status page */
statuspage {
    padding: 32px;
}

statuspage.devices-empty image {
    -gtk-icon-size: 72px;
    opacity: 0.5;
    animation: pulse 1.8s ease-in-out infinite;
}

.status-pill {
    padding: 4px 10px;
    border-radius: 10px;
    background: alpha(@accent_bg_color, 0.12);
}

.header-title {
    padding-top: 2px;
    padding-bottom: 2px;
}

.status-text {
    font-size: 0.85em;
}

.status-speed-box {
    opacity: 0.8;
}

.status-speed-text {
    font-size: 0.8em;
}

.status-speed-sep {
    opacity: 0.6;
}

.status-pill.status-online {
    background: alpha(@success_color, 0.12);
    color: @success_color;
}

.status-pill.status-offline {
    background: alpha(@warning_color, 0.12);
    color: @warning_color;
}

.status-pill.status-hotspot {
    background: alpha(@success_color, 0.2);
    color: @success_color;
}

/* Light mode tuning */
@media (prefers-color-scheme: light) {
    .card {
        background: alpha(@window_fg_color, 0.025);
        border: 1px solid alpha(@window_fg_color, 0.08);
    }

    .connected-card {
        background: alpha(@accent_bg_color, 0.1);
        border-color: alpha(@accent_bg_color, 0.25);
    }

    .hotspot-hero {
        background: alpha(@accent_bg_color, 0.08);
        border-color: alpha(@accent_bg_color, 0.18);
    }

    .network-row:hover,
    .network-item:hover,
    row:hover {
        background: alpha(@window_fg_color, 0.04);
    }

    .status-pill {
        background: alpha(@accent_bg_color, 0.1);
        border: 1px solid alpha(@window_fg_color, 0.08);
    }

    .status-pill.status-online {
        background: alpha(@success_color, 0.1);
    }

    .status-pill.status-offline {
        background: alpha(@warning_color, 0.1);
    }

    .status-pill.status-hotspot {
        background: alpha(@success_color, 0.14);
    }

    button.window-close {
        background: alpha(@window_fg_color, 0.08);
        border-color: alpha(@window_fg_color, 0.16);
    }

    viewswitcher button:checked {
        background: alpha(@accent_bg_color, 0.2);
    }
}

"#;

        provider.load_from_data(css);

        gtk4::style_context_add_provider_for_display(
            &gtk4::gdk::Display::default().expect("Could not connect to display"),
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    pub fn present(&self) {
        self.window.present();
    }
}

fn read_interface_bytes(iface: &str) -> Option<(u64, u64)> {
    let rx_path = format!("/sys/class/net/{}/statistics/rx_bytes", iface);
    let tx_path = format!("/sys/class/net/{}/statistics/tx_bytes", iface);
    let rx = fs::read_to_string(rx_path)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    let tx = fs::read_to_string(tx_path)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    Some((rx, tx))
}

fn format_speed(bytes_per_sec: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

    let value = bytes_per_sec as f64;
    if value >= GIB {
        format!("{:.1} GiB/s", value / GIB)
    } else if value >= MIB {
        format!("{:.1} MiB/s", value / MIB)
    } else if value >= KIB {
        format!("{:.0} KiB/s", value / KIB)
    } else {
        format!("{} B/s", bytes_per_sec)
    }
}
