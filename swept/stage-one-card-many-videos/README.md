# One card, many videos

A 10s film where nothing is designed twice: `compositions/card.html` declares four variables (`label`, `title`, `detail`, `accent`) and is mounted **eight times** from `index.html`. The first instance passes no values and shows the declared defaults; three instances describe three inspirations from this skill; the closing grid is four more instances scaled to quarters. The master also declares one top-level variable (`headline`, the intro text), which batch rows change. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103`.

```bash
npx hyperframes@0.8.103 render -q draft --batch rows.json -o "renders/{name}.mp4"
# → renders/intro-four-examples.mp4, renders/intro-same-file.mp4, renders/manifest.json (status, timing, variables per row)
```

| What | How |
|---|---|
| per-instance values | `data-variable-values='{"title":"Arcade cabinet","accent":"#00E5FF",…}'` on each host; `{}` keeps the defaults |
| text | `data-var-text="title"` (children survive; the authored text is the fallback) |
| color | `var(--accent, #9aa3b5)` in CSS: every scalar variable becomes a custom property |
| grid of instances | same file, same id, each host scaled with `transform: … scale(.5)` and `transform-origin: 0 0` |
| batch | each row in `rows.json` merges over the top-level defaults; `{name}` in `-o` comes from the row |

Precedence, verified in this project:
- `--variables '{"title":"FROM THE CLI"}'` → "Variable "title" is not declared" and the cards keep their own values. CLI values reach the top-level composition only, never a scene that declares its own variables.
- By contrast, a scene that declares **no** variables reads the top-level values in its bindings (`../name-card-template/` binds the host's `name` inside `compositions/card.html`).

One build fix: the intro is a root-level clip with text content, so it had its own size and sat top-left. Root clips only get the full-frame box when they have no size, so it got `position:absolute; inset:0`.
