//! Error-code catalog (T007 / CAP-007).
//!
//! Every diagnostic code the system can report has one entry with a
//! meaning, a minimal example, and a fix direction. The `errors`
//! subcommand prints this catalog; `check --json` carries the same
//! codes machine-readably. No code is reported without an entry here.

/// One catalog entry: what the code means, what triggers it, how to fix it.
#[derive(Debug, Clone, Copy)]
pub struct CatalogEntry {
    pub code: &'static str,
    pub meaning: &'static str,
    pub example: &'static str,
    pub fix: &'static str,
}

/// All codes the system can emit, sorted by code.
///
/// Validation codes (`validate.rs`) first in spirit, then the FFmpeg
/// bootstrap codes (`bootstrap.rs`). The order below is alphabetical so
/// `errors` output stays stable.
pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        code: "E_AMBIGUOUS_ROOT",
        meaning: "The project holds several compositions but does not mark exactly one as the render root (`root=true`), or a `root` mark is not `true` or `false`.",
        example: "composition name=\"a\" { }  composition name=\"b\" { }   // no root marked",
        fix: "Mark exactly one composition `root=true` (for example on the main cutdown); `render` uses the root by default and `render --composition <name>` selects another.",
    },
    CatalogEntry {
        code: "E_BAD_AUDIO",
        meaning: "An `audio` track or video sound definition names an unusable trim, a `rate` outside 0.1 to 10, a volume above +12 dB, a fade outside its clip window, or an omitted `duration` with no knowable source end to infer from.",
        example: "audio path=\"s.wav\" start=0 duration=2 volume=20   // above the +12 dB ceiling",
        fix: "Point `path` at a readable supported audio file; keep `trim-start`/`trim-end` ordered seconds from 0, `rate` from 0.1 to 10, `volume` at most +12 dB (`-inf` is silence), each fade within the clip duration, and every duration-less track offering a knowable source end (or its own `trim-end`).",
    },
    CatalogEntry {
        code: "E_BAD_AUTOPLAY",
        meaning: "A `slideshow` `autoplay` duration is not a finite number > 0 (zero, negative, non-numeric, or non-finite).",
        example: "slideshow autoplay=0 { slide id=\"s1\" composition=\"intro\" }  // zero duration never auto-advances",
        fix: "Set `autoplay=<positive seconds>` (for example `autoplay=5`), or remove the key for manual advance only.",
    },
    CatalogEntry {
        code: "E_BAD_BLUR",
        meaning: "A composition `motion-blur` element is unusable: a missing or unreadable `shutter` outside 0 to 360 degrees, a missing or non-whole `samples` outside 1 to 32, an unknown `tier`, a child where the element carries attributes only, or a second `motion-blur` on one composition.",
        example: "motion-blur shutter=400 samples=8 tier=\"high\"   // the shutter angle runs 0 to 360 degrees",
        fix: "Set `shutter=<degrees from 0 to 360>`, `samples=<whole pictures from 1 to 32>`, and `tier` to one of `auto`, `high`, `medium`, or `low`; keep `motion-blur` attribute-only with at most one per composition.",
    },
    CatalogEntry {
        code: "E_BAD_BOX",
        meaning: "A box size, radius, border width, or opacity is missing, not a number, or outside its accepted range.",
        example: "box x=10 y=20 width=0 height=50 fill=\"#ff0000\" start=0 duration=1   // zero width",
        fix: "Give the box a positive `width` and `height`, a non-negative `radius` and `border-width`, and an `opacity` from 0 to 1.",
    },
    CatalogEntry {
        code: "E_BAD_CANVAS",
        meaning: "A composition width or height is not a positive whole number of pixels, or the `canvas` preset is not a known preset name.",
        example: "composition name=\"t\" width=0 height=640 duration=2 fps=30 background=\"#000\" version=1 { }",
        fix: "Set `width` and `height` to positive integers (e.g. `width=1280 height=720`), or set `canvas` to one of `1080p` (alias `fullhd`), `720p`, `square`, `portrait`, `4k` (alias `uhd`); explicit dimensions override the preset.",
    },
    CatalogEntry {
        code: "E_BAD_CAPTIONS",
        meaning: "A `captions` clip cannot build: a `src` naming no caption format (`.srt`, `.vtt`, or transcript `.json`), a missing or unreadable source or matte file, an undecodable body (bad timestamps, no cues), overlapping cues, a word outside its cue, an unknown style property or value (`position`, `font-weight`, `max-lines`, colors, opacities), an unknown `entrance`, `highlight`, or `exit` preset, a second `captions` track on one composition, a `behind-subject` order with no `matte`, or a matte that is not a readable still `.png` picture.",
        example: "captions src=\"talk.srt\" start=0 duration=4 entrance=\"whoosh\"   // unknown entrance preset",
        fix: "Point `src` at a project-relative `.srt`, `.vtt`, or transcript `.json` file (or author inline `cue` children with `start`, `end`, and `value` or `word` grandchildren); keep cue windows ordered and non-overlapping with word times inside their cue; set `position` to `rail` or `embed`, `font-weight` from 100 to 900, `max-lines` to a whole number of at least 1, and colors to `#rgb`/`#rrggbb`; pick `entrance` from the 13 entrance families, `highlight` from the 7 highlight families, and `exit` from the 11 exit families; keep one `captions` clip per composition; pair `behind-subject=true` with a readable still-PNG `matte`.",
    },
    CatalogEntry {
        code: "E_BAD_CHART",
        meaning: "A `chart` clip cannot build: an unknown `kind` (or none), an unreadable or non-UTF-8 `data` file, a data row with a missing `label` or non-finite `value`, a row child with a missing `label`, a non-finite `value`/`x`, an `x` on a non-scatter row, an unknown key, `min`/`max` off a gauge (or a `min` at or above `max`), a multi-series pie or donut, a gauge without exactly one row, a negative pie slice, or an unreadable `colors`, `background-color`, `border-width`, `border-color`, `title`, or show switch.",
        example: "chart kind=\"histogram\" start=0 duration=2 { row label=\"Q1\" value=1 }   // unknown kind",
        fix: "Set `kind` to one of `bar`, `line`, `area`, `pie`, `donut`, `scatter`, `radar`, or `gauge`; feed rows as `label,value` or `series,label,value` file lines or `row label=\"Q1\" value=12 series=\"a\"` children with finite numbers; keep `min`/`max`/`x` on gauge/scatter rows only; keep pies to one non-negative series and gauges to one row.",
    },
    CatalogEntry {
        code: "E_BAD_COLOR",
        meaning: "A color value is not a `#rgb` or `#rrggbb` hex color.",
        example: "composition name=\"t\" width=64 height=64 duration=1 background=\"red\" version=1 { }",
        fix: "Use a hex color such as `background=\"#1a1a2e\"` or `color=\"#ffffff\"`.",
    },
    CatalogEntry {
        code: "E_BAD_CONFIG",
        meaning: "A `framesmith.toml` value has the wrong shape (e.g. `ffmpeg_auto_download` is not `true` or `false`).",
        example: "ffmpeg_auto_download = maybe",
        fix: "Write `ffmpeg_auto_download = true` or `ffmpeg_auto_download = false`.",
    },
    CatalogEntry {
        code: "E_BAD_DURATION",
        meaning: "A composition `duration` is not a number, not positive, below the 1/60s minimum length, or omitted with no finite clip end to infer from.",
        example: "composition name=\"t\" width=64 height=64 duration=-2 background=\"#000\" version=1 { }",
        fix: "Set `duration=<positive seconds>` on the composition root, e.g. `duration=3` (at least 1/60s); omit it only when a clip bounds the timeline so the composition can end at the latest finite clip end.",
    },
    CatalogEntry {
        code: "E_BAD_EASING",
        meaning: "A `tween`, `key`, or `frame` names an unknown easing, or a parameterized easing carries missing, non-numeric, or out-of-range parameters; the system never guesses a curve.",
        example: "tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"fast\"   // unknown easing",
        fix: "Set `ease=` to one of the nineteen named curves (e.g. `linear`, `in-out-sine`), `ease=\"out-back(1.7)\"` with a numeric overshoot, or a full-curve spelling: `cubic-bezier(x1,y1,x2,y2)`, `steps(n)`, `spring` or `spring(stiffness,damping)`, `bounce`, `elastic`, `wiggle` or `wiggle(n)`, or `hold`.",
    },
    CatalogEntry {
        code: "E_BAD_EFFECT",
        meaning: "A clip `effect` is unusable: an unknown preset, an `amount` outside 0 to 1, a parameter outside its preset's accepted shape, a parameter the preset does not read, or a child where the effect carries attributes only.",
        example: "text value=\"Hi\" start=0 duration=1 { effect preset=\"vintage\" amount=2 }   // unknown preset, amount above the range",
        fix: "Set `preset` to one of `grayscale`, `sepia`, `invert`, `brightness`, `contrast`, `hue-rotate`, `saturate`, `blur`, `sharpen`, `vignette`, `grain`, `noise`, `pixelate`, `posterize`, `threshold`, `edge-detect`, `emboss`, or `chromatic-aberration`; keep `amount` from 0 to 1; give each preset only its own parameters (`brightness`/`contrast`/`saturate` factors from 0 to 4, `angle` from 0 to 360, `radius` from 0 to 64 pixels, `strength` from 0 to 4, `softness` and `threshold` from 0 to 1, whole-pixel `size` from 1 to 64, a whole-number `seed`, whole `levels` from 2 to 16, `distance` from 0 to 32 pixels), and keep `effect` children at the clip level with no children of their own.",
    },
    CatalogEntry {
        code: "E_BAD_FILL",
        meaning: "A gradient or image fill on a `box` is unusable: a `linear-gradient` or `radial-gradient` carries no stops, an unordered, duplicated, or out-of-range stop, a first stop past offset 0 or a last stop short of offset 1, or a `stop` outside a gradient; an `image-fill` names a missing `src`, an unreadable file, or an `object-fit`/`object-position` outside the accepted sets.",
        example: "box x=0 y=0 width=100 height=50 start=0 duration=1 { linear-gradient { stop offset=0 color=\"#ff0000\" stop offset=0.5 color=\"#00ff00\" } }   // last stop is not at offset 1",
        fix: "Order `stop` children from offset 0 to offset 1 with unique in-range offsets and `#rgb`/`#rrggbb` colors; point `image-fill` at a readable PNG, JPEG, still GIF, or HEIC file with `object-fit` of `cover`, `contain`, or `fill` and an `object-position` like `center`.",
    },
    CatalogEntry {
        code: "E_BAD_FIT",
        meaning: "A video `fit` is not exactly one of `contain`, `cover`, or `fill`.",
        example: "video path=\"media/clip.mp4\" start=0 duration=2 fit=\"zoom\"",
        fix: "Set `fit=\"contain\"`, `fit=\"cover\"`, or `fit=\"fill\"`.",
    },
    CatalogEntry {
        code: "E_BAD_FLAG",
        meaning: "A composition `autoplay` switch is not `true` or `false`; the playback hint is never guessed.",
        example: "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 autoplay=\"yes\" { }   // not a boolean",
        fix: "Set `autoplay=true` to hint auto-playback, or `autoplay=false` (or remove the key) for the default still start.",
    },
    CatalogEntry {
        code: "E_BAD_FLEX",
        meaning: "A `flex` container or child carries an unusable layout value: a missing `flex-direction`, an unknown direction or alignment, a negative or non-numeric `gap`/`padding`/`margin`/`flex-grow`, a missing or non-positive size, a misplaced paint or motion key, or a non-`box` child.",
        example: "flex display=\"flex\" flex-direction=\"diagonal\" gap=-4 { box width=50 height=50 }   // unknown direction, negative gap",
        fix: "Set `flex-direction=\"row\"` or `\"column\"`, `align-items`/`justify-content` to `start`, `center`, or `end`, spacings to numbers >= 0, sizes to positive pixels, and lay out `box` children only; move paint keys onto the child boxes.",
    },
    CatalogEntry {
        code: "E_BAD_FONT",
        meaning: "A declared font file is not a supported font format, or a text `font-family` names no usable family; the renderer draws only TrueType/OpenType fonts with `glyf` outlines and never substitutes a different font silently.",
        example: "font path=\"fonts/body.woff\"   // WOFF is a web format the renderer cannot draw",
        fix: "Point `path` at a TrueType (.ttf) or OpenType (.otf) font with TrueType outlines, or remove the `font` to use the host system font (bitmap fallback only when the machine has none).",
    },
    CatalogEntry {
        code: "E_BAD_FOOTAGE",
        meaning: "A `video` clip names unusable footage intent: a backward or empty trim window, a `trim-start` at or past the source end, a `rate` outside 0.1 to 10, a non-boolean `loop`, a `loop=true` clip or infinitely looping GIF without an explicit `duration` (an unbounded length), or an omitted `duration` with no knowable source end to infer from.",
        example: "video path=\"clip.mp4\" start=0 duration=2 trim-start=3 trim-end=1 rate=20   // backward window, rate above the ceiling",
        fix: "Keep `trim-start`/`trim-end` ordered seconds from 0 with `trim-start` below the source end, `rate` from 0.1 to 10 (`rate=2` is double speed, `rate=0.5` is half speed), `loop` as `true`/`false`, every looping clip and infinite GIF carrying an explicit `duration`, and every duration-less clip offering a knowable source end (or its own `trim-end`).",
    },
    CatalogEntry {
        code: "E_BAD_FPS",
        meaning: "The composition `fps` is not an integer or exact rational within 1 to 240; decimal fps is rejected, never rounded.",
        example: "composition name=\"t\" width=64 height=64 duration=1 fps=29.97 background=\"#000\" version=1 { }",
        fix: "Use an integer like `fps=30` or an exact rational like `fps=\"30000/1001\"` within 1 to 240.",
    },
    CatalogEntry {
        code: "E_BAD_GRADE",
        meaning: "A `grade` or `lane` look is unusable: an unknown preset, an adjust outside its accepted range, a malformed curve list, an out-of-range detail value (including a `lut` amount outside 0 to 1), a bad grain seed or size, an unparsable or oversized LUT grid, a duplicated `grade` child, `lane` element, or detail stage, a `lane` with a missing or unknown `track`, or a child where the look carries none.",
        example: "box x=0 y=0 width=50 height=50 start=0 duration=1 { grade preset=\"vintage\" exposure=9 }   // unknown preset, exposure above the range",
        fix: "Set `preset` to one of `none`, `filmic`, `noir`, `teal-orange`, `pastel`, or `bleach`; keep `exposure` from -4 to 4, `contrast` from -1 to 1, `saturation` from -1 to 2, `temperature` and `tint` from -1 to 1; shape detail with at most one `wheel` (`shadows`, `midtones`, `highlights` from -1 to 1), one `curve` (`red`, `green`, `blue`, `master` point lists like `\"0,0 0.5,0.5 1,1\"`), one `hsl` (`hue-min`/`hue-max` from 0 to 360, `saturation` from 0 to 1, `lightness` from -1 to 1), one `vignette` (`amount`, `softness` from 0 to 1), one `grain` (`amount` from 0 to 1, `size` 1 to 64 whole pixels, whole-number `seed`), and one `lut` (`src` naming a readable 3D `.cube` with a grid of 33 or smaller, `amount` from 0 to 1) child; keep one `grade` child per clip, lane, or composition and one `lane` per track with `track` of `video`, `audio`, `caption`, or `graphics`.",
    },
    CatalogEntry {
        code: "E_BAD_HIDDEN",
        meaning: "A clip `hidden` switch is not `true` or `false`; hidden clips are still validated, never guessed.",
        example: "text value=\"Hi\" start=0 duration=1 hidden=\"yes\"   // not a boolean",
        fix: "Set `hidden=true` to hide the clip, or `hidden=false` (or remove the key) to show it.",
    },
    CatalogEntry {
        code: "E_BAD_ID",
        meaning: "A clip or composition `id` is not 1-64 letters, digits, `-` or `_`; a `start-after` names the clip's own id; or a value that must be a quoted id is not one.",
        example: "text value=\"Hi\" start=0 duration=1 id=\"has space\"   // spaces are not id characters",
        fix: "Set `id` to 1-64 letters, digits, `-` or `_` (e.g. `id=\"title-1\"`); point `start-after` at a different declared id.",
    },
    CatalogEntry {
        code: "E_BAD_IMAGE",
        meaning: "An `image` or `svg` position, size, or placement is not a usable value, the file is corrupt or not a PNG/JPEG/still-GIF/HEIC/SVG picture, or the file is an animated GIF (animations play in a `video` clip, not `image`). An `svg` `width` or `height` is missing, zero, or negative.",
        example: "image path=\"media/photo.png\" start=0 duration=2 placement=\"north\"   // unknown placement",
        fix: "Use numbers for `x`, `y`, `width`, and `height`, a placement like `placement=\"top-left\"`, an undamaged PNG, JPEG, still GIF, HEIC, or SVG file — and a `video` clip for animated GIFs. Give `svg` a positive `width` and `height` (for example `width=48 height=32`).",
    },
    CatalogEntry {
        code: "E_BAD_LABEL",
        meaning: "A clip `label` is not a quoted editor note.",
        example: "text value=\"Hi\" start=0 duration=1 label=5   // labels must be quoted strings",
        fix: "Set `label=\"...\"` to a quoted editor note, or remove the key; labels never change rendered output.",
    },
    CatalogEntry {
        code: "E_BAD_LOTTIE",
        meaning: "A `lottie` position, size, placement, or loop mode is not a usable value, or the file is not a readable `.json` Lottie animation or `.lottie` dotLottie pack (or holds content outside the native shape-layer subset). A `lottie` `width` or `height` is missing, zero, or negative.",
        example: "lottie path=\"media/anim.json\" start=0 duration=2 loop-mode=\"bounce\"   // unknown loop mode",
        fix: "Use numbers for `x`, `y`, `width`, and `height`, a placement like `placement=\"top-left\"`, a loop mode of `loop-mode=\"none\"`, `\"loop\"`, or `\"ping-pong\"` (or omit it to keep the file setting), and an undamaged `.json` or `.lottie` file holding shape layers with rects, ellipses, fills, strokes, and transforms. Give `lottie` a positive `width` and `height` (for example `width=512 height=512`).",
    },
    CatalogEntry {
        code: "E_BAD_MOTION",
        meaning: "A `tween`, `set`, or `key` names no target clip, names an unusable property (or a property its clip kind does not carry), uses an unknown form, carries a missing, mistimed, or unreadable window or value, or (for `key`) chains fewer than two stops, unordered stops, or duplicate stop times; or a motion assertion (`assert-appear`, `assert-inside`, `assert-moving`) misses its deadline, carries a bad time, an unknown key, a duplicate, children, or lives on an element with no picture (audio, flex child); or a visual clip carries an unreadable 3D turn (`rotation-x`/`rotation-y` degrees, a non-positive `perspective`, an unknown `origin`) or a malformed `filter`.",
        example: "tween property=\"x\" kind=\"to\" start=0 duration=1 to=10   // missing target",
        fix: "Name a clip with `target=<index from 0>`, a property with `property=\"x\"`, `\"y\"`, `\"scale\"`, `\"rotation\"`, `\"opacity\"`, `\"rotation-x\"`, `\"rotation-y\"`, `\"width\"`, `\"height\"`, `\"fill\"`, `\"radius\"`, `\"border-width\"`, `\"border-color\"`, `\"blur\"`, `\"font-size\"`, or `\"letter-spacing\"`, a form with `kind=\"to\"`, `\"from\"`, or `\"from-to\"`, an absolute `start` with a positive `duration`, and the values the form needs (`to`, `from`, or both; `value` for `set`; hex colors for `fill`/`border-color`). Chain keyframes as `key` elements with at least two `frame at=<seconds> value=<number>` stops in rising time order. Keep `rotation-x`/`rotation-y` to finite degrees, `perspective` to positive pixels (or omit it for flat), `origin` to a placement like `\"center\"` or `\"top-left\"`, and `filter` to `\"none\"` or `\"blur(N)\"`.",
    },
    CatalogEntry {
        code: "E_BAD_ORDER",
        meaning: "A clip paint `order` is not a whole number; the system never guesses a stacking arrangement.",
        example: "text value=\"Hi\" start=0 duration=1 order=\"front\"   // not a number",
        fix: "Set `order=<integer>` (for example `order=1`); clips with equal or absent orders paint in document order.",
    },
    CatalogEntry {
        code: "E_BAD_PAINT",
        meaning: "A `box` paint-stack value is unusable: a negative `stroke-width`, an unknown `stroke-position`, `mix-blend-mode`, or `overflow`, a malformed `box-shadow`, a negative `backdrop-blur` or a negative `backdrop-brightness`, or a `mask`/`clip-path` naming no box, a non-box clip, or a reference cycle.",
        example: "box x=0 y=0 width=100 height=50 start=0 duration=1 stroke-width=-2 mix-blend-mode=\"darken\" mask=\"ghost\"   // negative stroke, unknown blend, dangling mask",
        fix: "Set `stroke-width=<pixels >= 0>`, `stroke-position` to `center`, `inside`, or `outside`, `mix-blend-mode` to `normal`, `multiply`, `screen`, or `overlay`, `overflow` to `visible`, `hidden`, or `clip`, `box-shadow` to `\"<offset-x> <offset-y> <blur> <spread> <#color>\"` (or `\"none\"`), `backdrop-blur=<pixels >= 0>`, `backdrop-brightness=<factor >= 0>`, and point `mask`/`clip-path` at the `id` of a `box` clip in the same composition with no cycles.",
    },
    CatalogEntry {
        code: "E_BAD_PATH",
        meaning: "A `path` shape is unusable: `d` names no commands, carries an unsupported command, a bad number, or a truncated command, or `fill-rule` is not `nonzero` or `evenodd`.",
        example: "path d=\"M10,10 X20,20\" start=0 duration=1   // X is not a path command",
        fix: "Write `d` with `M`, `L`, `C`, `Q`, `A`, and `Z` commands carrying plain numbers (for example `d=\"M10,10 L90,10 L90,90 Z\"`), and set `fill-rule` to `\"nonzero\"` or `\"evenodd\"`.",
    },
    CatalogEntry {
        code: "E_BAD_SCENE3D",
        meaning: "A `scene3d` clip cannot build: an unknown element key, a child that is not `object`, `camera`, or `light` (plus shared `grade`/`effect`/`vfx`/assertion children), no `object` at all, an object with a missing or unknown `kind` or `name`, an unusable `position`, `rotation`, `size`, `radius`, `height`, `segments`, `color`, `metalness`, `roughness`, `vertices`, or `faces` value (meshes need well-formed `x,y,z; ...` vertices, 0-based `a,b,c; ...` faces inside the vertex list, and non-degenerate triangles), an unusable camera `mode`, `target`, `position`, `distance`, `azimuth`, `elevation`, or `fov` (keys need finite `at` seconds and inheritable snapshots, with no duplicate times), or a light with a missing or unknown `kind` or `name` or an unusable `position`, `direction`, `color`, or `intensity`.",
        example: "scene3d start=0 duration=2 width=320 height=240 { object name=\"cube\" kind=\"torus\" }   // `torus` is not an authored primitive",
        fix: "Give the scene at least one `object` with a unique `name` and `kind` of `box`, `sphere`, `plane`, `cylinder`, or `mesh`; write triplets as `\"x,y,z\"`, sizes and radii as positive numbers, `segments` as a whole number of at least 3, colors as `#rgb`/`#rrggbb`, `metalness`/`roughness` from 0 to 1, mesh lists as `\"x,y,z; ...\"` vertices with 0-based `\"a,b,c; ...\"` faces; set camera `mode` to `orbit` or `fixed` with finite framing numbers and distinct key times; set light `kind` to `point` or `directional` with finite placement numbers. A scene with no `light` renders with the named default light, never darkness.",
    },
    CatalogEntry {
        code: "E_BAD_SLIDESHOW",
        meaning: "A `slideshow` manifest is unusable: more than one `slideshow` node per project, a slide with a missing `id` or `composition`, a duplicate slide id, a `fragment` without `at`, a `branch` without `to`, a `hotspot` without `id`/`target`/coordinates, an unknown slideshow key, or an embedded composition where only a reference is allowed.",
        example: "slideshow { slide composition=\"intro\" }  // missing slide id",
        fix: "Keep one `slideshow autoplay=<seconds> { slide id=\"s1\" composition=\"intro\" notes=\"...\" { fragment at=1 branch to=\"s2\" hotspot id=\"h1\" x=0 y=0 target=\"s2\" } }` per project; give every slide an `id` and `composition`, every fragment an `at`, every branch a `to`, every hotspot an `id` and `target`, and never nest a composition inside a slide.",
    },
    CatalogEntry {
        code: "E_BAD_SPACING",
        meaning: "A text `line-height` or `letter-spacing` is not a non-negative number of canvas pixels.",
        example: "text value=\"Hi\" start=0 duration=1 letter-spacing=-2   // negative spacing",
        fix: "Set `line-height=<pixels >= 0>` and `letter-spacing=<pixels >= 0>`, or remove the key for the font default.",
    },
    CatalogEntry {
        code: "E_BAD_STYLE",
        meaning: "A `style` block or `style=` reference names an unusable shared look: an unknown style name, a duplicated block name, a `style` key on a `style` block, an empty reference, or a property outside the shared-look vocabulary.",
        example: "box style=\"ghost\" x=0 y=0 width=50 height=50 start=0 duration=1   // no style is called `ghost`",
        fix: "Declare each look once as `style name=\"...\" ...` with properties from the valid set, and point `style=\"...\"` at declared names (space-separated for several; later names win, element attributes win over all styles).",
    },
    CatalogEntry {
        code: "E_BAD_TEXT",
        meaning: "A `text` clip carries an unusable styled run: an unknown `direction`, a `font-weight` outside 100 to 900, a non-positive `font-size`, a non-boolean `italic`, a `span` with no text and no nested spans, a non-`span` child, or a split, typewriter, or count effect combined with `span` runs.",
        example: "text value=\"Hi\" start=0 duration=1 direction=\"sideways\" { span value=\"Hi\" font-weight=50 }   // unknown direction, weight below the range",
        fix: "Set `direction` to `ltr`, `rtl`, or `auto`, `font-weight` from 100 to 900, `font-size` to positive pixels, `italic` to `true`/`false`, and give every `span` a `value` or nested `span` children; keep split, typewriter, and count effects off text with spans.",
    },
    CatalogEntry {
        code: "E_BAD_TIMING",
        meaning: "A clip start, duration, or relative start is unusable: a missing or negative absolute start, a non-positive duration, a written or resolved length below the 1/60s minimum, an `end=` key, an absolute `start` combined with `start-after` on one clip, a `start-after` cycle or an effective start before zero, a non-numeric `start-offset` (or one without `start-after`), an unbounded `nest` with no written `duration`, a `nest` trim/rate problem (a backward or empty window, a trim below zero, a `rate` outside 0.1 to 10, or a `trim-start` at or past the child end), a circular `nest` chain, or a `nest` chain deeper than 8 levels.",
        example: "text value=\"Hi\" duration=2   // missing start",
        fix: "Declare an absolute `start=<seconds >= 0>` on every clip; write `duration=<seconds > 0>` (at least 1/60s) on every clip but `video`, `audio`, and the stills (which may omit `duration` to infer it) and on every `nest`; use `start-after` without `start`, keep chains cycle-free and at zero or later, pair `start-offset` with a numeric seconds value, and keep `nest` windows forward inside the child timeline with `rate` from 0.1 to 10, chains cycle-free, and at most 8 levels deep.",
    },
    CatalogEntry {
        code: "E_BAD_TRACK",
        meaning: "A clip `track` is not exactly one of `video`, `audio`, `caption`, or `graphics`. Undeclared tracks default by kind (video to video, audio to audio, everything else to graphics).",
        example: "text value=\"Hi\" start=0 duration=1 track=\"titles\"   // unknown lane",
        fix: "Set `track` to `video`, `audio`, `caption`, or `graphics`, or remove the key for the kind default; tracks never change rendered output.",
    },
    CatalogEntry {
        code: "E_BAD_TRANSITION",
        meaning: "A `transition` bridge is unusable: a missing or mis-shaped `from`/`to` id, a missing, unreadable, or non-positive `duration`, an unknown `kind`, a child where the transition carries attributes only, a bridge from a clip to itself, two transitions sharing a clip whose windows overlap, or a `duration` longer than either clip's range.",
        example: "transition from=\"a\" to=\"b\" duration=2 kind=\"fade\"   // clips `a`/`b` run 1s each, so the 2s bridge has no handles",
        fix: "Point `from` and `to` at two different declared clip ids; set `duration=<positive seconds>` fitting inside both clips; set `kind` to one of `fade`, `dissolve`, `wipe-left`, `wipe-right`, `wipe-up`, `wipe-down`, `slide`, `push`, `cover`, `circle-open`, `iris`, `dip-black`, `dip-white`, `blur-dissolve`, `css-fade-up`, `css-fade-down`, `css-zoom-in`, `css-zoom-out`, `css-slide-left`, or `css-slide-right`; keep one transition per boundary at a time.",
    },
    CatalogEntry {
        code: "E_BAD_VARIABLE",
        meaning: "A `variable` declaration names an unknown type, a default outside its type, an `enum` without at least two `options` (or a default outside them), `options` on a non-`enum` type, or carries children; a `var(--id)` binding names an unknown or valueless variable, uses bad `var()` syntax, sits in a structural reference (`id`, `track`, `start-after` targets) or the composition timing foundation (`name`, `width`, `height`, `duration`, `fps`), or substitutes a value outside the attribute's shape; a `--variables` override value sits outside its variable's type; a `theme` block misnames itself, repeats a theme or token name, or holds non-`token` children; a composition selects two themes or an unknown theme; a `--theme` flag names no declared theme; a `var(--x)` resolves through nothing (no override, no theme token, no default, no fallback); or, under `--strict-variables`, an override key names no declared variable, a declaration is never bound by `var(--id)`, or a default-less variable received no caller value.",
        example: "variable id=\"accent\" type=\"color\" default=\"red\"   // not a `#rgb`/`#rrggbb` color",
        fix: "Set `type` to one of `string`, `number`, `color`, `boolean`, `enum`, `font`, or `image`; match the `default` to the type's shape (`#rgb`/`#rrggbb` colors, decimals, `true`/`false`, an entry of `options`, a quoted font family list, a readable project-relative image path); give every `enum` at least two `options`. Write bindings as `var(--id)` or `var(--id, fallback)` in text, media, and ordinary attributes (including motion values), never in `id`, `track`, `start-after`, or composition timing keys. Pass `--variables '{\"id\":value}'` with one scalar (string, number, or boolean) per key matching the variable's type; unknown keys only warn. Group shared looks in `theme name=\"...\" { token id=\"...\" value=\"...\" }`, select one per render with the composition `theme` attribute or `--theme`; a composition selects at most one theme and the name must match a declared block. Under `--strict-variables`, supply the value, remove the key, bind the variable, or drop the flag for warning-only mode.",
    },
    CatalogEntry {
        code: "E_BAD_VERSION",
        meaning: "The notation `version` is missing or is an integer below 1.",
        example: "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=0 { }",
        fix: "Set `version=1` on the composition root.",
    },
    CatalogEntry {
        code: "E_BAD_VFX",
        meaning: "A clip `vfx` chain is unusable: an unknown stage, a duplicated stage or `vfx` child, an `amount` outside 0 to 1, a parameter outside its stage's accepted shape, a `luma-matte` missing its required `src`, or a stage child where the stage carries attributes only.",
        example: "box x=0 y=0 width=50 height=50 start=0 duration=1 { vfx { noise amount=2 } }   // amount above the range",
        fix: "Hold at most one `vfx` child per clip with each of `noise`, `warp`, `displacement`, and `luma-matte` at most once; keep `amount` from 0 to 1, `grid` from 1 to 64 whole pixels, `strength` and `scale` from 0 to 64 pixels, a whole-number `seed`, and `map`/`src` pointing at readable picture files with `luma-matte` always carrying `src`.",
    },
    CatalogEntry {
        code: "E_DOWNLOAD_FAILED",
        meaning: "The pinned FFmpeg self-download failed: the transfer was interrupted, the SHA256 did not match, or the downloaded binary failed verification.",
        example: "render refuses: could not self-provide FFmpeg: checksum mismatch for the pinned build ...",
        fix: "Re-run the render to retry the download; if the failure persists, install FFmpeg manually (see the pinned source in the refusal) and re-run.",
    },
    CatalogEntry {
        code: "E_DUP_ID",
        meaning: "An `id` is declared more than once in the project; ids must be unique across the whole project (clips and compositions share one namespace).",
        example: "text value=\"A\" start=0 duration=1 id=\"t\"  text value=\"B\" start=1 duration=1 id=\"t\"   // reused id",
        fix: "Rename all but one `id=\"t\"` to unique ids so every clip and composition has its own name.",
    },
    CatalogEntry {
        code: "E_EMPTY_DECK",
        meaning: "A `slideshow` manifest declares no slides, so present or render would have nothing to show.",
        example: "slideshow autoplay=5 { }  // no slide children",
        fix: "Add at least one `slide id=\"...\" composition=\"...\"` child to the `slideshow` block.",
    },
    CatalogEntry {
        code: "E_MISSING_COMPOSITION",
        meaning: "The project folder has no readable `composition.kdl`.",
        example: "(empty folder) $ framesmith check demo  ->  E_MISSING_COMPOSITION",
        fix: "Create the composition with `framesmith init <name>` or restore `composition.kdl` in the folder.",
    },
    CatalogEntry {
        code: "E_MISSING_DIR",
        meaning: "The folder passed to `check` or `render` does not exist.",
        example: "$ framesmith check ghost  ->  E_MISSING_DIR",
        fix: "Point the command at an existing folder, or create one with `framesmith init <name>`.",
    },
    CatalogEntry {
        code: "E_MISSING_FIELD",
        meaning: "A required field is absent: composition `name`, `background`, `version`, clip `value`/`path`, and similar.",
        example: "composition width=64 height=64 duration=1 background=\"#000\" version=1 { }   // missing name",
        fix: "Add the named field, e.g. `name=\"...\"` on the composition root or `value=\"...\"` on a text clip.",
    },
    CatalogEntry {
        code: "E_MISSING_FILE",
        meaning: "A referenced media or font file does not exist or is not readable at the declared path.",
        example: "video path=\"media/gone.mp4\" start=0 duration=2   // file absent",
        fix: "Provide the file at the referenced path relative to the project folder, or fix the `path`.",
    },
    CatalogEntry {
        code: "E_NO_FFMPEG",
        meaning: "No suitable FFmpeg is available and the pinned self-download is disabled or offline, so rendering refuses before the first frame.",
        example: "render refuses: no suitable FFmpeg ... Self-download is disabled by framesmith.toml ...",
        fix: "Install FFmpeg 6+ with libx264 and aac encoders on PATH, or re-enable downloading (`ffmpeg_auto_download = true`).",
    },
    CatalogEntry {
        code: "E_PARSE",
        meaning: "The KDL notation or a `framesmith.toml` config line is malformed at the reported location.",
        example: "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {   // unclosed brace",
        fix: "Fix the notation syntax at the reported line and column; config lines must be `key = value`.",
    },
    CatalogEntry {
        code: "E_UNKNOWN_ELEMENT",
        meaning: "A top-level node is not `composition`, or a child element is not a known element name.",
        example: "composition name=\"t\" ... { }  subtitle value=\"Hi\" start=0 duration=1",
        fix: "Keep a single `composition` root and use known elements (`text`, `font`, `video` in Sprint 1).",
    },
    CatalogEntry {
        code: "E_UNKNOWN_KEY",
        meaning: "A key is not part of the notation (composition, clip, or config keys); unknown keys are errors, never ignored.",
        example: "composition name=\"t\" width=64 height=64 duration=1 bogus=1 background=\"#000\" version=1 { }",
        fix: "Remove the key or check the spelling against the documented keys.",
    },
    CatalogEntry {
        code: "E_UNKNOWN_REF",
        meaning: "A `start-after` or `nest` reference names no declared id, `mark`, or composition in the project, or a `nest` names a clip id instead of a composition.",
        example: "text value=\"B\" start=0 duration=1 start-after=\"ghost\"   // no clip, mark, or composition is called `ghost`",
        fix: "Declare `id=\"ghost\"` on the intended clip or composition, declare a `mark` named `ghost`, fix the spelling, or point `nest` at a composition `name`.",
    },
    CatalogEntry {
        code: "E_UNREADABLE",
        meaning: "A required file exists but cannot be read (permissions or I/O error).",
        example: "$ chmod 000 composition.kdl; framesmith check demo  ->  E_UNREADABLE",
        fix: "Make the file readable, or remove it and restore a readable copy.",
    },
    CatalogEntry {
        code: "E_UNSUPPORTED",
        meaning: "The element, property, key, easing, or look is valid notation but needs a later sprint, so it cannot render yet (audio effects, automation, groups, and ducking need Sprint 10; later motion properties and easings need Sprint 7; rich box looks need Sprint 8; hardware encode needs Sprint 25).",
        example: "audio path=\"s.wav\" start=0 duration=1 effect=\"reverb\"   // valid shape, Sprint 10 behavior",
        fix: "Remove the element or key for the current sprint, or wait for the named sprint; `check` exits 2 and `render` refuses before the first frame.",
    },
    CatalogEntry {
        code: "STYLE-NAME-UNKNOWN",
        meaning: "A style or layout property name is not part of the notation (for example a CSS `float` has no Framesmith layout equivalent); the refusal names the closest valid property.",
        example: "flex display=\"flex\" flex-direction=\"row\" float=\"left\" { }   // `float` is not a layout property",
        fix: "Remove the key, or use the suggested valid property (for example `flex-direction`, `gap`, `padding`, `align-items`, or `justify-content` on a `flex` container).",
    },
];

/// Look up one code. Returns `None` for unknown codes.
pub fn lookup(code: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.code == code)
}

/// Render one entry as human text.
pub fn render_entry(e: &CatalogEntry) -> String {
    format!(
        "{code}\n  meaning: {meaning}\n  example: {example}\n  fix: {fix}\n",
        code = e.code,
        meaning = e.meaning,
        example = e.example,
        fix = e.fix
    )
}

/// Escape a string for JSON output.
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Every code the validator can emit, plus the bootstrap codes.
    /// Keep in sync with `grep -rho '"E_[A-Z_]*"' src/`.
    const KNOWN_EMITTED: &[&str] = &[
        "E_AMBIGUOUS_ROOT",
        "E_BAD_AUDIO",
        "E_BAD_BLUR",
        "E_BAD_BOX",
        "E_BAD_CANVAS",
        "E_BAD_CAPTIONS",
        "E_BAD_CHART",
        "E_BAD_COLOR",
        "E_BAD_CONFIG",
        "E_BAD_DURATION",
        "E_BAD_EASING",
        "E_BAD_EFFECT",
        "E_BAD_FILL",
        "E_BAD_FIT",
        "E_BAD_FLAG",
        "E_BAD_FLEX",
        "E_BAD_FONT",
        "E_BAD_FOOTAGE",
        "E_BAD_FPS",
        "E_BAD_GRADE",
        "E_BAD_HIDDEN",
        "E_BAD_ID",
        "E_BAD_IMAGE",
        "E_BAD_LABEL",
        "E_BAD_LOTTIE",
        "E_BAD_MOTION",
        "E_BAD_ORDER",
        "E_BAD_PAINT",
        "E_BAD_PATH",
        "E_BAD_SCENE3D",
        "E_BAD_SPACING",
        "E_BAD_STYLE",
        "E_BAD_TEXT",
        "E_BAD_TIMING",
        "E_BAD_TRACK",
        "E_BAD_TRANSITION",
        "E_BAD_VARIABLE",
        "E_BAD_VERSION",
        "E_BAD_AUTOPLAY",
        "E_BAD_SLIDESHOW",
        "E_BAD_VFX",
        "E_EMPTY_DECK",
        "E_DOWNLOAD_FAILED",
        "E_DUP_ID",
        "E_MISSING_COMPOSITION",
        "E_MISSING_DIR",
        "E_MISSING_FIELD",
        "E_MISSING_FILE",
        "E_NO_FFMPEG",
        "E_PARSE",
        "E_UNKNOWN_ELEMENT",
        "E_UNKNOWN_KEY",
        "E_UNKNOWN_REF",
        "E_UNREADABLE",
        "E_UNSUPPORTED",
        "STYLE-NAME-UNKNOWN",
    ];

    #[test]
    fn catalog_covers_every_emitted_code() {
        let cataloged: HashSet<&str> = CATALOG.iter().map(|e| e.code).collect();
        for code in KNOWN_EMITTED {
            assert!(cataloged.contains(code), "catalog is missing {code}");
        }
        assert_eq!(
            cataloged.len(),
            KNOWN_EMITTED.len(),
            "catalog has extra codes beyond the emitted set"
        );
    }

    #[test]
    fn entries_are_complete_and_ordered() {
        let mut prev = "";
        let mut seen = HashSet::new();
        for e in CATALOG {
            assert!(!e.meaning.is_empty(), "{} needs a meaning", e.code);
            assert!(!e.example.is_empty(), "{} needs an example", e.code);
            assert!(!e.fix.is_empty(), "{} needs a fix direction", e.code);
            assert!(seen.insert(e.code), "duplicate catalog entry {}", e.code);
            assert!(e.code > prev, "catalog must stay sorted for stable output");
            prev = e.code;
        }
    }

    #[test]
    fn lookup_round_trips() {
        assert_eq!(lookup("E_PARSE").unwrap().code, "E_PARSE");
        assert!(lookup("E_BOGUS").is_none());
        assert!(lookup("").is_none());
    }

    #[test]
    fn json_escape_shapes() {
        assert_eq!(json_escape("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(json_escape("x\ny\tz"), "x\\ny\\tz");
        assert_eq!(json_escape("é"), "é");
        assert_eq!(json_escape("\u{1}"), "\\u0001");
    }
}
