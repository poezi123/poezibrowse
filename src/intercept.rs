use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Instant;

use gtk::glib;
use gtk::glib::translate::IntoGlib;
use gtk::prelude::*;
use soup::prelude::*;

use crate::engine::webkit::Held;

#[derive(Clone, Default)]
pub struct RequestRecord {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
}

const SECURITY_HEADERS: &[(&str, &str)] = &[
    ("content-security-policy", "CSP – schützt vor XSS und Injection"),
    ("strict-transport-security", "HSTS – erzwingt HTTPS"),
    ("x-content-type-options", "nosniff – verhindert MIME-Sniffing"),
    ("x-frame-options", "Clickjacking-Schutz (oder CSP frame-ancestors)"),
    ("referrer-policy", "steuert, was an fremde Seiten verraten wird"),
];

const PAYLOADS: &[(&str, &str)] = &[
    ("xss", "<script>alert(1)</script>\n\"><img src=x onerror=alert(1)>\n'><svg/onload=alert(1)>\njavascript:alert(1)\n\"><input autofocus onfocus=alert(1)>"),
    ("sqli", "'\n' OR '1'='1\n' OR 1=1--\n'; WAITFOR DELAY '0:0:5'--\n\" OR \"\"=\"\n1 UNION SELECT NULL--\nadmin'--"),
    ("traversal", "../../../../etc/passwd\n..%2f..%2f..%2f..%2fetc%2fpasswd\n....//....//....//etc/passwd\n%2e%2e%2f%2e%2e%2fetc%2fpasswd\n/etc/passwd%00"),
    ("cmd", "; id\n| id\n|| id\n$(id)\n`id`\n& whoami\n; sleep 5"),
    ("ssti", "{{7*7}}\n${7*7}\n<%= 7*7 %>\n#{7*7}\n{{config}}"),
    (" numbers", "0\n1\n2\n3\n5\n10\n100\n1000\n-1\n9999999"),
];

fn preset_label(id: &str) -> &'static str {
    match id {
        "xss" => "XSS (Cross-Site-Scripting)",
        "sqli" => "SQL-Injection",
        "traversal" => "Path Traversal / LFI",
        "cmd" => "Command Injection",
        "ssti" => "Template-Injection (SSTI)",
        "numbers" => "Zahlen (Fuzzing)",
        _ => "Nutzlasten",
    }
}

fn text_of(view: &gtk::TextView) -> String {
    let buf = view.buffer().unwrap_or_else(|| gtk::TextBuffer::new(None::<&gtk::TextTagTable>));
    buf.text(&buf.start_iter(), &buf.end_iter(), false).map(|s| s.to_string()).unwrap_or_default()
}

fn set_text(view: &gtk::TextView, text: &str) {
    if let Some(buf) = view.buffer() {
        buf.set_text(text);
    }
}

fn parse_headers(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.split_once(':').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

fn build_message(method: &str, url: &str, headers: &[(String, String)], body: &str) -> Result<soup::Message, String> {
    let msg = soup::Message::new(method, url).map_err(|_| format!("Ungültige URL: {url}"))?;
    if let Some(h) = msg.request_headers() {
        let mut content_type = None;
        for (k, v) in headers {
            if k.eq_ignore_ascii_case("content-type") {
                content_type = Some(v.clone());
            }
            h.append(k, v);
        }
        if !body.is_empty() {
            let ct = content_type.unwrap_or_else(|| "application/x-www-form-urlencoded".into());
            msg.set_request_body_from_bytes(Some(&ct), Some(&glib::Bytes::from(body.as_bytes())));
        }
    }
    Ok(msg)
}

fn format_response(msg: &soup::Message, body: &glib::Bytes) -> String {
    let status = msg.status().into_glib();
    let reason = msg.reason_phrase().map(|r| r.to_string()).unwrap_or_default();
    let mut out = format!("HTTP {status} {reason}\n");
    let mut present = Vec::new();
    if let Some(h) = msg.response_headers() {
        out.push_str("\n");
        h.foreach(|k, v| {
            out.push_str(&format!("{k}: {v}\n"));
            present.push(k.to_ascii_lowercase());
        });
    }
    let missing: Vec<&str> = SECURITY_HEADERS
        .iter()
        .filter(|(name, _)| !present.iter().any(|p| p == name))
        .map(|(_, desc)| *desc)
        .collect();
    out.push_str("\n──────── Sicherheits-Header ────────\n");
    if missing.is_empty() {
        out.push_str("✓ Alle geprüften Schutz-Header sind gesetzt.\n");
    } else {
        for m in &missing {
            out.push_str(&format!("✗ fehlt: {m}\n"));
        }
    }
    for leak in ["server", "x-powered-by", "x-aspnet-version"] {
        if present.iter().any(|p| p == leak) {
            out.push_str(&format!("⚠ {leak} verrät Software-Details\n"));
        }
    }
    out.push_str("\n──────── Body ────────\n");
    let text = String::from_utf8_lossy(body);
    if text.len() > 200_000 {
        out.push_str(&text[..200_000]);
        out.push_str("\n… (gekürzt)");
    } else {
        out.push_str(&text);
    }
    out
}

fn editor(label: &str, lines: i32) -> (gtk::Box, gtk::TextView) {
    let view = gtk::TextView::new();
    view.set_monospace(true);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    let scroller = gtk::ScrolledWindow::builder().min_content_height(lines * 18).build();
    scroller.add(&view);
    scroller.style_context().add_class("tool-field");
    let caption = gtk::Label::new(Some(label));
    caption.set_xalign(0.0);
    caption.style_context().add_class("tool-caption");
    let bx = gtk::Box::new(gtk::Orientation::Vertical, 2);
    bx.pack_start(&caption, false, false, 0);
    bx.pack_start(&scroller, true, true, 0);
    (bx, view)
}

fn method_combo() -> gtk::ComboBoxText {
    let combo = gtk::ComboBoxText::new();
    for m in ["GET", "POST", "PUT", "DELETE", "HEAD", "PATCH", "OPTIONS"] {
        combo.append(Some(m), m);
    }
    combo.set_active_id(Some("GET"));
    combo
}

struct InterceptUi {
    toggle: gtk::CheckButton,
    status: gtk::Label,
    method: gtk::Label,
    url: gtk::Entry,
    forward: gtk::Button,
    drop: gtk::Button,
    forward_all: gtk::Button,
    queue: RefCell<VecDeque<Held>>,
    notebook: gtk::Notebook,
    page: gtk::Widget,
}

pub struct Intercept {
    pub root: gtk::Widget,
    log: gtk::ListBox,
    records: Rc<RefCell<Vec<RequestRecord>>>,
    rep_method: gtk::ComboBoxText,
    rep_url: gtk::Entry,
    rep_headers: gtk::TextView,
    rep_body: gtk::TextView,
    ui: Rc<InterceptUi>,
    cancel: Rc<RefCell<gtk::gio::Cancellable>>,
}

impl Intercept {
    pub fn new(on_toggle: Rc<dyn Fn(bool)>) -> Rc<Intercept> {
        let session = soup::Session::new();
        let cancel: Rc<RefCell<gtk::gio::Cancellable>> = Rc::new(RefCell::new(gtk::gio::Cancellable::new()));
        let notebook = gtk::Notebook::new();
        notebook.set_scrollable(true);

        let log = gtk::ListBox::new();
        log.style_context().add_class("intercept-log");
        let log_scroller = gtk::ScrolledWindow::builder().build();
        log_scroller.add(&log);
        let records: Rc<RefCell<Vec<RequestRecord>>> = Rc::default();
        let log_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let log_hint = gtk::Label::new(Some("Alle Anfragen der aktuellen Seite. Klick lädt eine in den Repeater."));
        log_hint.set_xalign(0.0);
        log_hint.style_context().add_class("tool-caption");
        let clear = gtk::Button::with_label("Liste leeren");
        let log_head = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        log_head.pack_start(&log_hint, true, true, 0);
        log_head.pack_end(&clear, false, false, 0);
        log_box.pack_start(&log_head, false, false, 0);
        log_box.pack_start(&log_scroller, true, true, 0);
        notebook.append_page(&log_box, Some(&gtk::Label::new(Some("Verlauf"))));

        let rep_method = method_combo();
        let rep_url = gtk::Entry::new();
        rep_url.set_placeholder_text(Some("https://ziel.example/pfad"));
        rep_url.set_hexpand(true);
        let send = gtk::Button::with_label("Senden");
        send.style_context().add_class("suggested-action");
        let rep_top = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        rep_top.pack_start(&rep_method, false, false, 0);
        rep_top.pack_start(&rep_url, true, true, 0);
        rep_top.pack_start(&send, false, false, 0);
        let (headers_box, rep_headers) = editor("Header (eine pro Zeile: Name: Wert)", 5);
        let (body_box, rep_body) = editor("Body", 4);
        let (resp_box, rep_resp) = editor("Antwort", 10);
        rep_resp.set_editable(false);
        let rep_paned = gtk::Paned::new(gtk::Orientation::Vertical);
        let rep_upper = gtk::Box::new(gtk::Orientation::Vertical, 6);
        rep_upper.pack_start(&rep_top, false, false, 0);
        rep_upper.pack_start(&headers_box, true, true, 0);
        rep_upper.pack_start(&body_box, true, true, 0);
        rep_paned.pack1(&rep_upper, true, false);
        rep_paned.pack2(&resp_box, true, false);
        let rep_outer = gtk::Box::new(gtk::Orientation::Vertical, 6);
        rep_outer.set_border_width(8);
        rep_outer.pack_start(&rep_paned, true, true, 0);
        notebook.append_page(&rep_outer, Some(&gtk::Label::new(Some("Repeater"))));

        let intr_url = gtk::Entry::new();
        intr_url.set_placeholder_text(Some("https://ziel.example/suche?q=§"));
        intr_url.set_hexpand(true);
        let intr_method = method_combo();
        let intr_top = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        intr_top.pack_start(&intr_method, false, false, 0);
        intr_top.pack_start(&intr_url, true, true, 0);
        let preset = gtk::ComboBoxText::new();
        for (id, _) in PAYLOADS {
            preset.append(Some(id), preset_label(id));
        }
        preset.set_active_id(Some("xss"));
        let load_preset = gtk::Button::with_label("Liste einfügen");
        let start = gtk::Button::with_label("Starten");
        start.style_context().add_class("suggested-action");
        let preset_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let mark_hint = gtk::Label::new(Some("§ markiert die Einfügestelle"));
        mark_hint.style_context().add_class("tool-caption");
        preset_row.pack_start(&preset, false, false, 0);
        preset_row.pack_start(&load_preset, false, false, 0);
        preset_row.pack_start(&mark_hint, true, true, 0);
        preset_row.pack_end(&start, false, false, 0);
        let (payload_box, intr_payloads) = editor("Nutzlasten (eine pro Zeile)", 5);

        let store = gtk::ListStore::new(&[String::static_type(), String::static_type(), String::static_type(), String::static_type()]);
        let results = gtk::TreeView::with_model(&store);
        for (i, title) in ["Nutzlast", "Status", "Länge", "Zeit"].iter().enumerate() {
            let renderer = gtk::CellRendererText::new();
            let column = gtk::TreeViewColumn::new();
            column.set_title(title);
            CellLayoutExt::pack_start(&column, &renderer, true);
            CellLayoutExt::add_attribute(&column, &renderer, "text", i as i32);
            column.set_resizable(true);
            results.append_column(&column);
        }
        let results_scroller = gtk::ScrolledWindow::builder().build();
        results_scroller.add(&results);
        let intr_outer = gtk::Box::new(gtk::Orientation::Vertical, 6);
        intr_outer.set_border_width(8);
        intr_outer.pack_start(&intr_top, false, false, 0);
        intr_outer.pack_start(&preset_row, false, false, 0);
        intr_outer.pack_start(&payload_box, false, false, 0);
        intr_outer.pack_start(&results_scroller, true, true, 0);
        notebook.append_page(&intr_outer, Some(&gtk::Label::new(Some("Intruder"))));

        let icept_toggle = gtk::CheckButton::with_label("Anfragen abfangen");
        let icept_hint = gtk::Label::new(Some("Hält Seitenaufrufe und Formular-Sendungen an, bis du weiterleitest oder verwirfst (keine Hintergrund-/XHR-Anfragen)."));
        icept_hint.set_xalign(0.0);
        icept_hint.set_line_wrap(true);
        icept_hint.style_context().add_class("tool-caption");
        let icept_status = gtk::Label::new(Some("Aus – keine Anfrage angehalten."));
        icept_status.set_xalign(0.0);
        icept_status.style_context().add_class("tool-caption");
        let icept_method = gtk::Label::new(None);
        icept_method.set_xalign(0.0);
        let attrs = gtk::pango::AttrList::new();
        attrs.insert(gtk::pango::AttrInt::new_weight(gtk::pango::Weight::Bold));
        attrs.insert(gtk::pango::AttrFloat::new_scale(1.15));
        icept_method.set_attributes(Some(&attrs));
        let icept_url = gtk::Entry::new();
        icept_url.set_hexpand(true);
        icept_url.set_sensitive(false);
        let icept_forward = gtk::Button::with_label("Weiterleiten");
        icept_forward.style_context().add_class("suggested-action");
        icept_forward.set_sensitive(false);
        let icept_drop = gtk::Button::with_label("Verwerfen");
        icept_drop.style_context().add_class("destructive-action");
        icept_drop.set_sensitive(false);
        let icept_forward_all = gtk::Button::with_label("Alle weiterleiten");
        icept_forward_all.set_sensitive(false);
        let icept_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        icept_buttons.pack_start(&icept_forward, false, false, 0);
        icept_buttons.pack_start(&icept_drop, false, false, 0);
        icept_buttons.pack_end(&icept_forward_all, false, false, 0);
        let icept_outer = gtk::Box::new(gtk::Orientation::Vertical, 8);
        icept_outer.set_border_width(8);
        icept_outer.pack_start(&icept_toggle, false, false, 0);
        icept_outer.pack_start(&icept_hint, false, false, 0);
        icept_outer.pack_start(&gtk::Separator::new(gtk::Orientation::Horizontal), false, false, 4);
        icept_outer.pack_start(&icept_status, false, false, 0);
        icept_outer.pack_start(&icept_method, false, false, 0);
        icept_outer.pack_start(&icept_url, false, false, 0);
        icept_outer.pack_start(&icept_buttons, false, false, 0);
        let icept_scroller = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).build();
        icept_scroller.add(&icept_outer);
        notebook.append_page(&icept_scroller, Some(&gtk::Label::new(Some("Intercept"))));

        let ui = Rc::new(InterceptUi {
            toggle: icept_toggle.clone(),
            status: icept_status,
            method: icept_method,
            url: icept_url,
            forward: icept_forward,
            drop: icept_drop,
            forward_all: icept_forward_all,
            queue: RefCell::new(VecDeque::new()),
            notebook: notebook.clone(),
            page: icept_scroller.clone().upcast(),
        });

        let title = gtk::Label::new(Some("PoeziIntercept"));
        title.set_xalign(0.0);
        title.style_context().add_class("tool-title");
        let warn = gtk::Label::new(Some("Nur für autorisierte Tests gegen eigene oder freigegebene Ziele. Repeater/Intruder verbinden direkt (nicht über Tor)."));
        warn.set_line_wrap(true);
        warn.set_xalign(0.0);
        warn.style_context().add_class("tool-caption");
        let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
        root.pack_start(&title, false, false, 0);
        root.pack_start(&warn, false, false, 0);
        root.pack_start(&notebook, true, true, 0);
        root.show_all();

        let me = Rc::new(Intercept {
            root: root.upcast(),
            log: log.clone(),
            records: records.clone(),
            rep_method: rep_method.clone(),
            rep_url: rep_url.clone(),
            rep_headers: rep_headers.clone(),
            rep_body: rep_body.clone(),
            ui: ui.clone(),
            cancel: cancel.clone(),
        });

        {
            let records = records.clone();
            let log2 = log.clone();
            clear.connect_clicked(move |_| {
                records.borrow_mut().clear();
                for child in log2.children() {
                    log2.remove(&child);
                }
            });
        }
        {
            let notebook = notebook.clone();
            let weak = Rc::downgrade(&me);
            let records = records.clone();
            log.connect_row_activated(move |_, row| {
                let idx = row.index();
                if idx < 0 {
                    return;
                }
                if let (Some(me), Some(rec)) = (weak.upgrade(), records.borrow().get(idx as usize).cloned()) {
                    me.load_record(&rec);
                    notebook.set_current_page(Some(1));
                }
            });
        }
        {
            let session = session.clone();
            let cancel = cancel.clone();
            let (m, u, h, b, r) = (rep_method.clone(), rep_url.clone(), rep_headers.clone(), rep_body.clone(), rep_resp.clone());
            send.connect_clicked(move |btn| {
                let method = m.active_id().map(|s| s.to_string()).unwrap_or_else(|| "GET".into());
                let url = u.text().to_string();
                let headers = parse_headers(&text_of(&h));
                let body = text_of(&b);
                let msg = match build_message(&method, &url, &headers, &body) {
                    Ok(msg) => msg,
                    Err(e) => {
                        set_text(&r, &e);
                        return;
                    }
                };
                set_text(&r, "Sende …");
                btn.set_sensitive(false);
                let (btn, r) = (btn.clone(), r.clone());
                let started = Instant::now();
                let msg_cb = msg.clone();
                let c = cancel.borrow().clone();
                session.send_and_read_async(&msg, glib::Priority::DEFAULT, Some(&c), move |res| {
                    btn.set_sensitive(true);
                    match res {
                        Ok(bytes) => set_text(&r, &format!("(in {} ms)\n\n{}", started.elapsed().as_millis(), format_response(&msg_cb, &bytes))),
                        Err(e) => set_text(&r, &format!("Fehler: {e}")),
                    }
                });
            });
            let send2 = send.clone();
            rep_url.connect_activate(move |_| send2.emit_clicked());
        }
        {
            let (preset, payloads) = (preset.clone(), intr_payloads.clone());
            load_preset.connect_clicked(move |_| {
                if let Some(id) = preset.active_id() {
                    if let Some((_, list)) = PAYLOADS.iter().find(|(p, _)| *p == id.as_str()) {
                        set_text(&payloads, list);
                    }
                }
            });
        }
        {
            let session = session.clone();
            let cancel = cancel.clone();
            let (m, u, p, store) = (intr_method.clone(), intr_url.clone(), intr_payloads.clone(), store.clone());
            start.connect_clicked(move |_| {
                store.clear();
                let method = m.active_id().map(|s| s.to_string()).unwrap_or_else(|| "GET".into());
                let template = u.text().to_string();
                if !template.contains('§') {
                    store.insert_with_values(None, &[(0, &"Setze ein § in die URL als Einfügestelle".to_value())]);
                    return;
                }
                let payloads: Vec<String> = text_of(&p).lines().map(|l| l.to_string()).filter(|l| !l.is_empty()).take(300).collect();
                for payload in payloads {
                    let encoded = glib::Uri::escape_string(&payload, None, false);
                    let url = template.replacen('§', &encoded, 1);
                    let msg = match soup::Message::new(&method, &url) {
                        Ok(msg) => msg,
                        Err(_) => continue,
                    };
                    let iter = store.insert_with_values(None, &[(0, &payload.to_value()), (1, &"…".to_value())]);
                    let path = store.path(&iter);
                    let (store, started) = (store.clone(), Instant::now());
                    let msg_cb = msg.clone();
                    let c = cancel.borrow().clone();
                session.send_and_read_async(&msg, glib::Priority::DEFAULT, Some(&c), move |res| {
                        let Some(iter) = path.as_ref().and_then(|p| store.iter(p)) else { return };
                        match res {
                            Ok(bytes) => {
                                let status = format!("{} {}", msg_cb.status().into_glib(), msg_cb.reason_phrase().map(|r| r.to_string()).unwrap_or_default());
                                store.set(&iter, &[(1, &status.to_value()), (2, &bytes.len().to_string().to_value()), (3, &format!("{} ms", started.elapsed().as_millis()).to_value())]);
                            }
                            Err(e) => store.set(&iter, &[(1, &"Fehler".to_value()), (2, &e.to_string().to_value())]),
                        }
                    });
                }
            });
        }
        {
            let weak = Rc::downgrade(&me);
            let on_toggle = on_toggle.clone();
            ui.toggle.connect_toggled(move |t| {
                let on = t.is_active();
                on_toggle(on);
                if let Some(me) = weak.upgrade() {
                    if !on {
                        while let Some(held) = me.ui.queue.borrow_mut().pop_front() {
                            held.forward();
                        }
                        me.show_current();
                    } else {
                        me.show_current();
                    }
                }
            });
        }
        {
            let weak = Rc::downgrade(&me);
            ui.forward.connect_clicked(move |_| {
                if let Some(me) = weak.upgrade() {
                    if let Some(held) = me.ui.queue.borrow_mut().pop_front() {
                        held.forward_to(&me.ui.url.text());
                    }
                    me.show_current();
                }
            });
        }
        {
            let weak = Rc::downgrade(&me);
            ui.drop.connect_clicked(move |_| {
                if let Some(me) = weak.upgrade() {
                    if let Some(held) = me.ui.queue.borrow_mut().pop_front() {
                        held.drop_request();
                    }
                    me.show_current();
                }
            });
        }
        {
            let weak = Rc::downgrade(&me);
            ui.forward_all.connect_clicked(move |_| {
                if let Some(me) = weak.upgrade() {
                    while let Some(held) = me.ui.queue.borrow_mut().pop_front() {
                        held.forward();
                    }
                    me.show_current();
                }
            });
        }

        me
    }

    pub fn shutdown(&self) {
        if self.ui.toggle.is_active() {
            self.ui.toggle.set_active(false);
        }
        while let Some(held) = self.ui.queue.borrow_mut().pop_front() {
            held.forward();
        }
        self.cancel.borrow().cancel();
        *self.cancel.borrow_mut() = gtk::gio::Cancellable::new();
        self.records.borrow_mut().clear();
        for child in self.log.children() {
            self.log.remove(&child);
        }
        self.show_current();
    }

    pub fn is_intercepting(&self) -> bool {
        self.ui.toggle.is_active()
    }

    pub fn push_held(&self, held: Held) {
        self.ui.queue.borrow_mut().push_back(held);
        self.show_current();
        if let Some(n) = self.ui.notebook.page_num(&self.ui.page) {
            self.ui.notebook.set_current_page(Some(n));
        }
    }

    fn show_current(&self) {
        let queue = self.ui.queue.borrow();
        let count = queue.len();
        let has = count > 0;
        self.ui.forward.set_sensitive(has);
        self.ui.drop.set_sensitive(has);
        self.ui.forward_all.set_sensitive(count > 1);
        self.ui.url.set_sensitive(has);
        match queue.front() {
            Some(held) => {
                self.ui.method.set_text(&held.method);
                self.ui.url.set_text(&held.url);
                self.ui.status.set_text(&if count > 1 {
                    format!("Angehalten – {count} in der Warteschlange")
                } else {
                    "Angehalten – 1 Anfrage".into()
                });
            }
            None => {
                self.ui.method.set_text("");
                self.ui.url.set_text("");
                self.ui.status.set_text(if self.ui.toggle.is_active() {
                    "Abfangen aktiv – warte auf die nächste Anfrage."
                } else {
                    "Aus – keine Anfrage angehalten."
                });
            }
        }
    }

    fn load_record(&self, rec: &RequestRecord) {
        self.rep_method.set_active_id(Some(&rec.method));
        self.rep_url.set_text(&rec.url);
        let headers: String = rec.headers.iter().map(|(k, v)| format!("{k}: {v}\n")).collect();
        set_text(&self.rep_headers, &headers);
        set_text(&self.rep_body, "");
    }

    pub fn log_request(&self, rec: RequestRecord) {
        let mut records = self.records.borrow_mut();
        if records.len() > 500 {
            records.remove(0);
            if let Some(first) = self.log.row_at_index(0) {
                self.log.remove(&first);
            }
        }
        let label = gtk::Label::new(Some(&format!("{}  {}", rec.method, rec.url)));
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_tooltip_text(Some(&rec.url));
        let row = gtk::ListBoxRow::new();
        row.add(&label);
        row.show_all();
        self.log.add(&row);
        records.push(rec);
    }

}
