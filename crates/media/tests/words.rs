//! The reason taken from what `ffmpeg` printed before it stopped.

use media::last_words;

#[test]
fn the_reason_is_the_last_line_that_is_not_blank() {
    assert_eq!(last_words("first\nsecond line\n\n  \n"), "second line");
    assert_eq!(last_words("  only this  "), "only this");
}

#[test]
fn nothing_printed_is_said_in_words() {
    assert_eq!(last_words(""), "it gave no reason");
    assert_eq!(last_words("\n \n"), "it gave no reason");
}

#[test]
fn a_missing_encoder_is_named_before_the_last_line() {
    let printed = "[vost#0:0 @ 0x1] Unknown encoder 'libx264'\n[vost#0:0 @ 0x1] Error selecting an encoder\n\
Error opening output file out.mp4.\nError opening output files: Encoder not found\n";
    assert_eq!(
        last_words(printed),
        "Unknown encoder 'libx264'; Error opening output files: Encoder not found"
    );
    // Alone on the last line it is not repeated.
    assert_eq!(last_words("x\nUnknown encoder 'libx264'\n"), "Unknown encoder 'libx264'");
}

#[test]
fn a_very_long_line_is_cut_to_300_characters() {
    assert_eq!(last_words(&"é".repeat(1000)).chars().count(), 300);
}
