use tray::disc::disc_face;

const N: u32 = 200;

fn alpha(face: &[u8], x: u32, y: u32) -> u8 {
    face[((y * N + x) * 4 + 3) as usize]
}

fn rgb(face: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * N + x) * 4) as usize;
    [face[i], face[i + 1], face[i + 2]]
}

#[test]
fn a_disc_has_a_hole_and_clear_corners() {
    let f = disc_face(N, "Test", [200, 40, 40], None);
    assert_eq!(f.len(), (N * N * 4) as usize);
    assert_eq!(alpha(&f, 100, 100), 0, "no hole");
    assert_eq!(alpha(&f, 0, 0), 0, "square corners");
    assert_eq!(alpha(&f, 100, 30), 255, "the label is not solid");
}

#[test]
fn the_hub_is_see_through_but_the_label_is_not() {
    let f = disc_face(N, "Test", [200, 40, 40], None);
    let hub = alpha(&f, 100, 100 - 20);
    assert!(hub > 60 && hub < 250, "hub alpha {hub}");
}

#[test]
fn the_label_is_printed_in_its_colour_above_the_hub() {
    let f = disc_face(N, "Test", [40, 160, 60], None);
    let [r, g, b] = rgb(&f, 100, 30);
    assert!(g > r + 60 && g > b + 60, "label colour {:?}", [r, g, b]);
}

#[test]
fn the_title_is_inked_below_the_hub() {
    let plain = disc_face(N, "", [40, 40, 140], None);
    let titled = disc_face(N, "Wipeout", [40, 40, 140], None);
    let differs = (130..170)
        .flat_map(|y| (50..150).map(move |x| (x, y)))
        .filter(|&(x, y)| rgb(&plain, x, y) != rgb(&titled, x, y))
        .count();
    assert!(differs > 100, "only {differs} pixels of title");
    let above = (20..60)
        .flat_map(|y| (60..140).map(move |x| (x, y)))
        .filter(|&(x, y)| rgb(&plain, x, y) != rgb(&titled, x, y))
        .count();
    assert_eq!(above, 0, "the title spilled above the hub");
}

#[test]
fn label_art_replaces_the_colour_and_title() {
    let art: Vec<u8> = (0..N * N).flat_map(|_| [250, 250, 10, 255]).collect();
    let f = disc_face(N, "Wipeout", [40, 40, 140], Some(&art));
    assert_eq!(rgb(&f, 100, 30), [250, 250, 10]);
    assert_eq!(
        rgb(&f, 100, 150),
        [250, 250, 10],
        "the title was printed over art"
    );
    assert_eq!(alpha(&f, 100, 100), 0, "art filled the hole");
}
