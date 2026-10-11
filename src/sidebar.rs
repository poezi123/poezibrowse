use gtk::gdk_pixbuf::{Pixbuf, PixbufLoader};
use gtk::prelude::*;
use gtk::{Align, Orientation};

const ONION: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="48" height="48"><g fill="none" stroke="#e8ecf2" stroke-width="1.6" stroke-linecap="round"><path d="M12 4.5c-1.2 1.6-1 2.6 0 3.6"/><path d="M12 8.1c4.4 1.1 7 4.3 7 7.4 0 3.2-3.1 5.4-7 5.4s-7-2.2-7-5.4c0-3.1 2.6-6.3 7-7.4z"/><path d="M12 10.5c2.6 1 4.2 3 4.2 5 0 2-1.9 3.3-4.2 3.3s-4.2-1.3-4.2-3.3c0-2 1.6-4 4.2-5z"/><path d="M12 13c1.1.6 1.7 1.5 1.7 2.5 0 .9-.8 1.6-1.7 1.6s-1.7-.7-1.7-1.6c0-1 .6-1.9 1.7-2.5z"/></g></svg>"##;
const MASK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="48" height="48"><g fill="none" stroke="#e8ecf2" stroke-width="1.6" stroke-linejoin="round"><path d="M3 9c0-1.2 1-2 2.2-1.8 2.2.4 4.4.6 6.8.6s4.6-.2 6.8-.6C20 7 21 7.8 21 9c0 4-2.2 7-5 7-1.7 0-2.6-1.6-4-1.6S9.7 16 8 16c-2.8 0-5-3-5-7z"/><circle cx="8.2" cy="11" r="1.5"/><circle cx="15.8" cy="11" r="1.5"/></g></svg>"##;

pub enum Icon {
    Theme(&'static str),
    Svg(&'static str),
}

pub const TOR_ICON: Icon = Icon::Svg(ONION);
pub const PRIVATE_ICON: Icon = Icon::Svg(MASK);

fn svg_pixbuf(svg: &str, size: i32) -> Option<Pixbuf> {
    let loader = PixbufLoader::new();
    loader.set_size(size, size);
    loader.write(svg.as_bytes()).ok()?;
    loader.close().ok()?;
    loader.pixbuf()
}

pub fn image(icon: &Icon, scale: i32) -> gtk::Image {
    match icon {
        Icon::Theme(name) => gtk::Image::from_icon_name(Some(name), gtk::IconSize::Button),
        Icon::Svg(svg) => {
            let image = gtk::Image::new();
            if let Some(pixbuf) = svg_pixbuf(svg, 16 * scale) {
                if let Some(surface) = gtk::gdk::prelude::GdkPixbufExt::create_surface(&pixbuf, scale, None::<&gtk::gdk::Window>) {
                    image.set_from_surface(Some(&surface));
                }
            }
            image
        }
    }
}

pub fn button(icon: &Icon, tooltip: &str, scale: i32) -> gtk::Button {
    let b = gtk::Button::new();
    b.set_image(Some(&image(icon, scale)));
    b.set_tooltip_text(Some(tooltip));
    b.set_relief(gtk::ReliefStyle::None);
    b.style_context().add_class("side-button");
    b
}

pub fn toggle(icon: &Icon, tooltip: &str, scale: i32) -> gtk::ToggleButton {
    let b = gtk::ToggleButton::new();
    b.set_image(Some(&image(icon, scale)));
    b.set_tooltip_text(Some(tooltip));
    b.set_relief(gtk::ReliefStyle::None);
    b.style_context().add_class("side-button");
    b
}

pub struct Panel {
    pub root: gtk::Box,
    pub search: Option<gtk::SearchEntry>,
    pub list: gtk::ListBox,
    pub footer: gtk::Box,
}

pub fn panel(title: &str, searchable: bool, empty: &str) -> Panel {
    let root = gtk::Box::new(Orientation::Vertical, 8);
    root.style_context().add_class("side-panel");
    let heading = gtk::Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.style_context().add_class("panel-title");
    root.pack_start(&heading, false, false, 0);
    let search = searchable.then(|| {
        let s = gtk::SearchEntry::new();
        s.set_placeholder_text(Some("Suchen …"));
        root.pack_start(&s, false, false, 0);
        s
    });
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.style_context().add_class("panel-list");
    let placeholder = gtk::Label::new(Some(empty));
    placeholder.set_line_wrap(true);
    placeholder.set_max_width_chars(28);
    placeholder.set_justify(gtk::Justification::Center);
    placeholder.set_margin_top(24);
    placeholder.style_context().add_class("dim-label");
    placeholder.show();
    list.set_placeholder(Some(&placeholder));
    let scroller = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).build();
    scroller.add(&list);
    root.pack_start(&scroller, true, true, 0);
    let footer = gtk::Box::new(Orientation::Horizontal, 6);
    root.pack_start(&footer, false, false, 0);
    root.set_size_request(300, -1);
    root.set_hexpand(false);
    Panel { root, search, list, footer }
}

pub struct EntryRow {
    pub row: gtk::ListBoxRow,
    pub open: gtk::EventBox,
    pub remove: gtk::Button,
}

pub fn entry_row(title: &str, url: &str, meta: Option<&str>) -> EntryRow {
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    let bx = gtk::Box::new(Orientation::Horizontal, 8);
    bx.style_context().add_class("panel-row");
    let text = gtk::Box::new(Orientation::Vertical, 1);
    let title_label = gtk::Label::new(Some(if title.trim().is_empty() { url } else { title }));
    title_label.set_xalign(0.0);
    title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title_label.set_max_width_chars(30);
    let url_label = gtk::Label::new(Some(&match meta {
        Some(m) => format!("{m} · {url}"),
        None => url.to_string(),
    }));
    url_label.set_xalign(0.0);
    url_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    url_label.set_max_width_chars(30);
    url_label.style_context().add_class("dim-label");
    url_label.style_context().add_class("small");
    text.pack_start(&title_label, false, false, 0);
    text.pack_start(&url_label, false, false, 0);
    let open = gtk::EventBox::new();
    open.add(&text);
    open.set_hexpand(true);
    open.set_tooltip_text(Some(url));
    let remove = gtk::Button::from_icon_name(Some("window-close-symbolic"), gtk::IconSize::Menu);
    remove.set_relief(gtk::ReliefStyle::None);
    remove.set_valign(Align::Center);
    remove.set_tooltip_text(Some("Entfernen"));
    bx.pack_start(&open, true, true, 0);
    bx.pack_start(&remove, false, false, 0);
    row.add(&bx);
    row.show_all();
    EntryRow { row, open, remove }
}
