//! The preview page: the HTML around the composition, written once here. Its script is
//! `web/preview.js`. Specified in `docs/spec/preview.md`, "The preview page".

use model::Manifest;
use protocol::VERSION;
use timebase::frame_count;

/// The path the preview page's script is served from.
pub const SCRIPT_PATH: &str = "/__framesmith__/preview.js";
/// The script itself.
pub const SCRIPT: &str = include_str!("../web/preview.js");

/// The address of the composition's own page, shown in the frame.
const COMPOSITION_PATH: &str = "/index.html";

/// Text for an HTML text node or a double-quoted attribute.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}

const STYLE: &str = "\
html,body{height:100%;margin:0}\
body{display:flex;flex-direction:column;background:#1c1c1e;color:#f2f2f7;font:16px/1.5 system-ui,-apple-system,\"Segoe UI\",sans-serif}\
#stage{position:relative;flex:1 1 0;min-height:0;overflow:hidden}\
#picture{position:absolute;overflow:hidden;background:#000}\
#composition{position:absolute;left:0;top:0;border:0;transform-origin:0 0;visibility:hidden;background:#000}\
#notice{box-sizing:border-box;max-width:60ch;margin:0 auto;padding:24px 16px}\
#notice[hidden]{display:none}\
#notice p{margin:0 0 8px;font-weight:600}\
#notice ul{margin:0;padding-left:20px}\
body[data-state=refused] #stage{display:none}";

/// The preview page of a composition.
pub fn shell(manifest: &Manifest) -> String {
    let name = escape(&manifest.name);
    let frames = frame_count(manifest.length, manifest.fps);
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{name} - Framesmith preview</title>\n<link rel=\"icon\" href=\"data:,\">\n<style>{STYLE}</style>\n</head>\n\
         <body data-state=\"loading\" data-name=\"{name}\" data-width=\"{width}\" data-height=\"{height}\" data-fps=\"{fps}\" data-frames=\"{frames}\" data-protocol=\"{VERSION}\">\n\
         <main id=\"stage\" aria-label=\"Preview of {name}\">\n\
         <div id=\"picture\"><iframe id=\"composition\" src=\"{COMPOSITION_PATH}\" title=\"Composition {name}\" width=\"{width}\" height=\"{height}\" \
         style=\"width:{width}px;height:{height}px\"></iframe></div>\n</main>\n\
         <section id=\"notice\" role=\"alert\" hidden></section>\n\
         <script src=\"{SCRIPT_PATH}\"></script>\n</body>\n</html>\n",
        width = manifest.width,
        height = manifest.height,
        fps = manifest.fps,
    )
}

/// The page shown in place of the preview page when the project no longer passes the check.
pub fn blocked() -> String {
    "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Framesmith preview</title>\n\
     <link rel=\"icon\" href=\"data:,\">\n<style>body{margin:0;background:#1c1c1e;color:#f2f2f7;font:16px/1.5 system-ui,sans-serif}\
     p{max-width:60ch;margin:0 auto;padding:24px 16px}</style>\n</head>\n<body data-state=\"blocked\">\n\
     <p role=\"alert\">The preview cannot show this project: it no longer passes `framesmith check`, so no picture is shown. \
     Run `framesmith check` on the project to see why.</p>\n</body>\n</html>\n"
        .to_string()
}
