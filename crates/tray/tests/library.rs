use std::path::Path;

use tray::library::scan;

fn card(files: &[(&str, &str)]) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    for (name, body) in files {
        std::fs::write(d.path().join(name), body).unwrap();
    }
    d
}

fn titles(dir: &Path) -> Vec<(String, String)> {
    scan(dir)
        .into_iter()
        .map(|g| {
            (
                g.title,
                g.path.file_name().unwrap().to_string_lossy().into_owned(),
            )
        })
        .collect()
}

#[test]
fn a_bare_bin_is_a_game_titled_without_its_tags() {
    let d = card(&[("Bloody Roar II (USA).bin", "")]);
    assert_eq!(
        titles(d.path()),
        [("Bloody Roar II".into(), "Bloody Roar II (USA).bin".into())]
    );
}

#[test]
fn a_cue_and_the_tracks_it_names_are_one_game() {
    let cue = "FILE \"Crash (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\nFILE \"Crash (Track 2).bin\" BINARY\n  TRACK 02 AUDIO\n";
    let d = card(&[
        ("Crash.cue", cue),
        ("Crash (Track 1).bin", ""),
        ("Crash (Track 2).bin", ""),
    ]);
    assert_eq!(titles(d.path()), [("Crash".into(), "Crash.cue".into())]);
}

#[test]
fn an_unquoted_cue_file_line_still_claims_its_track() {
    let d = card(&[("Spyro.cue", "FILE Spyro.bin BINARY\n"), ("Spyro.bin", "")]);
    assert_eq!(titles(d.path()), [("Spyro".into(), "Spyro.cue".into())]);
}

#[test]
fn a_playlist_and_the_discs_it_names_are_one_game() {
    let m3u = "Final Fantasy VII (Disc 1).cue\nFinal Fantasy VII (Disc 2).cue\n";
    let d = card(&[
        ("Final Fantasy VII.m3u", m3u),
        (
            "Final Fantasy VII (Disc 1).cue",
            "FILE \"ff7d1.bin\" BINARY\n",
        ),
        (
            "Final Fantasy VII (Disc 2).cue",
            "FILE \"ff7d2.bin\" BINARY\n",
        ),
        ("ff7d1.bin", ""),
        ("ff7d2.bin", ""),
    ]);
    assert_eq!(
        titles(d.path()),
        [("Final Fantasy VII".into(), "Final Fantasy VII.m3u".into())]
    );
}

#[test]
fn games_sort_by_title_and_other_files_are_ignored() {
    let d = card(&[
        ("Tekken 3.chd", ""),
        ("Ape Escape.pbp", ""),
        ("notes.txt", ""),
        ("._Tekken 3.chd", ""),
    ]);
    let got: Vec<String> = titles(d.path()).into_iter().map(|t| t.0).collect();
    assert_eq!(got, ["Ape Escape", "Tekken 3"]);
}

#[test]
fn a_missing_folder_is_an_empty_shelf() {
    assert!(scan(Path::new("/no/such/folder")).is_empty());
}
