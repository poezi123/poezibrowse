#[cfg(feature = "cef")]
pub mod chromium;
pub mod webkit;

use std::cell::RefCell;
use std::rc::Rc;

pub enum HeldDecision {
    Forward,
    Drop,
    ForwardTo(String),
}

pub struct Held {
    pub method: String,
    pub url: String,
    action: Option<Box<dyn FnOnce(HeldDecision)>>,
}

impl Held {
    pub fn new(method: String, url: String, action: impl FnOnce(HeldDecision) + 'static) -> Self {
        Held {
            method,
            url,
            action: Some(Box::new(action)),
        }
    }

    fn dispatch(mut self, decision: HeldDecision) {
        if let Some(action) = self.action.take() {
            action(decision);
        }
    }

    pub fn forward(self) {
        self.dispatch(HeldDecision::Forward);
    }

    pub fn drop_request(self) {
        self.dispatch(HeldDecision::Drop);
    }

    pub fn forward_to(self, url: &str) {
        if url.is_empty() || url == self.url {
            self.dispatch(HeldDecision::Forward);
        } else {
            let url = url.to_string();
            self.dispatch(HeldDecision::ForwardTo(url));
        }
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        if let Some(action) = self.action.take() {
            action(HeldDecision::Forward);
        }
    }
}

pub type InterceptRoute = Rc<dyn Fn(Held)>;

pub enum PageEvent {
    TitleChanged,
    UriChanged,
    Progress(f64),
    LoadingChanged,
    LoadFinished,
    NavigationStateChanged,
    FaviconChanged,
    HoverLink(Option<String>),
    OpenInNewTab(String, bool),
    Popup(Box<dyn Page>),
    Fullscreen(bool),
    AudioChanged,
    Crashed(String),
    Blocked(String),
    Request(crate::intercept::RequestRecord),
    CloseRequested,
}

pub type EventSink = Rc<dyn Fn(PageEvent)>;

#[derive(Clone, Default)]
pub struct SinkSlot(Rc<RefCell<Option<EventSink>>>);

impl SinkSlot {
    pub fn set(&self, sink: EventSink) {
        *self.0.borrow_mut() = Some(sink);
    }

    pub fn emit(&self, event: PageEvent) {
        let sink = self.0.borrow().clone();
        if let Some(sink) = sink {
            sink(event);
        }
    }
}

pub trait Page {
    fn engine(&self) -> crate::config::EngineKind;
    fn widget(&self) -> gtk::Widget;
    fn set_event_sink(&self, sink: EventSink);

    fn load_uri(&self, uri: &str);
    fn reload(&self, bypass_cache: bool);
    fn stop(&self);
    fn go_back(&self);
    fn go_forward(&self);

    fn uri(&self) -> Option<String>;
    fn title(&self) -> Option<String>;
    fn favicon(&self) -> Option<gtk::gdk_pixbuf::Pixbuf> {
        None
    }
    fn can_go_back(&self) -> bool;
    fn can_go_forward(&self) -> bool;
    fn is_loading(&self) -> bool;

    fn zoom(&self) -> f64;
    fn set_zoom(&self, level: f64);

    fn is_playing_audio(&self) -> bool {
        false
    }
    fn is_muted(&self) -> bool {
        false
    }
    fn set_muted(&self, _muted: bool) {}

    fn find(&self, _text: &str, _backwards: bool) {}

    fn set_request_logging(&self, _on: bool) {}

    fn set_intercepting(&self, _on: bool) {}

    fn set_intercept_route(&self, _route: InterceptRoute) {}
    fn print(&self, _parent: &gtk::Window) {}
    fn show_inspector(&self) {}

    fn webview(&self) -> Option<webkit2gtk::WebView> {
        None
    }

    fn save_state(&self) -> Option<Vec<u8>> {
        None
    }
    fn restore_state(&self, _state: &[u8]) -> bool {
        false
    }

    fn close(&self) {}
}
