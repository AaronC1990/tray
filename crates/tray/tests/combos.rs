//! tray's own button combinations in a disc game, and which buttons they
//! keep from slot and the game.
use slot_input::{Btn, RawEvent};
use tray::play::{Ask, Combos};

fn down(c: &mut Combos, b: Btn) -> (Option<Ask>, bool) {
    c.take(RawEvent::Down(b))
}

fn up(c: &mut Combos, b: Btn) -> (Option<Ask>, bool) {
    c.take(RawEvent::Up(b))
}

#[test]
fn start_and_select_quit_in_either_order() {
    let mut c = Combos::default();
    assert_eq!(
        down(&mut c, Btn::Start),
        (None, true),
        "START alone is the game's"
    );
    assert_eq!(down(&mut c, Btn::Select), (Some(Ask::Quit), false));
    assert_eq!(
        up(&mut c, Btn::Select),
        (None, false),
        "the quitting SELECT's release leaked"
    );
    assert_eq!(up(&mut c, Btn::Start), (None, true));

    let mut c = Combos::default();
    assert_eq!(down(&mut c, Btn::Select), (None, true));
    assert_eq!(down(&mut c, Btn::Start), (Some(Ask::Quit), false));
}

#[test]
fn menu_and_start_no_longer_quit() {
    let mut c = Combos::default();
    down(&mut c, Btn::Menu);
    assert_eq!(down(&mut c, Btn::Start), (None, true));
}

#[test]
fn menu_and_l2_rewind_until_l2_is_let_go() {
    let mut c = Combos::default();
    down(&mut c, Btn::Menu);
    assert_eq!(down(&mut c, Btn::L2), (Some(Ask::RewindStart), false));
    // MENU can be let go first; rewinding goes on until L2 is.
    assert_eq!(up(&mut c, Btn::Menu), (None, true));
    assert_eq!(up(&mut c, Btn::L2), (Some(Ask::RewindStop), false));
    assert_eq!(
        down(&mut c, Btn::L2),
        (None, true),
        "L2 alone is the game's"
    );
}

#[test]
fn menu_with_shoulders_changes_discs_and_fast_forwards() {
    let mut c = Combos::default();
    down(&mut c, Btn::Menu);
    assert_eq!(down(&mut c, Btn::R1).0, Some(Ask::Disc(1)));
    assert_eq!(down(&mut c, Btn::L1).0, Some(Ask::Disc(-1)));
    assert_eq!(down(&mut c, Btn::R2).0, Some(Ask::FastForward));
}
