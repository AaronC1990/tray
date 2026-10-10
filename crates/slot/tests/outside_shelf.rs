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
