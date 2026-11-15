//! Music is copied into the project, never over earlier music, and only when it can be played.

#![cfg(target_os = "macos")]

use shotmaxxing_core::bundle::ProjectBundle;
use shotmaxxing_core::project::Project;

#[test]
fn music_is_copied_into_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let song = dir.path().join("Theme Song.wav");
    std::fs::write(&song, shotmaxxing_render::click::wav(&vec![0.1f32; 48_000], 48_000)).unwrap();
    let mut project = Project { name: "Take".into(), ..Project::default() };
    let root = ProjectBundle::create(dir.path(), &mut project).unwrap().root().to_path_buf();

    let first = shotmaxxing_render::import_music(&root, &song).unwrap();
    assert_eq!((first.path.as_str(), first.name.as_str()), ("media/music.wav", "Theme Song.wav"));
    assert!((first.seconds - 1.0).abs() < 0.01, "{first:?}");
    assert!(root.join("media/music.wav").exists());
    // A second song never replaces the first, which undo may still need.
    assert_eq!(shotmaxxing_render::import_music(&root, &song).unwrap().path, "media/music 2.wav");

    // Not music, not sound, or not a project: nothing is copied.
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, "not music").unwrap();
    assert!(shotmaxxing_render::import_music(&root, &notes).is_err());
    let fake = dir.path().join("fake.mp3");
    std::fs::write(&fake, "not really an mp3").unwrap();
    assert!(shotmaxxing_render::import_music(&root, &fake).is_err());
    assert!(shotmaxxing_render::import_music(dir.path(), &song).is_err());
    let copies = std::fs::read_dir(root.join("media")).unwrap().count();
    assert_eq!(copies, 2);
}
