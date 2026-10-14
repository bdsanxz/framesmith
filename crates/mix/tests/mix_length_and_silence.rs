//! The mix is as long as the picture to the sample, and silent (`docs/spec/render-command.md`
//! section 5.2).

use mix::{length_for_picture, Mix, CHANNELS, SAMPLE_RATE};

#[test]
fn the_mix_is_48_khz_in_two_channels() {
    assert_eq!(SAMPLE_RATE, 48_000);
    assert_eq!(CHANNELS, 2);
}

#[test]
fn the_length_is_the_seconds_of_the_picture_times_48000_at_24_30_and_60() {
    assert_eq!(length_for_picture(120, 24), 240_000);
    assert_eq!(length_for_picture(150, 30), 240_000);
    assert_eq!(length_for_picture(300, 60), 240_000);
    assert_eq!(length_for_picture(31, 30), 49_600);
    assert_eq!(length_for_picture(1, 60), 800);
}

#[test]
fn a_rate_that_does_not_divide_48000_rounds_to_the_nearest_sample() {
    // 1 / 7 s is 6857.14 sample frames; 2 / 7 s is 13714.29; 1 / 64 s is 750 exactly.
    assert_eq!(length_for_picture(1, 7), 6_857);
    assert_eq!(length_for_picture(2, 7), 13_714);
    assert_eq!(length_for_picture(1, 64), 750);
    // 1 / 96000 of a second would be half a sample; a half rounds up.
    assert_eq!(length_for_picture(1, 96_000), 1);
}

#[test]
fn every_sample_read_is_silence_and_reading_ends_at_the_length() {
    let mut mix = Mix::for_picture(30, 30);
    assert_eq!(mix.length(), 48_000);
    let mut piece = vec![1.0_f32; 2 * 1777];
    let mut total = 0_u64;
    loop {
        piece.fill(1.0);
        let read = mix.read(&mut piece);
        if read == 0 {
            break;
        }
        assert!(piece[..read * 2].iter().all(|sample| *sample == 0.0));
        total += read as u64;
    }
    assert_eq!(total, 48_000);
    assert_eq!(mix.left(), 0);
}

#[test]
fn a_piece_of_an_odd_number_of_values_takes_only_whole_frames() {
    let mut mix = Mix::for_picture(1, 30);
    let mut piece = vec![1.0_f32; 5];
    assert_eq!(mix.read(&mut piece), 2);
    assert_eq!(piece[4], 1.0);
}
