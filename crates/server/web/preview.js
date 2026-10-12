// Framesmith preview page (see docs/spec/preview.md). It fits the composition to the window and shows
// frame 0 through the same loader calls a render makes. Logic belongs in Rust.
(() => {
  'use strict';
  const page = document.body;
  const stage = document.getElementById('stage');
  const box = document.getElementById('picture');
  const view = document.getElementById('composition');
  const notice = document.getElementById('notice');
  const width = Number(page.dataset.width);
  const height = Number(page.dataset.height);
  // The page protocol version this preview speaks is written into the page by the server; none is kept here.
  const speaks = Number(page.dataset.protocol);

  // The composition keeps its declared size; the box takes the largest size of that shape that fits.
  const fit = () => {
    const room = stage.getBoundingClientRect();
    const scale = Math.min(room.width / width, room.height / height);
    box.style.width = `${width * scale}px`;
    box.style.height = `${height * scale}px`;
    box.style.left = `${(room.width - width * scale) / 2}px`;
    box.style.top = `${(room.height - height * scale) / 2}px`;
    view.style.transform = `scale(${scale})`;
    page.dataset.scale = String(scale);
  };
  new ResizeObserver(fit).observe(stage);
  fit();

  // What a report from the loader says went wrong, one line each.
  const lines = report => [
    ...(report.problems || []).map(p => `${p.code}: ${p.description}`),
    ...(report.unsupported || []).map(u => `${u.code}: ${u.detail}`),
    ...(report.failures || []).map(f => `${f.kind}: ${f.detail}`),
  ];
  // No picture is shown for a project the page cannot show correctly.
  const refuse = reasons => {
    const title = document.createElement('p');
    title.textContent = 'The preview cannot show this project.';
    const list = document.createElement('ul');
    for (const reason of reasons) list.append(Object.assign(document.createElement('li'), { textContent: reason }));
    notice.replaceChildren(title, list);
    notice.hidden = false;
    page.dataset.state = 'refused';
  };

  const show = async () => {
    const until = Date.now() + 30000;
    let loader;
    while (!(loader = view.contentWindow && view.contentWindow.__framesmith)) {
      if (Date.now() > until) throw new Error('the composition did not start');
      await new Promise(done => setTimeout(done, 20));
    }
    const report = JSON.parse(await loader.ready);
    const reasons = lines(report);
    if (report.protocol !== speaks) reasons.push(`protocol: the page speaks version ${report.protocol}, this preview speaks version ${speaks}`);
    if (report.clocks !== true) reasons.push('clocks: the clocks of the page could not be replaced');
    if (!report.manifest) reasons.push('manifest: the page could not work out its composition');
    if (reasons.length) return refuse(reasons);
    for (const answer of [loader.begin({ fps: report.manifest.fps }), loader.seek(0)]) {
      if (answer) reasons.push(...lines(JSON.parse(answer)));
    }
    if (reasons.length) return refuse(reasons);
    view.style.visibility = 'visible';
    page.dataset.frame = '0';
    page.dataset.state = 'shown';
  };
  show().catch(error => refuse([String((error && error.message) || error)]));
})();
