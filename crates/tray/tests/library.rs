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

#[test]
fn discs_numbered_in_their_names_are_one_game() {
    let d = card(&[
        ("Final Fantasy IX (USA) (Disc 2).chd", ""),
        ("Final Fantasy IX (USA) (Disc 1).chd", ""),
        ("Final Fantasy IX (USA) (Disc 3).chd", ""),
        ("Tekken 3 (USA).chd", ""),
    ]);
    let games = scan(d.path());
    assert_eq!(games.len(), 2);
    let ff = &games[0];
    assert_eq!(ff.title, "Final Fantasy IX");
    assert_eq!(ff.stem, "Final Fantasy IX (USA)");
    assert!(ff.grouped);
    let names: Vec<_> = ff
        .discs
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        [
            "Final Fantasy IX (USA) (Disc 1).chd",
            "Final Fantasy IX (USA) (Disc 2).chd",
            "Final Fantasy IX (USA) (Disc 3).chd"
        ]
    );
    assert_eq!(games[1].discs.len(), 1);
}

#[test]
fn numbered_cues_group_and_keep_their_tracks_hidden() {
    let d = card(&[
        ("MGS (Disc 1).cue", "FILE \"MGS (Disc 1).bin\" BINARY\n"),
        ("MGS (Disc 1).bin", ""),
        ("MGS (Disc 2).cue", "FILE \"MGS (Disc 2).bin\" BINARY\n"),
        ("MGS (Disc 2).bin", ""),
    ]);
    let games = scan(d.path());
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].discs.len(), 2);
    assert!(games[0]
        .discs
        .iter()
        .all(|p| p.extension().unwrap() == "cue"));
}

#[test]
fn a_playlist_lists_its_discs_as_the_core_reads_them() {
    let d = card(&[
        (
            "Game.m3u",
            "# a comment\nGame (Disc 1).cue\nGame (Disc 2).cue\n",
        ),
        ("Game (Disc 1).cue", ""),
        ("Game (Disc 2).cue", ""),
    ]);
    let games = scan(d.path());
    assert_eq!(games.len(), 1, "the playlist's discs showed as games too");
    assert!(!games[0].grouped);
    assert_eq!(
        games[0].discs,
        [
            d.path().join("Game (Disc 1).cue"),
            d.path().join("Game (Disc 2).cue")
        ]
    );
}

#[test]
fn a_lone_numbered_disc_is_just_a_game() {
    let d = card(&[("Demo (Disc 1).bin", "")]);
    let games = scan(d.path());
    assert!(!games[0].grouped);
    assert_eq!(games[0].stem, "Demo (Disc 1)");
}

#[test]
fn disc_numbers_are_read_with_or_without_a_total() {
    use tray::library::disc_number;
    assert_eq!(
        disc_number("FF7 (USA) (Disc 2)"),
        Some((2, "FF7 (USA)".into()))
    );
    assert_eq!(
        disc_number("FF7 (Disc 3 of 3) (USA)"),
        Some((3, "FF7 (USA)".into()))
    );
    assert_eq!(disc_number("Discworld (USA)"), None);
}

#[test]
fn grouped_discs_get_a_playlist_of_full_paths() {
    let d = card(&[("X (Disc 1).chd", ""), ("X (Disc 2).chd", "")]);
    let games = scan(d.path());
    let root = tempfile::tempdir().unwrap();
    let list = tray::library::content(root.path(), &games[0]).unwrap();
    assert_eq!(list.file_name().unwrap(), "X.m3u");
    let text = std::fs::read_to_string(list).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        [
            d.path().join("X (Disc 1).chd").to_str().unwrap(),
            d.path().join("X (Disc 2).chd").to_str().unwrap()
        ]
    );
}
