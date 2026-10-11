// Framesmith runtime loader (page protocol 1; see docs/spec/page-protocol.md).
// It replaces the page's clocks, starts the WebAssembly runtime, hands it what
// the page declares, and applies what it computes. Logic belongs in Rust.
(() => {
  'use strict';
  const boot = window.__framesmithBoot || {};
  const realNow = performance.now.bind(performance);
  const unsupported = [];
  const failures = [];
  const note = (list, a, b) => {
    b = String(b).replace(location.origin + '/', '');
    if (!list.some(x => x[0] === a && x[1] === b)) list.push([a, b]);
  };
  const run = (fn, args) => {
    try {
      if (typeof fn === 'function') fn(...args);
    } catch (e) {
      note(failures, 'script', (e && e.message) || e);
    }
  };

  // Clocks: composition time in milliseconds, moved only by a seek.
  let now = 0;
  let nextId = 1;
  let inTimer = false;
  let frames = new Map();
  const timers = new Map();
  const add = (fn, args, wait, every) => {
    timers.set(nextId, { due: now + wait, fn, args, every });
    return nextId++;
  };
  const clocks = {
    // A timer set from inside a timer waits at least 1 ms, so a chain of
    // zero waits still moves forward in time.
    setTimeout: (fn, ms, ...args) => add(fn, args, Math.max(inTimer ? 1 : 0, +ms || 0), 0),
    setInterval: (fn, ms, ...args) => add(fn, args, Math.max(1, +ms || 0), Math.max(1, +ms || 0)),
    clearTimeout: id => void timers.delete(id),
    clearInterval: id => void timers.delete(id),
    requestAnimationFrame: fn => {
      frames.set(nextId, fn);
      return nextId++;
    },
    cancelAnimationFrame: id => void frames.delete(id),
  };
  const runTimers = to => {
    inTimer = true;
    for (;;) {
      let next = null;
      let id = 0;
      for (const [key, timer] of timers) {
        if (timer.due <= to && (!next || timer.due < next.due)) {
          next = timer;
          id = key;
        }
      }
      if (!next) break;
      now = Math.max(now, next.due);
      if (next.every) next.due += next.every;
      else timers.delete(id);
      run(next.fn, next.args);
    }
    inTimer = false;
    now = to;
  };
  const RealDate = Date;
  function FsDate(...args) {
    if (!new.target) return new RealDate(now).toString();
    return args.length ? new RealDate(...args) : new RealDate(now);
  }
  FsDate.prototype = RealDate.prototype;
  FsDate.now = () => Math.floor(now);
  FsDate.parse = RealDate.parse;
  FsDate.UTC = RealDate.UTC;
  if (boot.fault !== 'clocks') {
    RealDate.prototype.constructor = FsDate;
    Object.assign(window, clocks, { Date: FsDate });
    performance.now = () => now;
  }
  const clocksReplaced = () =>
    window.Date === FsDate &&
    Object.keys(clocks).every(name => window[name] === clocks[name]) &&
    performance.now() === now &&
    Date.now() === Math.floor(now) &&
    new Date().getTime() === Math.floor(now);

  // Uses this version refuses, seen as they happen.
  const stub = new Proxy(function () {}, {
    get: (_, key) => (typeof key === 'symbol' ? undefined : stub),
    apply: () => stub,
    construct: () => stub,
  });
  Object.defineProperty(window, 'framesmith', {
    get: () => (note(unsupported, 'FS-U017', 'framesmith'), stub),
    set: () => note(unsupported, 'FS-U017', 'framesmith'),
  });
  const animate = Element.prototype.animate;
  Element.prototype.animate = function (...args) {
    note(unsupported, 'FS-U016', 'Element.animate');
    return animate.apply(this, args);
  };
  const noteAnimations = () => {
    if (document.getAnimations().length) note(unsupported, 'FS-U016', 'an animation running in the page');
  };

  // Failures: a file that did not load, or a script that threw.
  addEventListener(
    'error',
    e => {
      const t = e.target;
      if (t && t !== window && t.localName) {
        const kind = { img: 'image', image: 'image', script: 'script' }[t.localName] || 'file';
        note(failures, kind, t.currentSrc || t.src || t.href || t.localName);
      } else {
        note(failures, 'script', e.message || e.error);
      }
    },
    true
  );
  addEventListener('unhandledrejection', e => note(failures, 'script', (e.reason && e.reason.message) || e.reason));

  const drain = () => ({
    unsupported: unsupported.splice(0).map(([code, detail]) => ({ code, detail })),
    failures: failures.splice(0).map(([kind, detail]) => ({ kind, detail })),
  });
  const news = () => {
    const d = drain();
    return d.unsupported.length || d.failures.length ? JSON.stringify(d) : '';
  };

  let wasm = null;
  let fps = 30;
  let clips = [];
  let targets = [];
  let order = [];
  let ownOpacity = [];
  const costs = [];
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  // Calls into the runtime: text in (optional), JSON out.
  const ask = (name, text, ...args) => {
    if (text != null) {
      const bytes = encoder.encode(text);
      const at = wasm.fs_input(bytes.length);
      new Uint8Array(wasm.memory.buffer, at, bytes.length).set(bytes);
    }
    const length = wasm[name](...args);
    return JSON.parse(decoder.decode(new Uint8Array(wasm.memory.buffer, wasm.fs_output(), length)));
  };
  const isOurs = a => a.name.startsWith('data-fs-');

  const start = async () => {
    if (document.readyState !== 'complete') await new Promise(done => addEventListener('load', done, { once: true }));
    const file = await fetch('/__framesmith__/runtime.wasm');
    wasm = (await WebAssembly.instantiate(await file.arrayBuffer())).instance.exports;

    // What the page declares.
    const marked = [...document.querySelectorAll('*')].filter(e => [...e.attributes].some(isOurs));
    const indexOf = new Map(marked.map((e, i) => [e, i]));
    const enclosing = e => {
      for (let p = e.parentElement; p; p = p.parentElement) if (indexOf.has(p)) return indexOf.get(p);
      return null;
    };
    const snapshot = {
      elements: marked.map(e => ({
        tag: e.localName.toLowerCase(),
        id: e.getAttribute('id'),
        attrs: [...e.attributes].filter(isOurs).map(a => ({ name: a.name, value: a.value })),
        parent: enclosing(e),
      })),
      motion: [...document.querySelectorAll('script')]
        .filter(s => (s.getAttribute('type') || '').trim().toLowerCase() === 'application/framesmith-motion+json')
        .map(s => ({ text: s.textContent })),
    };
    const composition = marked.find(e => e.hasAttribute('data-fs-composition')) || document;
    const keys = new Map();
    const keyOf = e => {
      if (!keys.has(e)) keys.set(e, targets.push(e) - 1);
      return keys.get(e);
    };
    const matches = ask('fs_read', JSON.stringify(snapshot)).map(selector => {
      try {
        return [...composition.querySelectorAll(selector)].map(keyOf);
      } catch {
        return null;
      }
    });
    const resolution = ask('fs_resolve', JSON.stringify(matches));
    clips = resolution.clips.map(i => marked[i]);
    document.querySelectorAll('[fs-shown]').forEach(e => e.removeAttribute('fs-shown'));

    // Fonts and pictures must be in before the first frame.
    const bare = family => family.replace(/["']/g, '');
    const fontFiles = new Map();
    for (const sheet of document.styleSheets) {
      for (const rule of sheet.cssRules) {
        const url = rule instanceof CSSFontFaceRule && /url\("?([^")]+)"?\)/.exec(rule.style.getPropertyValue('src'));
        if (url) fontFiles.set(bare(rule.style.getPropertyValue('font-family')), new URL(url[1], sheet.href || document.baseURI).href);
      }
    }
    const fontName = face => fontFiles.get(bare(face.family)) || face.family;
    await Promise.all([...document.fonts].map(face => face.load().catch(() => note(failures, 'font', fontName(face)))));
    await document.fonts.ready;
    const backgrounds = new Set();
    for (const e of document.querySelectorAll('*')) {
      for (const m of getComputedStyle(e).backgroundImage.matchAll(/url\("?([^")]+)"?\)/g)) backgrounds.add(m[1]);
    }
    const pictures = [...document.images].filter(i => i.getAttribute('src') || i.getAttribute('srcset'));
    for (const url of backgrounds) pictures.push(Object.assign(new Image(), { src: url }));
    await Promise.all(pictures.map(i => i.decode().catch(() => note(failures, 'image', i.currentSrc || i.src))));

    for (const e of targets) {
      const style = getComputedStyle(e);
      for (const p of ['translate', 'rotate', 'scale']) {
        if (style[p] !== 'none') note(unsupported, 'FS-U021', `${p} on ${e.id ? '#' + e.id : e.localName}`);
      }
    }
    runTimers(0);
    noteAnimations();
    return { protocol: 1, clocks: clocksReplaced(), manifest: resolution.manifest, problems: resolution.problems };
  };

  window.__framesmith = {
    protocol: 1,
    ready: start()
      .catch(e => {
        note(failures, 'script', 'the runtime did not start: ' + ((e && e.message) || e));
        return { protocol: 1, clocks: clocksReplaced(), manifest: null, problems: [] };
      })
      .then(report => JSON.stringify({ ...report, ...drain() })),
    begin: config => {
      fps = config.fps;
      order = ask('fs_begin', null, fps).map(key => targets[key]);
      ownOpacity = order.map(e => parseFloat(getComputedStyle(e).opacity));
      return news();
    },
    seek: frame => {
      const before = realNow();
      runTimers((frame * 1000) / fps);
      const callbacks = frames;
      frames = new Map();
      for (const fn of callbacks.values()) run(fn, [now]);
      const count = wasm.fs_frame(frame);
      const block = new Float64Array(wasm.memory.buffer, wasm.fs_block(), count);
      clips.forEach((e, i) => e.toggleAttribute('fs-shown', block[i] === 1));
      order.forEach((e, i) => {
        const at = clips.length + i * 6;
        const flags = block[at];
        if (flags & 3) e.style.translate = `${block[at + 1]}px ${block[at + 2]}px`;
        if (flags & 4) e.style.scale = `${block[at + 3]}`;
        if (flags & 8) e.style.rotate = `${block[at + 4]}deg`;
        if (flags & 16) e.style.opacity = `${ownOpacity[i] * block[at + 5]}`;
      });
      noteAnimations();
      costs.push(realNow() - before);
      return news();
    },
    costs: () => JSON.stringify(costs),
  };
})();
