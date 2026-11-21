use overseer::{hold, screen};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/vibe/{name}.bin",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn typed(bytes: &[u8]) -> Option<String> {
    hold::input_text(&screen::render(bytes, 120, 40).ansi())
}

#[test]
fn an_idle_vibe_prompt_has_an_empty_input() {
    assert_eq!(typed(&fixture("idle")).as_deref(), Some(""));
    assert!(hold::input_empty(Some(&screen::render(&fixture("idle"), 120, 40).ansi())).ok);
}

#[test]
fn text_typed_at_the_vibe_prompt_is_read_back() {
    assert_eq!(typed(&fixture("typed")).as_deref(), Some("abc"));
}

#[test]
fn a_vibe_prompt_after_a_finished_turn_has_an_empty_input() {
    assert_eq!(typed(&fixture("after_reply")).as_deref(), Some(""));
}

#[test]
fn the_vibe_slash_menu_holds_text_so_the_prompt_is_not_found() {
    assert_eq!(typed(&fixture("slash")), None);
}

#[test]
fn the_vibe_prompt_survives_more_output_than_the_screen_tail() {
    let mut s = screen::Screen::new(120, 40, 0);
    s.advance(&fixture("idle"));
    let mut frames = Vec::new();
    for i in 0..4000 {
        frames.extend_from_slice(
            format!("\x1b[10;3Hline {i:0>90} \x1b[11;3Hsecond {i:0>90}").as_bytes(),
        );
    }
    assert!(frames.len() > 2 * screen::TAIL);
    s.advance(&frames);
    assert_eq!(hold::input_text(&s.grid().ansi()).as_deref(), Some(""));
    assert_eq!(typed(&frames), None);
}
