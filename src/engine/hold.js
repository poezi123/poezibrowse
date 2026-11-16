(() => {
  const state = window.__poeziHold || (window.__poeziHold = { on: false, installed: false });
  state.on = true;
  if (state.installed) return;
  state.installed = true;
  const ask = (method, url) => {
    if (!state.on) return Promise.resolve('forward');
    try {
      return window.webkit.messageHandlers.poeziHold.postMessage(JSON.stringify({ method, url })).catch(() => 'forward');
    } catch (e) {
      return Promise.resolve('forward');
    }
  };
  const abs = (u) => {
    try { return new URL(u, location.href).href; } catch (e) { return String(u); }
  };
  const web = (u) => /^https?:/i.test(u);
  const target = (d) => (typeof d === 'string' && d.startsWith('to:') ? d.slice(3) : null);
  const nativeFetch = window.fetch;
  window.fetch = function (input, init) {
    let url, method;
    try {
      url = abs(typeof input === 'string' || input instanceof URL ? input : input.url);
      method = String((init && init.method) || (input && input.method) || 'GET').toUpperCase();
    } catch (e) {
      return nativeFetch.apply(this, arguments);
    }
    if (!state.on || !web(url)) return nativeFetch.apply(this, arguments);
    return ask(method, url).then((d) => {
      if (d === 'drop') throw new TypeError('Load failed');
      const to = target(d);
      if (to) return nativeFetch.call(this, input instanceof Request ? new Request(to, input) : to, init);
      return nativeFetch.call(this, input, init);
    });
  };
  const open = XMLHttpRequest.prototype.open;
  const send = XMLHttpRequest.prototype.send;
  const meta = new WeakMap();
  XMLHttpRequest.prototype.open = function (method, url, async) {
    meta.set(this, { method: String(method || 'GET').toUpperCase(), url: abs(url), sync: async === false });
    return open.apply(this, arguments);
  };
  XMLHttpRequest.prototype.send = function (body) {
    const m = meta.get(this);
    if (!state.on || !m || m.sync || !web(m.url)) return send.apply(this, arguments);
    ask(m.method, m.url).then((d) => {
      if (d === 'drop') {
        try { this.abort(); } catch (e) {}
        this.dispatchEvent(new ProgressEvent('error'));
        this.dispatchEvent(new ProgressEvent('loadend'));
        return;
      }
      try { send.call(this, body); } catch (e) {}
    });
  };
  if (navigator.sendBeacon) {
    const beacon = navigator.sendBeacon.bind(navigator);
    navigator.sendBeacon = function (url, data) {
      const u = abs(url);
      if (!state.on || !web(u)) return beacon(url, data);
      ask('POST', u).then((d) => {
        if (d !== 'drop') beacon(target(d) || url, data);
      });
      return true;
    };
  }
})();
