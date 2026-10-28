//! The registry a film is written with holds the MP4 format alone: one format, which
//! claims `.mp4` and `.m4v` and does not claim `.mov` (`docs/spec/render-command.md`, section 6).

#[test]
fn the_registry_a_film_is_written_with_holds_the_mp4_format_alone() {
    let registry = render::formats();
    let formats = registry.formats();
    assert_eq!(formats.len(), 1, "the registry holds one format");
    assert_eq!(formats[0].name, "mp4", "the one format");
    assert!(
        formats[0].extensions.contains(&"mp4") && formats[0].extensions.contains(&"m4v"),
        "the MP4 format claims .mp4 and .m4v: {:?}",
        formats[0].extensions
    );
    assert!(
        !formats[0].extensions.contains(&"mov"),
        "no writer of this build claims .mov: {:?}",
        formats[0].extensions
    );
}
