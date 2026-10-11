use webkit2gtk::{
    UserContentInjectedFrames, UserContentManager, UserContentManagerExt, UserScript, UserScriptInjectionTime,
    UserStyleLevel, UserStyleSheet,
};

const SITES: &[&str] = &["https://*.youtube.com/*", "https://youtube.com/*", "https://*.youtube-nocookie.com/*"];

const SCRIPT: &str = r#"(() => {
  if (window.__poeziYoutube) return;
  window.__poeziYoutube = true;
  const AD_KEYS = ['adPlacements', 'adSlots', 'playerAds', 'adBreakHeartbeatParams', 'adSafetyReason'];
  const strip = (value, depth = 0) => {
    if (!value || typeof value !== 'object' || depth > 6) return value;
    for (const key of AD_KEYS) {
      if (key in value) {
        try { delete value[key]; } catch (e) {}
      }
    }
    for (const key of ['playerResponse', 'response', 'playerConfig']) {
      if (value[key] && typeof value[key] === 'object') strip(value[key], depth + 1);
    }
    return value;
  };
  const parse = JSON.parse;
  JSON.parse = function () {
    return strip(parse.apply(this, arguments));
  };
  for (const name of ['ytInitialPlayerResponse', 'ytInitialData']) {
    let stored;
    try {
      Object.defineProperty(window, name, {
        configurable: true,
        get() { return stored; },
        set(value) { stored = strip(value); },
      });
    } catch (e) {}
  }
  const nativeFetch = window.fetch;
  window.fetch = async function (input, init) {
    const response = await nativeFetch.apply(this, arguments);
    try {
      const url = typeof input === 'string' ? input : (input && input.url) || '';
      if (/\/youtubei\/v1\/(player|next|reel)/.test(url) && response.ok) {
        const data = strip(await response.clone().json());
        return new Response(JSON.stringify(data), { status: response.status, statusText: response.statusText, headers: response.headers });
      }
    } catch (e) {}
    return response;
  };
  const skip = () => {
    const player = document.querySelector('.html5-video-player.ad-showing, .html5-video-player.ad-interrupting');
    if (player) {
      const video = player.querySelector('video');
      if (video && isFinite(video.duration) && video.duration > 0) {
        video.muted = true;
        video.currentTime = video.duration;
      }
    }
    for (const button of document.querySelectorAll('.ytp-ad-skip-button, .ytp-ad-skip-button-modern, .ytp-skip-ad-button, .ytp-ad-skip-button-container button')) {
      button.click();
    }
  };
  const start = () => {
    new MutationObserver(skip).observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ['class'] });
    skip();
  };
  if (document.documentElement) start(); else document.addEventListener('DOMContentLoaded', start);
})();"#;

const STYLE: &str = "ytd-ad-slot-renderer, ytd-in-feed-ad-layout-renderer, ytd-promoted-sparkles-web-renderer, \
ytd-promoted-video-renderer, ytd-display-ad-renderer, ytd-banner-promo-renderer, ytd-statement-banner-renderer, \
ytd-companion-slot-renderer, ytd-player-legacy-desktop-watch-ads-renderer, ytd-engagement-panel-section-list-renderer[target-id='engagement-panel-ads'], \
#player-ads, #masthead-ad, .ytp-ad-overlay-container, .ytp-ad-message-container, \
ytd-rich-item-renderer:has(ytd-ad-slot-renderer), ytd-rich-section-renderer:has(ytd-statement-banner-renderer) \
{ display: none !important; }";

pub struct YoutubeBlocker {
    script: UserScript,
    style: UserStyleSheet,
}

impl YoutubeBlocker {
    pub fn new() -> YoutubeBlocker {
        YoutubeBlocker {
            script: UserScript::new(SCRIPT, UserContentInjectedFrames::AllFrames, UserScriptInjectionTime::Start, SITES, &[]),
            style: UserStyleSheet::new(STYLE, UserContentInjectedFrames::AllFrames, UserStyleLevel::User, SITES, &[]),
        }
    }

    pub fn apply(&self, ucm: &UserContentManager, enabled: bool) {
        ucm.remove_script(&self.script);
        ucm.remove_style_sheet(&self.style);
        if enabled {
            ucm.add_script(&self.script);
            ucm.add_style_sheet(&self.style);
        }
    }
}
