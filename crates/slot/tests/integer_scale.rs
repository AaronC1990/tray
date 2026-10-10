//! The Integer Scale settings. These rows only appear on panels where they
//! change the picture, which is a process-wide switch, so they live in their
//! own test binary rather than beside the quick menu tests that expect the
//! RG SP's four Screen rows.
mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use common::{app_booting_at, tmp_root_with_carts, CLOCK_IS_SET};
use slot::app::{App, Phase};
use slot::frontend::scale_rows_matter;
use slot::session::Session;
use slot_gfx::GameFit;
use slot_input::{Action, Btn, RawEvent};
use slot_store::{read_slot_state, write_slot_state, SlotState};
use slot_ui::{set_scale_rows, QuickRow, QuickValue};

fn press(a: &mut App, btn: Btn) {
    a.apply(Action::GbaDown(btn));
    a.apply(Action::GbaUp(btn));
}

fn session_playing(root: &Path) -> Session {
    common::clocked(root);
    let mut s = Session::boot(root.to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        s.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
    }
    s
}

#[test]
fn the_rows_only_show_on_a_panel_where_they_change_the_picture() {
    assert!(!scale_rows_matter((720, 480)), "the RG SP is already at 3x");
    assert!(scale_rows_matter((640, 480)), "the RG35XXSP is not");
}

#[test]
fn the_screen_page_holds_each_scale_row_under_its_shader() {
    set_scale_rows(true);
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            ..SlotState::default()
        },
    )
    .unwrap();
    let (mut a, _clock) = app_booting_at(d.path(), CLOCK_IS_SET);
    a.apply(Action::QuickMenu);
    press(&mut a, Btn::A);
    assert_eq!(a.quick_menu(), Some(QuickRow::GbaShader));
    for want in [
        QuickRow::GbaInteger,
        QuickRow::GbShader,
        QuickRow::GbInteger,
        QuickRow::ColourCorrection,
        QuickRow::GbPalettes,
        QuickRow::GbaShader,
    ] {
        press(&mut a, Btn::Down);
        assert_eq!(
            a.quick_menu(),
            Some(want),
            "the page did not wrap on itself"
        );
    }

    assert_eq!(a.quick_value(QuickRow::GbaInteger), Some(QuickValue::Off));
    assert_eq!(a.quick_value(QuickRow::GbInteger), Some(QuickValue::On));
    press(&mut a, Btn::Down);
    press(&mut a, Btn::Right);
    assert!(
        read_slot_state(d.path()).integer_gba,
        "GBA never reached the card"
    );
    press(&mut a, Btn::Down);
    press(&mut a, Btn::Down);
    press(&mut a, Btn::Left);
    assert!(
        !read_slot_state(d.path()).integer_gb,
        "GB never reached the card"
    );
    assert_eq!(a.quick_value(QuickRow::GbaInteger), Some(QuickValue::On));
    assert_eq!(a.quick_value(QuickRow::GbInteger), Some(QuickValue::Off));
}

#[test]
fn a_gba_game_fills_the_screen_until_integer_scale_is_on() {
    let d = tmp_root_with_carts(&["Emerald", "Zzz"]);
    let s = session_playing(d.path());
    assert_eq!(s.app().game_fit(), GameFit::Fill);

    let d = tmp_root_with_carts(&["Emerald", "Zzz"]);
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            integer_gba: true,
            ..SlotState::default()
        },
    )
    .unwrap();
    let s = session_playing(d.path());
    assert_eq!(s.app().game_fit(), GameFit::Whole);
}

#[test]
fn a_game_boy_game_is_whole_at_actual_size_and_fills_when_stretched() {
    let d = common::tmp_root_with_gb_carts(&["Tetris", "Zzz"]);
    let mut s = session_playing(d.path());
    assert_eq!(
        s.app().game_fit(),
        GameFit::Whole,
        "actual size is not whole"
    );
    s.feed([RawEvent::Down(Btn::L1)], 100);
    s.feed([RawEvent::Up(Btn::L1)], 120);
    assert_eq!(s.app().game_fit(), GameFit::Fill, "stretch is not spread");
    s.feed([RawEvent::Down(Btn::R1)], 200);
    s.feed([RawEvent::Up(Btn::R1)], 220);
    assert_eq!(s.app().game_fit(), GameFit::Whole, "R did not give it back");
}

#[test]
fn the_game_boy_bezel_frames_actual_size_but_not_stretched_or_gba() {
    let d = common::tmp_root_with_gb_carts(&["Tetris", "Zzz"]);
    let mut s = session_playing(d.path());
    assert!(
        s.app().frames_game_boy_sides(),
        "actual size has bare sides"
    );
    s.feed([RawEvent::Down(Btn::L1)], 100);
    s.feed([RawEvent::Up(Btn::L1)], 120);
    assert!(
        !s.app().frames_game_boy_sides(),
        "a stretched picture fills the width"
    );

    let d = tmp_root_with_carts(&["Emerald", "Zzz"]);
    let s = session_playing(d.path());
    assert!(
        !s.app().frames_game_boy_sides(),
        "a GBA picture fills the width"
    );
}
