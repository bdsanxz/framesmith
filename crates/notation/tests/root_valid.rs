//! A root that is valid: what is read of it, the defaults, and the frame count.

use notation::{read_root, Colour, Root};
use timebase::Time;

fn root(attributes: &str) -> Root {
    let text = format!("<composition {attributes}/>");
    read_root(text.as_bytes()).unwrap_or_else(|problems| panic!("{attributes}: {problems:?}"))
}

#[test]
fn every_attribute_of_a_valid_root_is_read() {
    let root = root(r##"version="1" id="demo" width="1280" height="720" fps="24" duration="6.5" background="#101820""##);
    assert_eq!(root.id.as_deref(), Some("demo"));
    assert_eq!((root.width, root.height, root.fps), (1280, 720, 24));
    assert_eq!(root.duration, Time::new(13, 2));
    assert_eq!(root.frames, 156);
    assert_eq!(
        root.background,
        Colour {
            red: 0x10,
            green: 0x18,
            blue: 0x20,
            alpha: 255
        }
    );
}

#[test]
fn five_seconds_with_no_fps_is_150_frames_at_30_a_second() {
    let root = root(r#"version="1" width="1280" height="720" duration="5""#);
    assert_eq!(root.fps, 30);
    assert_eq!(root.frames, 150);
}

#[test]
fn one_point_zero_one_seconds_at_30_is_31_frames() {
    let root = root(r#"version="1" width="1280" height="720" fps="30" duration="1.01""#);
    assert_eq!(root.frames, 31);
}

#[test]
fn the_frame_count_follows_the_frame_grid_rule() {
    let frames = |fps: &str, duration: &str| root(&format!(r#"version="1" width="2" height="2" fps="{fps}" duration="{duration}""#)).frames;
    assert_eq!(frames("24", "6"), 144);
    assert_eq!(frames("30", "6"), 180);
    assert_eq!(frames("60", "6"), 360);
    assert_eq!(frames("30", "6.01"), 181);
    // 180.0005 frames is within a thousandth of a frame of 180.
    assert_eq!(frames("30", "6.000016667"), 180);
    // A length shorter than a frame is still one frame.
    assert_eq!(frames("30", "0.0001"), 1);
    assert_eq!(frames("1", "1"), 1);
    assert_eq!(frames("240", "10"), 2400);
}

#[test]
fn a_root_with_no_background_and_no_fps_is_valid_black_at_30() {
    let root = root(r#"version="1" width="640" height="360" duration="2""#);
    assert_eq!(root.fps, 30);
    assert_eq!(root.background, Colour::BLACK);
    assert_eq!(root.id, None);
    assert_eq!(root.frames, 60);
}

#[test]
fn a_background_may_carry_an_alpha_and_be_written_in_either_case() {
    let root = root(r##"version="1" width="640" height="360" duration="2" background="#AbCdEf80""##);
    assert_eq!(
        root.background,
        Colour {
            red: 0xab,
            green: 0xcd,
            blue: 0xef,
            alpha: 0x80
        }
    );
}

#[test]
fn the_size_may_be_the_smallest_and_the_largest() {
    let root = root(r#"version="1" width="2" height="8192" duration="1""#);
    assert_eq!((root.width, root.height), (2, 8192));
}

#[test]
fn a_whole_number_may_be_written_with_a_point_a_sign_or_an_exponent() {
    let root = root(r#"version="1.0" width="1.28e3" height="+720." fps="3e1" duration=".5""#);
    assert_eq!((root.width, root.height, root.fps), (1280, 720, 30));
    assert_eq!(root.frames, 15);
    // The version is the whole number 1 however it is written.
    for version in ["01", "1e0", "+1", "1.00"] {
        self::root(&format!(r#"version="{version}" width="2" height="2" duration="1""#));
    }
}

#[test]
fn white_space_around_a_value_is_not_part_of_it() {
    let root = root("version=\" 1 \" width=\"\t640\n\" height=\" 360 \" fps=\" 25 \" duration=\" 2 \" background=\" #000000 \"");
    assert_eq!((root.width, root.height, root.fps, root.frames), (640, 360, 25, 50));
}

#[test]
fn the_children_of_the_root_and_other_root_attributes_are_not_judged() {
    let text = r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- a comment -->
<composition version="1" width="640" height="360" duration="2" class="big" color="nonsense">
  <style/>
  <circle radius="oops"/>
  <box width="odd"><tween/></box>
</composition>
"#;
    let root = read_root(text.as_bytes()).unwrap();
    assert_eq!(root.frames, 60);
}

#[test]
fn a_byte_order_mark_and_a_declaration_before_the_root_are_fine() {
    let mut text = vec![0xef, 0xbb, 0xbf];
    text.extend_from_slice(b"<?xml version=\"1.0\"?>\n<composition version=\"1\" width=\"4\" height=\"4\" duration=\"1\"/>");
    assert_eq!(read_root(&text).unwrap().frames, 30);
}
