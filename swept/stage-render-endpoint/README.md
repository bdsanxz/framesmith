# A composition as a URL with a render button

The deploy templates turn a project into a preview anyone can scrub plus an API that renders it. This inspiration is that shape run locally, so it can be verified without a cloud account. It serves `../one-take-journey/` with `scripts/render-server.mjs` (nothing to copy: the server takes any project folder).

```bash
node ../../scripts/render-server.mjs ../one-take-journey --port 8787
# open http://127.0.0.1:8787/
```

Session, verified on `hyperframes@0.8.103`:

```bash
curl -s -X POST http://127.0.0.1:8787/api/render -H 'content-type: application/json' \
  -d '{"variables":{"accent":"#ff9f43"},"quality":"draft"}'
# {"id":"3e233302","status":"rendering","poll":"/api/render/3e233302"}

curl -s -X POST http://127.0.0.1:8787/api/render -H 'content-type: application/json' -d '{"variables":{"title":"nope"}}'
# {"id":"ad795f57","status":"queued",...}               queued behind the first render

curl -s http://127.0.0.1:8787/api/render/3e233302
# 202 {"status":"rendering","progress":0.9,...}
# 200 {"status":"done","url":"/renders/3e233302.mp4","bytes":2309031,"seconds":16.216}

curl -s http://127.0.0.1:8787/api/render/ad795f57
# 500 {"status":"failed","error":"... Variable \"title\" is not declared in data-composition-variables. ✗ Variable validation failed ..."}
```

What was checked:
- The MP4 probes at 17.5 s of video and 17.5 s of audio. `Range: bytes=0-99` returns `206` with `content-range`, so the browser can seek the result. `/composition/..%2F..%2FSKILL.md` returns `403`.
- In the browser, the page's `<hyperframes-player>` reported `ready`, `duration 17.5`, `1920×1080`, and seeked to 9.8 s showing the waveform region. The sub-composition mounted because the server injects the runtime into `index.html`.
- The page's own form (accent set to `#ff5fa2`, Render) polled to `done` and swapped in a `<hyperframes-player type="video/mp4">` of the result. The rendered frame's average hue moved from 267 (the amber render) to 279, so the variable reached the render.

To put the same project on the user's own cloud, follow `references/deploy.md` (Vercel, Cloudflare, or Modal template). The user signs in and deploys.
