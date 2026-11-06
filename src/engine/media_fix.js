(() => {
  if (window.__poeziMediaFix) return;
  window.__poeziMediaFix = true;
  const vp9 = /vp0?9/i;
  const av1 = /av0?1/i;
  const blockAv1 = __BLOCK_AV1__;
  const canPlayType = HTMLMediaElement.prototype.canPlayType;
  HTMLMediaElement.prototype.canPlayType = function (type) {
    const t = String(type);
    return vp9.test(t) || (blockAv1 && av1.test(t)) ? '' : canPlayType.call(this, type);
  };
  if (blockAv1 && window.MediaSource && MediaSource.isTypeSupported) {
    const isTypeSupported = MediaSource.isTypeSupported.bind(MediaSource);
    MediaSource.isTypeSupported = (type) => (av1.test(String(type)) ? false : isTypeSupported(type));
  }
  const isWebm = (url) => /\.webm($|[?#])/i.test(url || '');
  const watched = new WeakMap();
  const check = () => {
    for (const video of document.querySelectorAll('video')) {
      if (video.paused || video.ended || !isWebm(video.currentSrc)) {
        watched.delete(video);
        continue;
      }
      const state = watched.get(video) || { time: -1, since: Date.now() };
      if (video.currentTime !== state.time || video.readyState > 2) {
        watched.set(video, { time: video.currentTime, since: Date.now() });
        continue;
      }
      if (Date.now() - state.since < 3000) continue;
      const fallback = Array.from(video.querySelectorAll('source')).find((s) => s.src && !isWebm(s.src) && !/webm/i.test(s.type || ''));
      watched.delete(video);
      if (!fallback) continue;
      const time = video.currentTime;
      video.addEventListener('loadedmetadata', () => {
        if (time > 0) video.currentTime = time;
        video.play().catch(() => {});
      }, { once: true });
      video.src = fallback.src;
      video.load();
    }
  };
  setInterval(check, 1000);
})();
