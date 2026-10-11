use slot::bezel::{pick, BEZELS};

fn card(files: &[&str]) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join(BEZELS)).unwrap();
    for f in files {
        std::fs::write(d.path().join(BEZELS).join(f), b"").unwrap();
    }
    d
}

fn picked(d: &tempfile::TempDir, panel: (u32, u32)) -> Option<String> {
    pick(d.path(), panel).map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
}

#[test]
fn the_panel_takes_the_bezel_named_for_its_size() {
    let d = card(&["640x480.png", "720x720.png", "1024x768.png"]);
    assert_eq!(picked(&d, (640, 480)).as_deref(), Some("640x480.png"));
    assert_eq!(picked(&d, (720, 720)).as_deref(), Some("720x720.png"));
}

#[test]
fn a_panel_with_no_bezel_of_its_size_takes_the_nearest_of_its_shape() {
    let d = card(&["640x480.png", "1024x768.png", "720x720.png"]);
    assert_eq!(picked(&d, (1280, 960)).as_deref(), Some("1024x768.png"));
    assert_eq!(picked(&d, (720, 480)), None);
}

#[test]
fn files_that_are_not_sized_pngs_are_ignored() {
    let d = card(&["notes.txt", "sp.png", "640x480.jpg", "._640x480.png"]);
    assert_eq!(picked(&d, (640, 480)), None);
    assert_eq!(picked(&tempfile::tempdir().unwrap(), (640, 480)), None);
}

#[test]
fn a_game_boy_bezel_comes_from_its_own_folder_and_never_the_shared_one() {
    use slot::bezel::{pick_gb, GB_BEZELS};
    let d = card(&["640x480.png"]);
    assert_eq!(pick_gb(d.path(), (640, 480)), None, "no Game Boy bezel");
    std::fs::create_dir_all(d.path().join(GB_BEZELS)).unwrap();
    std::fs::write(d.path().join(GB_BEZELS).join("640x480.png"), b"").unwrap();
    let gb = pick_gb(d.path(), (640, 480)).expect("the Game Boy bezel");
    assert!(gb.ends_with("System/bezels/gb/640x480.png"), "{gb:?}");
    assert_eq!(picked(&d, (640, 480)).as_deref(), Some("640x480.png"));
}

#[test]
fn the_shipped_game_boy_bezel_leaves_exactly_the_3x_picture_open() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../card/System/bezels/gb/640x480.png");
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (640, 480));
    let alpha = |x: u32, y: u32| buf[((y * 640 + x) * 4 + 3) as usize];
    for y in [10, 200, 400] {
        assert_eq!(alpha(79, y), 255, "the left panel is open at row {y}");
        assert_eq!(alpha(560, y), 255, "the right panel is open at row {y}");
        assert_eq!(alpha(320, y), 0, "the picture is covered at row {y}");
        assert!(
            alpha(80, y) < 255 && alpha(559, y) < 255,
            "the picture's edge is covered"
        );
    }
}

#[test]
fn the_power_light_is_green_full_amber_middling_and_red_low() {
    use slot::bezel::light_colour;
    let green = light_colour(Some(100));
    assert_eq!(light_colour(Some(70)), green);
    let amber = light_colour(Some(69));
    assert_ne!(amber, green);
    assert_eq!(light_colour(Some(30)), amber);
    let red = light_colour(Some(29));
    assert_ne!(red, amber);
    assert_eq!(light_colour(Some(0)), red);
    assert_eq!(light_colour(None), green, "an unknown battery");
}

#[test]
fn the_power_light_sits_where_the_file_beside_the_bezel_says() {
    use slot::bezel::light_of;
    let d = tempfile::tempdir().unwrap();
    let bezel = d.path().join("640x480.png");
    std::fs::write(&bezel, b"").unwrap();
    assert_eq!(light_of(&bezel, (640, 480)), None, "a light with no file");
    std::fs::write(d.path().join("640x480.light"), "40 132 4.0\n").unwrap();
    assert_eq!(light_of(&bezel, (640, 480)), Some((40.0, 132.0, 4.0)));
    assert_eq!(
        light_of(&bezel, (1280, 960)),
        Some((80.0, 264.0, 8.0)),
        "not scaled with the bezel"
    );
}

#[test]
fn the_shipped_game_boy_bezel_has_its_light() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bezel = repo.join("card/System/bezels/gb/640x480.png");
    assert!(slot::bezel::light_of(&bezel, (640, 480)).is_some());
}
