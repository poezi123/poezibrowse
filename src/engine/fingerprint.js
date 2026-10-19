(() => {
  const seed = __SEED__ >>> 0;
  const platform = __PLATFORM__;
  const apple = platform === 'MacIntel';
  const originals = new WeakMap();
  const nativeToString = Function.prototype.toString;
  const patch = (proto, name, make) => {
    if (!proto || typeof proto[name] !== 'function') return;
    const original = proto[name];
    const replacement = make(original);
    originals.set(replacement, original);
    try { Object.defineProperty(proto, name, { value: replacement, writable: true, configurable: true, enumerable: false }); } catch (e) {}
  };
  const getter = (proto, name, value) => {
    try { Object.defineProperty(proto, name, { get: function () { return typeof value === 'function' ? value.call(this) : value; }, configurable: true, enumerable: true }); } catch (e) {}
  };
  patch(Function.prototype, 'toString', () => function () { return nativeToString.call(originals.get(this) || this); });
  const hash = (i) => {
    let h = Math.imul(i ^ seed, 2654435761);
    h ^= h >>> 15;
    h = Math.imul(h, 2246822519);
    h ^= h >>> 13;
    return h >>> 0;
  };

  if (platform) getter(Navigator.prototype, 'platform', platform);
  getter(Navigator.prototype, 'hardwareConcurrency', 4);
  getter(Screen.prototype, 'availWidth', function () { return this.width; });
  getter(Screen.prototype, 'availHeight', function () { return this.height; });
  getter(Screen.prototype, 'availLeft', 0);
  getter(Screen.prototype, 'availTop', 0);

  const noise = (data) => {
    for (let i = 0; i < data.length; i += 4) {
      const h = hash(i);
      if ((h & 63) === 0) data[i] ^= 1 + ((h >>> 8) & 1);
    }
  };
  const ctx = window.CanvasRenderingContext2D && CanvasRenderingContext2D.prototype;
  const getImageData = ctx && ctx.getImageData;
  patch(ctx, 'getImageData', (original) => function (...args) {
    const image = original.apply(this, args);
    noise(image.data);
    return image;
  });
  const noisyCopy = (canvas) => {
    const w = canvas.width, h = canvas.height;
    if (!getImageData || !w || !h || w * h > 4000000) return canvas;
    const copy = document.createElement('canvas');
    copy.width = w;
    copy.height = h;
    const c = copy.getContext('2d');
    c.drawImage(canvas, 0, 0);
    const image = getImageData.call(c, 0, 0, w, h);
    noise(image.data);
    c.putImageData(image, 0, 0);
    return copy;
  };
  const canvas = window.HTMLCanvasElement && HTMLCanvasElement.prototype;
  patch(canvas, 'toDataURL', (original) => function (...args) { return original.apply(noisyCopy(this), args); });
  patch(canvas, 'toBlob', (original) => function (...args) { return original.apply(noisyCopy(this), args); });

  for (const gl of [window.WebGLRenderingContext, window.WebGL2RenderingContext]) {
    patch(gl && gl.prototype, 'getParameter', (original) => function (p) {
      if (p === 0x9245) return apple ? 'Apple Inc.' : 'WebKit';
      if (p === 0x9246) return apple ? 'Apple GPU' : 'WebKit WebGL';
      return original.call(this, p);
    });
  }

  const touched = new WeakSet();
  patch(window.AudioBuffer && AudioBuffer.prototype, 'getChannelData', (original) => function (...args) {
    const data = original.apply(this, args);
    if (!touched.has(data)) {
      touched.add(data);
      for (let i = 0; i < data.length; i += 97) data[i] += (hash(i) % 1000) * 1e-9;
    }
    return data;
  });
  patch(window.AnalyserNode && AnalyserNode.prototype, 'getFloatFrequencyData', (original) => function (array) {
    original.call(this, array);
    for (let i = 0; i < array.length; i += 7) array[i] += (hash(i) % 100) * 1e-4;
  });
})();
