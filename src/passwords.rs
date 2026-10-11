use gtk::prelude::*;
use webkit2gtk::UserContentManager;

pub fn setup_page(_ucm: &UserContentManager, _private: bool) {
}

pub fn on_button_clicked(button: &gtk::Button, page_uri: &str) {
    let host = crate::config::host_of(page_uri).unwrap_or_else(|| "dieser Seite".into());
    let label = gtk::Label::new(Some(&format!(
        "Passwortmanager kommt bald!\n\nGespeicherte Logins für {host} würden hier stehen."
    )));
    label.set_margin_top(14);
    label.set_margin_bottom(14);
    label.set_margin_start(14);
    label.set_margin_end(14);
    let popover = gtk::Popover::new(Some(button));
    popover.add(&label);
    popover.show_all();
    popover.popup();
}
