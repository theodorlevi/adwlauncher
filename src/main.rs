use freedesktop_desktop_entry::DesktopEntry;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use gtk::prelude::WidgetExt;
use gtk::prelude::*;
use gtk4_layer_shell::{Layer, LayerShell};
use niri_ipc::{Action, Request, Response};
use rayon::prelude::*;
use relm4::factory::FactoryVecDeque;
use relm4::gtk::CssProvider;
use relm4::prelude::*;
use std::cmp::PartialEq;

#[derive(Debug, PartialEq, Clone)]
enum OpenType {
    Graphical,
    Terminal,
    Window,
}

impl Default for OpenType {
    fn default() -> Self {
        Self::Graphical
    }
}

#[derive(Default, Debug, Clone)]
struct Entry {
    open_type: OpenType,
    exec: String,
    icon: String,
    name: String,
}
fn get_entries() -> Vec<Entry> {
    let mut entries = vec![];
    // Check both system and user application directories
    let home = std::env::var("HOME").unwrap_or_default();
    let app_dirs = vec![
        std::path::PathBuf::from("/usr/share/applications"),
        std::path::PathBuf::from(format!("{}/.local/share/applications", home)),
        std::path::PathBuf::from("/var/lib/flatpak/exports/share/applications/"),
        std::path::PathBuf::from(format!(
            "{}/.local/share/flatpak/exports/share/applications/",
            home
        )),
    ];
    for app_dir in app_dirs {
        let dir = match std::fs::read_dir(&app_dir) {
            Ok(dir) => dir,
            Err(_) => continue, // Skip if the directory doesn't exist
        };
        let new_entries: Vec<Entry> = dir
            .collect::<Vec<_>>()
            .into_par_iter()
            .filter_map(|file| {
                let file = file.ok()?;
                let path = file.path();
                let desktop_file = DesktopEntry::from_path(path, None::<&[&str]>).ok()?;
                let name = desktop_file.name(&[""]).unwrap_or_default().to_string();
                if name.is_empty() {
                    return None;
                }
                Some(Entry {
                    name,
                    exec: desktop_file.exec().unwrap_or_default().to_string(),
                    icon: desktop_file.icon().unwrap_or_default().to_string(),
                    open_type: if desktop_file.terminal() {
                        OpenType::Terminal
                    } else {
                        OpenType::Graphical
                    },
                })
            })
            .collect();
        entries.extend(new_entries);
    }
    let mut soc = niri_ipc::socket::Socket::connect().unwrap();
    let response = soc.send(Request::Windows).unwrap().unwrap();
    let windows = match response {
        Response::Windows(windows) => Ok::<Vec<niri_ipc::Window>, String>(windows),
        _ => Err("Unexpected response type".into()),
    }
    .unwrap_or_default();
    for window in windows {
        let mut entry = Entry::default();
        entry.name = window.title.unwrap_or_default();
        entry.exec = window.id.to_string();
        entry.icon = window.app_id.unwrap();
        entry.open_type = OpenType::Window;
        entries.push(entry);
    }
    entries
}

#[derive(Debug)]
struct EntryView {
    entry: Entry,
    selected: bool,
}

#[relm4::factory]
impl FactoryComponent for EntryView {
    type ParentWidget = gtk::Box;
    type CommandOutput = ();
    type Input = bool;
    type Output = ();
    type Init = Entry;

    view! {
        #[root]
        root_box = gtk::Box {
            set_spacing: 6,
            gtk::Image {
                set_icon_name: Some(&self.entry.icon),
                set_pixel_size: 32,
            },
            gtk::Button {
                #[watch]
                set_css_classes: if self.selected { &["flat", "rounded", "selected"] } else { &["flat", "rounded"] },
                set_can_focus: false,
                set_focusable: false,
                set_hexpand: true,
                set_halign: gtk::Align::Start,
                gtk::Label {
                    set_label: &self.entry.name,
                    set_halign: gtk::Align::Start,
                },
            },
        }
    }

    fn init_model(entry: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self {
            entry,
            selected: false,
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: FactorySender<Self>) {
        self.selected = msg;
    }
}

struct App {
    selected_name: String,
    selected_index: usize,
    entries: FactoryVecDeque<EntryView>,
    all_entries: Vec<Entry>,
    search_query: String,
    scrolled_window: gtk::ScrolledWindow,
    search_entry: gtk::SearchEntry,
    window: adw::ApplicationWindow,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("selected_name", &self.selected_name)
            .field("selected_index", &self.selected_index)
            .finish()
    }
}

#[derive(Debug)]
enum Msg {
    NavigateUp,
    NavigateDown,
    SelectEntry,
    CloseWindow,
    SearchChanged(String),
    WindowShown,
}

#[relm4::component]
impl SimpleComponent for App {
    type Input = Msg;
    type Output = ();
    type Init = ();

    view! {
        #[name = "window"]
        adw::ApplicationWindow {
            set_default_size: (800, 600),
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                #[name = "headerbar"]
                adw::HeaderBar {
                    set_css_classes: &["flat"],
                    #[wrap(Some)]
                    #[name = "search_entry"]
                    set_title_widget = &gtk::SearchEntry {
                        set_hexpand: true,
                        set_placeholder_text: Some("Search..."),
                        connect_search_changed[sender] => move |entry| {
                            sender.input(Msg::SearchChanged(entry.text().to_string()));
                        },
                        connect_activate[sender] => move |_| {
                            sender.input(Msg::SelectEntry);
                        },
                    },
                },
                #[name = "scrolled_window"]
                gtk::ScrolledWindow {
                    set_vexpand: true,
                    set_hexpand: true,
                    #[local_ref]
                    entries_box -> gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 6,
                        set_margin_all: 12,
                    }
                }
            }
        }
    }

    fn init(
        _: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let entries = FactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .detach();

        let app_entries = get_entries();
        let first_name = app_entries
            .first()
            .map(|e| e.name.clone())
            .unwrap_or_default();

        let mut model = App {
            selected_name: first_name,
            selected_index: 0,
            entries,
            all_entries: app_entries.clone(),
            search_query: String::new(),
            scrolled_window: gtk::ScrolledWindow::new(),
            search_entry: gtk::SearchEntry::new(),
            window: root.clone(),
        };

        // Add all desktop entries to the factory
        for entry in app_entries {
            model.entries.guard().push_back(entry);
        }

        let entries_box = model.entries.widget();
        let widgets = view_output!();

        // Update with the actual widgets from the view
        model.scrolled_window = widgets.scrolled_window.clone();
        model.search_entry = widgets.search_entry.clone();

        // Add keyboard event controller to search entry for Escape key
        let search_key_controller = gtk::EventControllerKey::new();
        let sender_clone = sender.clone();
        search_key_controller.connect_key_pressed(move |_controller, key, _code, _modifier| {
            match key {
                gtk::gdk::Key::Escape => {
                    sender_clone.input(Msg::CloseWindow);
                    gtk::glib::Propagation::Stop
                }
                _ => gtk::glib::Propagation::Proceed,
            }
        });
        widgets.search_entry.add_controller(search_key_controller);

        // Focus search entry on startup
        widgets.search_entry.grab_focus();

        // Select the first entry
        if !model.entries.is_empty() {
            model.entries.send(0, true);
        }

        // Load CSS
        let css = CssProvider::new();
        css.load_from_string(include_str!("style.css"));
        gtk::style_context_add_provider_for_display(
            &WidgetExt::display(&widgets.window),
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        // Setup layer shell
        widgets.window.init_layer_shell();
        widgets.window.set_layer(Layer::Overlay);
        widgets.window.set_exclusive_zone(-1);
        widgets
            .window
            .set_keyboard_mode(gtk4_layer_shell::KeyboardMode::Exclusive);

        // Add keyboard event controller
        let key_controller = gtk::EventControllerKey::new();
        let sender_clone = sender.clone();
        key_controller.connect_key_pressed(move |_controller, key, _code, _modifier| match key {
            gtk::gdk::Key::Up | gtk::gdk::Key::k => {
                sender_clone.input(Msg::NavigateUp);
                gtk::glib::Propagation::Stop
            }
            gtk::gdk::Key::Down | gtk::gdk::Key::j => {
                sender_clone.input(Msg::NavigateDown);
                gtk::glib::Propagation::Stop
            }
            gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter => {
                sender_clone.input(Msg::SelectEntry);
                gtk::glib::Propagation::Stop
            }
            gtk::gdk::Key::Escape => {
                sender_clone.input(Msg::CloseWindow);
                gtk::glib::Propagation::Stop
            }
            _ => gtk::glib::Propagation::Proceed,
        });
        widgets.window.add_controller(key_controller);

        // Connect to window show signal to reload entries
        let sender_clone = sender.clone();
        widgets.window.connect_show(move |_| {
            sender_clone.input(Msg::WindowShown);
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Msg, sender: ComponentSender<Self>) {
        match msg {
            Msg::NavigateUp => {
                let entries_len = self.entries.len();
                if entries_len > 0 {
                    // Deselect current
                    self.entries.send(self.selected_index, false);

                    // Move up (wrap around)
                    if self.selected_index == 0 {
                        self.selected_index = entries_len - 1;
                    } else {
                        self.selected_index -= 1;
                    }

                    // Select new
                    self.entries.send(self.selected_index, true);

                    // Update selected name
                    if let Some(entry) = self.entries.get(self.selected_index) {
                        self.selected_name = entry.entry.name.clone();
                    }

                    // Scroll to selected item
                    self.scroll_to_index(self.selected_index);
                }
            }
            Msg::NavigateDown => {
                let entries_len = self.entries.len();
                if entries_len > 0 {
                    // Deselect current
                    self.entries.send(self.selected_index, false);

                    // Move down (wrap around)
                    self.selected_index = (self.selected_index + 1) % entries_len;

                    // Select new
                    self.entries.send(self.selected_index, true);

                    // Update selected name
                    if let Some(entry) = self.entries.get(self.selected_index) {
                        self.selected_name = entry.entry.name.clone();
                    }

                    // Scroll to selected item
                    self.scroll_to_index(self.selected_index);
                }
            }
            Msg::SelectEntry => {
                if let Some(entry) = self.entries.get(self.selected_index) {
                    let exec = entry.entry.exec.clone();

                    match entry.entry.open_type {
                        OpenType::Terminal => {
                            // Launch a terminal application
                            let mut soc = niri_ipc::socket::Socket::connect().unwrap();
                            soc.send(Request::Action(Action::Spawn {
                                command: vec![
                                    "ghostty".to_string(),
                                    "-c".to_string(),
                                    exec.clone(),
                                ],
                            }))
                            .unwrap()
                            .unwrap();
                        }
                        OpenType::Graphical => {
                            // Launch a graphical application
                            let mut soc = niri_ipc::socket::Socket::connect().unwrap();
                            soc.send(Request::Action(Action::Spawn {
                                command: exec
                                    .split_whitespace()
                                    .map(|s| s.to_string())
                                    .filter(|s| !s.contains('%'))
                                    .collect(),
                            }))
                            .unwrap()
                            .unwrap();
                        }
                        OpenType::Window => {
                            // Focus a window
                            let mut soc = niri_ipc::socket::Socket::connect().unwrap();
                            soc.send(Request::Action(Action::FocusWindow {
                                id: entry.entry.exec.parse::<u64>().unwrap(),
                            }))
                            .unwrap()
                            .unwrap();
                        }
                    }
                    // Close the window
                    sender.input(Msg::CloseWindow);
                }
            }
            Msg::CloseWindow => {
                self.window.set_visible(false);
            }
            Msg::SearchChanged(query) => {
                self.search_query = query;
                self.filter_entries();
            }
            Msg::WindowShown => {
                // Reload all entries when window is shown
                self.all_entries = get_entries();
                self.search_query.clear();
                self.search_entry.set_text("");
                self.filter_entries();
                self.search_entry.grab_focus();
            }
        }
    }
}

impl App {
    fn filter_entries(&mut self) {
        // Deselect current entry before clearing
        if !self.entries.is_empty() && self.selected_index < self.entries.len() {
            self.entries.send(self.selected_index, false);
        }

        // Clear existing entries
        self.entries.guard().clear();

        if self.search_query.is_empty() {
            // Show all entries if search is empty
            for entry in &self.all_entries {
                self.entries.guard().push_back(entry.clone());
            }
        } else {
            // Use fuzzy matching to filter entries
            let matcher = SkimMatcherV2::default();
            let mut scored_entries: Vec<(i64, Entry)> = self
                .all_entries
                .iter()
                .filter_map(|entry| {
                    matcher
                        .fuzzy_match(&entry.name, &self.search_query)
                        .map(|score| (score, entry.clone()))
                })
                .collect();

            // Sort by score (highest first)
            scored_entries.sort_by(|a, b| b.0.cmp(&a.0));

            // Add filtered and sorted entries
            for (_score, entry) in scored_entries {
                self.entries.guard().push_back(entry);
            }
        }

        // Reset selection to first entry
        self.selected_index = 0;
        if !self.entries.is_empty() {
            self.entries.send(0, true);
            if let Some(entry) = self.entries.get(0) {
                self.selected_name = entry.entry.name.clone();
            }
        } else {
            self.selected_name = String::new();
        }
    }

    fn scroll_to_index(&self, index: usize) {
        let entries_box = self.entries.widget();
        let adjustment = self.scrolled_window.vadjustment();

        // Get the nth child widget
        if let Some(child) = entries_box.first_child() {
            let mut current_child = Some(child);
            let mut current_index = 0;
            let mut y_pos = 0.0;

            while let Some(widget) = current_child {
                if current_index == index {
                    // Found the target widget
                    let widget_height = widget.height() as f64;
                    let widget_height = if widget_height > 0.0 {
                        widget_height
                    } else {
                        50.0 // Fallback estimate
                    };

                    let widget_top = y_pos;
                    let widget_bottom = y_pos + widget_height;

                    let current_scroll = adjustment.value();
                    let viewport_height = adjustment.page_size();

                    let visible_top = current_scroll;
                    let visible_bottom = current_scroll + viewport_height;

                    let margin = 20.0;

                    let new_value = if widget_top < visible_top + margin {
                        // Widget is above visible area, scroll up
                        (widget_top - margin).max(0.0)
                    } else if widget_bottom > visible_bottom - margin {
                        // Widget is below visible area, scroll down
                        let max_value = adjustment.upper() - adjustment.page_size();
                        (widget_bottom - viewport_height + margin)
                            .min(max_value)
                            .max(0.0)
                    } else {
                        // Widget is already visible, don't scroll
                        current_scroll
                    };

                    if new_value != current_scroll {
                        adjustment.set_value(new_value);
                    }
                    break;
                }

                // Add this widget's height and spacing to y_pos
                let widget_height = widget.height() as f64;
                let widget_height = if widget_height > 0.0 {
                    widget_height
                } else {
                    50.0
                };
                y_pos += widget_height + entries_box.spacing() as f64;

                current_child = widget.next_sibling();
                current_index += 1;
            }
        }
    }
}

fn main() {
    let app = RelmApp::new("me.bofusland.adwlauncher");

    // Check if we're running with --gapplication-service flag
    let has_service_flag = std::env::args().nth(1) == Some("--gapplication-service".to_string());

    if !has_service_flag {
        let gtk_app = relm4::main_adw_application();

        gtk_app.connect_startup(move |app| {
            // Only print if we're the primary instance (not remote)
            if !app.is_remote() {
                println!("Please run with --gapplication-service");
            }
        });
    }

    app.run::<App>(());
}
