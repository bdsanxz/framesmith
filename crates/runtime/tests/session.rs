use protocol::Resolution;
use runtime::Session;

/// A page with three clips (one nested) and motion on two elements.
const SNAPSHOT: &str = r##"{
  "elements": [
    { "tag": "div", "id": "stage", "parent": null, "attrs": [
        { "name": "data-fs-composition", "value": "main" },
        { "name": "data-fs-size", "value": "640x360" },
        { "name": "data-fs-duration", "value": "4" } ] },
    { "tag": "div", "id": "outer", "parent": 0, "attrs": [
        { "name": "data-fs-clip", "value": "" }, { "name": "data-fs-start", "value": "1" }, { "name": "data-fs-duration", "value": "2" } ] },
    { "tag": "p", "id": "inner", "parent": 1, "attrs": [
        { "name": "data-fs-clip", "value": "" }, { "name": "data-fs-start", "value": "0" }, { "name": "data-fs-duration", "value": "1.5" } ] },
    { "tag": "p", "id": null, "parent": 0, "attrs": [
        { "name": "data-fs-clip", "value": "" }, { "name": "data-fs-start", "value": "3.5" }, { "name": "data-fs-duration", "value": "9" } ] }
  ],
  "motion": [ { "text": "[ { \"target\": \"#a\", \"to\": { \"x\": 100, \"opacity\": 0 }, \"at\": 1, \"duration\": 1 }, { \"target\": \".b\", \"set\": { \"rotation\": 45 }, \"at\": 2 } ]" } ]
}"##;

fn started(fps: u32) -> (Session, Resolution, Vec<usize>) {
    let mut session = Session::new();
    let selectors: Vec<String> = serde_json::from_str(&session.read(SNAPSHOT)).unwrap();
    assert_eq!(selectors, vec!["#a", ".b"]);
    // "#a" is element 7; ".b" is elements 3 and 7.
    let resolution: Resolution = serde_json::from_str(&session.resolve("[[7],[3,7]]")).unwrap();
    let order: Vec<usize> = serde_json::from_str(&session.begin(fps)).unwrap();
    (session, resolution, order)
}

#[test]
fn the_page_resolves_to_a_manifest() {
    let (_, resolution, order) = started(30);
    assert_eq!(resolution.problems, vec![]);
    let manifest = resolution.manifest.unwrap();
    assert_eq!(
        (manifest.name.as_str(), manifest.width, manifest.height, manifest.fps),
        ("main", 640, 360, 30)
    );
    let labels: Vec<&str> = manifest.clips.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, vec!["#outer", "#inner", "number 3 <p>"]);
    assert_eq!(manifest.tweens.iter().map(|t| t.matches).collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(resolution.clips, vec![1, 2, 3]);
    // Animated elements come in order of first use.
    assert_eq!(order, vec![7, 3]);
}

#[test]
fn t011_clips_show_inside_their_windows_only() {
    let (mut session, _, _) = started(30);
    let mut shown = |frame: u32| session.frame(frame)[..3].to_vec();
    assert_eq!(shown(0), vec![0.0, 0.0, 0.0]);
    assert_eq!(shown(29), vec![0.0, 0.0, 0.0]);
    // The start is inclusive.
    assert_eq!(shown(30), vec![1.0, 1.0, 0.0]);
    // The nested clip's own window is 0 to 1.5 s, but it only shows while its enclosing clip shows.
    assert_eq!(shown(44), vec![1.0, 1.0, 0.0]);
    assert_eq!(shown(45), vec![1.0, 0.0, 0.0]);
    // The end is exclusive.
    assert_eq!(shown(89), vec![1.0, 0.0, 0.0]);
    assert_eq!(shown(90), vec![0.0, 0.0, 0.0]);
    assert_eq!(shown(105), vec![0.0, 0.0, 1.0]);
    assert_eq!(shown(119), vec![0.0, 0.0, 1.0]);
}

#[test]
fn t016_animated_values_come_with_flags_for_what_is_animated() {
    let (mut session, _, _) = started(30);
    let element = |session: &mut Session, frame: u32, index: usize| session.frame(frame)[3 + index * 6..3 + (index + 1) * 6].to_vec();
    // Element 7: x and opacity from the first tween, rotation from the second.
    assert_eq!(element(&mut session, 0, 0), vec![25.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    assert_eq!(element(&mut session, 45, 0), vec![25.0, 50.0, 0.0, 0.0, 0.0, 0.5]);
    assert_eq!(element(&mut session, 60, 0), vec![25.0, 100.0, 0.0, 0.0, 45.0, 0.0]);
    // Element 3: rotation only.
    assert_eq!(element(&mut session, 59, 1), vec![8.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(element(&mut session, 60, 1), vec![8.0, 0.0, 0.0, 0.0, 45.0, 0.0]);
    assert_eq!(session.frame(0).len(), 3 + 2 * 6);
}

#[test]
fn t017_c5_a_frame_does_not_depend_on_the_frames_before_it() {
    let (mut forward, _, _) = started(30);
    let in_order: Vec<Vec<f64>> = (0..120).map(|f| forward.frame(f).to_vec()).collect();
    let (mut backward, _, _) = started(30);
    for frame in (0..120).rev() {
        assert_eq!(backward.frame(frame), in_order[frame as usize].as_slice(), "frame {frame}");
    }
    assert_eq!(backward.frame(45), in_order[45].as_slice());
}

#[test]
fn t008_c3_the_frame_rate_of_the_run_moves_the_windows() {
    let (mut session, _, _) = started(60);
    assert_eq!(session.frame(59)[0], 0.0);
    assert_eq!(session.frame(60)[0], 1.0);
    assert_eq!(session.frame(179)[0], 1.0);
    assert_eq!(session.frame(180)[0], 0.0);
}

#[test]
fn a_page_that_does_not_resolve_says_why() {
    let mut session = Session::new();
    session.read(r#"{ "elements": [ { "tag": "div", "id": null, "parent": null, "attrs": [ { "name": "data-fs-composition", "value": "main" } ] } ], "motion": [] }"#);
    let resolution: Resolution = serde_json::from_str(&session.resolve("[]")).unwrap();
    assert!(resolution.manifest.is_none());
    assert_eq!(
        resolution.problems.iter().map(|p| p.code.as_str()).collect::<Vec<_>>(),
        vec!["FS-E005", "FS-E011"]
    );
    assert_eq!(session.begin(30), "[]");
    assert!(session.frame(0).is_empty());

    let mut broken = Session::new();
    assert_eq!(broken.read("not json"), "[]");
    let resolution: Resolution = serde_json::from_str(&broken.resolve("[]")).unwrap();
    assert_eq!(resolution.problems[0].code, "FS-R016");
}

#[test]
fn a_selector_the_page_rejects_is_a_problem() {
    let mut session = Session::new();
    session.read(SNAPSHOT);
    let resolution: Resolution = serde_json::from_str(&session.resolve("[null,[3]]")).unwrap();
    assert_eq!(
        resolution.problems.iter().map(|p| p.code.as_str()).collect::<Vec<_>>(),
        vec!["FS-E048"]
    );
    assert!(resolution.manifest.is_none());
}
