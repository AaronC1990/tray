//! slot sharing its shelf ring with another frontend's shelf, as tray's
//! discs do. The switch is process-wide, so these live in their own binary.
mod common;

use common::{app_booting_at, tmp_root_with_carts, write_gb_cart, CLOCK_IS_SET};
use slot::app::{set_outside_shelf, App, Phase};
use slot_input::{Action, Btn};
use slot_store::{write_slot_state, SlotState};

fn clocked(root: &std::path::Path) {
    let state = SlotState {
        clock_set: true,
        ..SlotState::default()
    };
    write_slot_state(root, &state).unwrap();
}

fn gba_and_gb() -> (tempfile::TempDir, App) {
    set_outside_shelf(true);
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    write_gb_cart(&d, "Tetris", "TETRIS");
    clocked(d.path());
    let (a, _clock) = app_booting_at(d.path(), CLOCK_IS_SET);
    (d, a)
}

fn press(a: &mut App, b: Btn) {
    a.apply(Action::GbaDown(b));
    a.apply(Action::GbaUp(b));
}

#[test]
fn right_past_the_last_shelf_leaves_the_ring_instead_of_wrapping() {
    let (_d, mut a) = gba_and_gb();
    assert_eq!(a.shelf_platform_name(), Some("Game Boy Advance"));
    press(&mut a, Btn::R1);
    assert_eq!(a.shelf_platform_name(), Some("Game Boy"));
    assert_eq!(a.take_leave(), None);
    press(&mut a, Btn::R1);
    assert_eq!(a.take_leave(), Some(1));
    assert_eq!(a.take_leave(), None, "a leave is taken once");
    assert_eq!(
        a.shelf_platform_name(),
        Some("Game Boy"),
        "the shelf wrapped"
    );
}

#[test]
fn left_past_the_first_shelf_leaves_the_other_way() {
    let (_d, mut a) = gba_and_gb();
    press(&mut a, Btn::L1);
    assert_eq!(a.take_leave(), Some(-1));
    assert_eq!(a.shelf_platform_name(), Some("Game Boy Advance"));
}

#[test]
fn coming_back_lands_on_the_near_end_of_the_ring() {
    let (_d, mut a) = gba_and_gb();
    a.enter_shelves(-1);
    assert_eq!(a.shelf_platform_name(), Some("Game Boy"));
    a.enter_shelves(1);
    assert_eq!(a.shelf_platform_name(), Some("Game Boy Advance"));
}

#[test]
fn one_cart_is_a_shelf_not_the_whole_library() {
    set_outside_shelf(true);
    let d = tmp_root_with_carts(&["Emerald"]);
    clocked(d.path());
    let (mut a, _clock) = app_booting_at(d.path(), CLOCK_IS_SET);
    assert!(!a.single_cart());
    assert!(
        matches!(a.phase(), Phase::Shelf),
        "the only cart was seated"
    );
    assert_eq!(a.shelf_platform_name(), Some("Game Boy Advance"));
    press(&mut a, Btn::R1);
    assert_eq!(a.take_leave(), Some(1));
}

#[test]
fn the_outside_shelf_is_drawn_in_slots_frame_and_named_in_the_slot() {
    use slot::app::Outside;
    use slot_gfx::Draw;
    let (_d, mut a) = gba_and_gb();
    let marker = Draw::Rect {
        x: 1.0,
        y: 2.0,
        w: 3.0,
        h: 4.0,
        colour: [0.1, 0.2, 0.3, 1.0],
    };
    press(&mut a, Btn::L1);
    assert_eq!(a.take_leave(), Some(-1));
    a.set_outside(Some(Outside {
        row: vec![marker],
        ..Outside::default()
    }));
    a.say_in_slot("PlayStation");
    assert_eq!(a.slot_text().as_deref(), Some("PlayStation"));
    let mut out = Vec::new();
    a.draw(&mut out);
    assert!(out.contains(&marker), "the outside shelf was not drawn");

    a.enter_shelves(1);
    assert_eq!(a.slot_text().as_deref(), Some("Game Boy Advance"));
    let mut out = Vec::new();
    a.draw(&mut out);
    assert!(!out.contains(&marker), "the outside shelf stayed up");
}

#[test]
fn an_outside_game_saves_loads_and_comes_out_when_asked() {
    use common::StubSnapshot;
    use slot_store::StateRing;
    let (d, mut a) = gba_and_gb();
    let dir = d.path().join("States/PS/pcsx_rearmed/Crash");
    let (snapshot, loaded) = StubSnapshot::pair();
    a.seat_outside("Crash".into(), StateRing::at(dir.clone()), snapshot);
    assert!(a.outside_seated());
    assert!(matches!(a.phase(), Phase::Playing { .. }));

    a.apply(Action::SaveState);
    let ring = StateRing::at(dir.clone());
    let saved = ring.list().unwrap();
    assert_eq!(saved.len(), 1, "the save went nowhere");

    a.apply(Action::LoadState);
    assert_eq!(
        loaded.lock().unwrap().as_deref(),
        Some(&[9u8; 1024][..]),
        "the newest state was not loaded back"
    );

    a.apply(Action::Polaroids);
    assert!(
        matches!(a.phase(), Phase::Polaroids { .. }),
        "no state browser"
    );
    a.apply(Action::GbaDown(Btn::B));
    assert!(matches!(a.phase(), Phase::Playing { .. }));

    // Holding MENU is left to the outside game's own combinations; it
    // comes out when its frontend asks.
    a.apply(Action::Eject);
    assert!(
        !a.take_outside_eject(),
        "holding MENU ejected an outside game"
    );
    a.eject_outside();
    assert!(a.take_outside_eject(), "the game was not let out");
    assert!(!a.take_outside_eject(), "an eject is taken once");
    assert!(
        ring.read_resume().unwrap().is_some(),
        "no state was kept to resume from"
    );
    a.unseat_outside();
    assert!(matches!(a.phase(), Phase::Shelf));
    assert!(!a.outside_seated());
}

#[test]
fn an_outside_game_leaves_slots_own_picture_settings_alone() {
    use common::StubSnapshot;
    use slot_store::StateRing;
    let (d, mut a) = gba_and_gb();
    let before = slot_store::read_slot_state(d.path());
    a.seat_outside(
        "Crash".into(),
        StateRing::at(d.path().join("States/PS/x/Crash")),
        StubSnapshot::boxed(),
    );
    a.apply(Action::ShaderNext);
    a.apply(Action::ColourCorrectionToggle);
    let after = slot_store::read_slot_state(d.path());
    assert_eq!(before.shader_gba, after.shader_gba);
    assert_eq!(before.shader_gb, after.shader_gb);
    assert_eq!(before.colour_correction, after.colour_correction);
}
