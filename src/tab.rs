use std::cell::{Cell, RefCell};
use std::time::Instant;

use gtk::prelude::*;
use gtk::{pango, Align, Orientation};

use crate::config::EngineKind;
use crate::engine::Page;

#[derive(Clone)]
pub struct Snapshot {
    pub uri: String,
    pub title: String,
    pub engine: EngineKind,
    pub state: Option<Vec<u8>>,
}

pub struct Tab {
    pub id: u64,
    pub container: gtk::Box,
    pub label: gtk::EventBox,
    chip: gtk::Box,
    icon: gtk::Image,
    spinner: gtk::Spinner,
    title: gtk::Label,
    audio: gtk::Button,
    pub close_button: gtk::Button,
    pub page: RefCell<Option<Box<dyn Page>>>,
    pub snapshot: RefCell<Option<Snapshot>>,
    pub last_active: Cell<Instant>,
}

impl Tab {
    pub fn new(id: u64) -> Tab {
        let container = gtk::Box::new(Orientation::Vertical, 0);
        container.show();

        let row = gtk::Box::new(Orientation::Horizontal, 6);
        row.style_context().add_class("tab-chip");
        let icon = gtk::Image::from_icon_name(Some("text-html-symbolic"), gtk::IconSize::Menu);
        let spinner = gtk::Spinner::new();
        let title = gtk::Label::new(Some("Neuer Tab"));
        title.set_ellipsize(pango::EllipsizeMode::End);
        title.set_width_chars(14);
        title.set_max_width_chars(22);
        title.set_xalign(0.0);
        title.set_hexpand(false);
        let audio = gtk::Button::from_icon_name(Some("audio-volume-high-symbolic"), gtk::IconSize::Menu);
        audio.set_relief(gtk::ReliefStyle::None);
        audio.set_tooltip_text(Some("Stummschalten"));
        audio.set_no_show_all(true);
        let close_button = gtk::Button::from_icon_name(Some("window-close-symbolic"), gtk::IconSize::Menu);
        close_button.set_relief(gtk::ReliefStyle::None);
        close_button.set_tooltip_text(Some("Tab schließen (Strg+W)"));
        close_button.set_valign(Align::Center);
        row.pack_start(&icon, false, false, 0);
        row.pack_start(&spinner, false, false, 0);
        row.pack_start(&title, true, true, 0);
        row.pack_start(&audio, false, false, 0);
        row.pack_start(&close_button, false, false, 0);

        let label = gtk::EventBox::new();
        label.set_visible_window(false);
        label.add(&row);
        label.show_all();
        spinner.hide();

        let tab = Tab {
            id,
            container,
            label,
            chip: row,
            icon,
            spinner,
            title,
            audio,
            close_button,
            page: RefCell::new(None),
            snapshot: RefCell::new(None),
            last_active: Cell::new(Instant::now()),
        };
        tab
    }

    pub fn set_active(&self, active: bool) {
        let style = self.chip.style_context();
        if active {
            style.add_class("active");
        } else {
            style.remove_class("active");
        }
    }

    pub fn is_sleeping(&self) -> bool {
        self.page.borrow().is_none()
    }

    pub fn uri(&self) -> String {
        if let Some(page) = self.page.borrow().as_ref() {
            return page.uri().unwrap_or_default();
        }
        self.snapshot.borrow().as_ref().map(|s| s.uri.clone()).unwrap_or_default()
    }

    pub fn title(&self) -> String {
        if let Some(page) = self.page.borrow().as_ref() {
            return page.title().or_else(|| page.uri()).unwrap_or_else(|| "Neuer Tab".into());
        }
        self.snapshot
            .borrow()
            .as_ref()
            .map(|s| if s.title.is_empty() { s.uri.clone() } else { s.title.clone() })
            .unwrap_or_else(|| "Neuer Tab".into())
    }

    pub fn engine(&self) -> Option<EngineKind> {
        if let Some(page) = self.page.borrow().as_ref() {
            return Some(page.engine());
        }
        self.snapshot.borrow().as_ref().map(|s| s.engine)
    }

    pub fn attach(&self, page: Box<dyn Page>) {
        let widget = page.widget();
        widget.set_vexpand(true);
        widget.set_hexpand(true);
        self.container.pack_start(&widget, true, true, 0);
        widget.show();
        *self.page.borrow_mut() = Some(page);
        self.label.style_context().remove_class("sleeping");
        self.refresh_label();
    }

    pub fn sleep(&self) -> bool {
        let Some(page) = self.page.borrow_mut().take() else { return false };
        *self.snapshot.borrow_mut() = Some(Snapshot {
            uri: page.uri().unwrap_or_default(),
            title: page.title().unwrap_or_default(),
            engine: page.engine(),
            state: page.save_state(),
        });
        page.close();
        let widget = page.widget();
        self.container.remove(&widget);
        drop(page);
        self.label.style_context().add_class("sleeping");
        self.refresh_label();
        true
    }

    pub fn refresh_label(&self) {
        let title = self.title();
        self.title.set_text(&title);
        self.label.set_tooltip_text(Some(&if self.is_sleeping() { format!("{title}\n(schläft – RAM freigegeben)") } else { title }));

        let page = self.page.borrow();
        let loading = page.as_ref().is_some_and(|p| p.is_loading());
        if loading {
            self.icon.hide();
            self.spinner.show();
            self.spinner.start();
        } else {
            self.spinner.stop();
            self.spinner.hide();
            self.icon.show();
            match page.as_ref().and_then(|p| p.favicon()) {
                Some(pixbuf) => self.icon.set_from_pixbuf(Some(&pixbuf)),
                None => self.icon.set_from_icon_name(
                    Some(if self.is_sleeping() { "weather-clear-night-symbolic" } else { "text-html-symbolic" }),
                    gtk::IconSize::Menu,
                ),
            }
        }
        let playing = page.as_ref().is_some_and(|p| p.is_playing_audio());
        let muted = page.as_ref().is_some_and(|p| p.is_muted());
        self.audio.set_visible(playing || muted);
        self.audio.set_image(Some(&gtk::Image::from_icon_name(
            Some(if muted { "audio-volume-muted-symbolic" } else { "audio-volume-high-symbolic" }),
            gtk::IconSize::Menu,
        )));
    }

    pub fn audio_button(&self) -> &gtk::Button {
        &self.audio
    }
}
