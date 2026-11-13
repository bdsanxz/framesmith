# Name card — a cloud-ready template

A 5s card with three declared variables (`name`, `role`, `accent`), set up so it can render anywhere: locally while authoring, or on HeyGen's managed cloud for shipping, with no change to the composition. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103`.

Template wiring:
- `<html data-composition-variables='[…]'>` declares `name` (string, max 28), `role` (string), and `accent` (color), each with a working default so preview needs no overrides.
- `data-var-text="name"` / `data-var-text="role"` substitute text with no script; the authored text is the fallback.
- `var(--accent, #7c5cff)` in CSS follows the render-time color, because every scalar variable becomes a CSS custom property.

Verified locally (the same render the cloud would run):

```bash
npx hyperframes@0.8.103 render -q draft --variables '{"name":"Grace Hopper","role":"Compiler pioneer","accent":"#ff7a45"}' -o renders/grace.mp4
npx hyperframes@0.8.103 render -q draft --variables '{"name":"Linus","role":"Kernel maintainer","accent":"#38d39f"}' -o renders/linus.mp4
npx hyperframes@0.8.103 render --strict-variables --variables '{"nmae":"Typo"}'   # ✗ Variable validation failed, no output
```

Ready for the cloud, checked without signing in:

```bash
npx hyperframes@0.8.103 cloud render . --dry-run --json
# file_count 4, size_bytes 2358, upload_limit_bytes 209715200: source/ is excluded by .hyperframesignore
```

`source/master-pattern.mp4` (not shipped) stands for a heavy master used only to make finished assets. Nothing references it, so `.hyperframesignore` keeps it out of the upload. Never ignore a file the composition references.

The cloud loop (needs the user's HeyGen sign-in):

```bash
npx hyperframes@0.8.103 auth login                         # or: echo "$HEYGEN_API_KEY" | npx hyperframes@0.8.103 auth login --api-key
npx hyperframes@0.8.103 cloud render . --quality high      # prints an asset id after upload
npx hyperframes@0.8.103 cloud render --asset-id <asset_id> --variables '{"name":"Ada"}' --idempotency-key "$(uuidgen)"
```
