//! The sentence a refused film size reports (`docs/spec/render-command.md` section 5.4): the encoder, the size, the
//! settings in effect, and the setting that makes the film render. `refused_sentence` builds it without asking any
//! encoder, so every remedy is pinned here, including the software one, which no machine the tests run on refuses.

use project::{Codec, Encoder, Quality};
use render::{refused_sentence, Encoding};

fn encoding(encoder: Encoder, codec: Codec) -> Encoding {
    Encoding {
        codec,
        encoder,
        quality: Quality::Standard,
    }
}

#[test]
fn a_hardware_h264_film_names_codec_hevc_when_the_machine_s_own_hevc_encoder_takes_the_size() {
    assert_eq!(
        refused_sentence(encoding(Encoder::Hardware, Codec::H264), 7680, 4320, true),
        "the machine's own H.264 encoder does not take a film of 7680x4320 (render.encoder: hardware, render.codec: h264); \
         set codec: hevc, which the machine's own encoder takes at this size, or encoder: software"
    );
}

#[test]
fn a_hardware_h264_film_names_encoder_software_when_hevc_does_not_take_the_size_either() {
    assert_eq!(
        refused_sentence(encoding(Encoder::Hardware, Codec::H264), 7680, 4320, false),
        "the machine's own H.264 encoder does not take a film of 7680x4320 (render.encoder: hardware, render.codec: h264); \
         set encoder: software"
    );
}

#[test]
fn a_hardware_hevc_film_names_encoder_software() {
    for hevc_taken in [true, false] {
        assert_eq!(
            refused_sentence(encoding(Encoder::Hardware, Codec::Hevc), 7680, 4320, hevc_taken),
            "the machine's own HEVC encoder does not take a film of 7680x4320 (render.encoder: hardware, render.codec: hevc); \
             set encoder: software"
        );
    }
}

#[test]
fn a_software_film_says_to_make_the_film_smaller() {
    for codec in [Codec::H264, Codec::Hevc] {
        let name = if codec == Codec::H264 { "H.264" } else { "HEVC" };
        assert_eq!(
            refused_sentence(encoding(Encoder::Software, codec), 8192, 8192, false),
            format!(
                "the system's {name} encoder on the processor does not take a film of 8192x8192 (render.encoder: software, render.codec: {}); make the film smaller",
                if codec == Codec::H264 { "h264" } else { "hevc" }
            )
        );
    }
}
