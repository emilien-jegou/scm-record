use std::borrow::Cow;
use crate::helpers::TestingInput;
use super::*;
use assert_matches::assert_matches;

#[test]
fn test_event_source_testing() {
    let mut event_source = TestingInput::new(80, 24, [Event::QuitCancel]);
    assert_matches!(
        event_source.next_events().unwrap().as_slice(),
        &[Event::QuitCancel]
    );
    assert_matches!(
        event_source.next_events().unwrap().as_slice(),
        &[Event::None]
    );
}

#[test]
fn test_quit_returns_error() {
    let state = RecordState::default();
    let mut input = TestingInput::new(80, 24, [Event::QuitCancel]);
    let recorder = Recorder::new(state, &mut input);
    assert_matches!(recorder.run(), Err(RecordError::Cancelled));

    let state = RecordState {
        is_read_only: false,
        commits: vec![Commit::default(), Commit::default()],
        files: vec![File {
            old_path: None,
            path: Cow::Borrowed(Path::new("foo/bar")),
            file_mode: FileMode::FILE_DEFAULT,
            sections: Default::default(),
        }],
    };
    let mut input = TestingInput::new(80, 24, [Event::QuitAccept]);
    let recorder = Recorder::new(state.clone(), &mut input);
    assert_eq!(recorder.run().unwrap(), state);
}

fn test_push_lines_from_span_impl(line: &str) {
    let mut spans = Vec::new();
    push_spans_from_line(line, &mut spans); // assert no panic
}

proptest::proptest! {
    #[test]
    fn test_push_lines_from_span(line in ".*") {
        test_push_lines_from_span_impl(line.as_str());
    }
}
