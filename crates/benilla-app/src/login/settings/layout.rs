//! Bounded connection-setup geometry in the glue screen's 768-high coordinates.
//! This fork-only dialog borrows stock glue chrome and bounds every group without paging.
pub(super) const WIDTH: f32 = 920.0;
pub(super) const HEIGHT: f32 = 704.0;
pub(super) const LIST: [f32; 4] = [22.0, 112.0, 430.0, 348.0];
pub(super) const EDITOR: [f32; 4] = [466.0, 112.0, 432.0, 348.0];
pub(super) const TESTS: [f32; 4] = [22.0, 468.0, 876.0, 40.0];
pub(super) const STATUS: [f32; 4] = [22.0, 536.0, 876.0, 90.0];
pub(super) const FOOTER: [f32; 4] = [22.0, 646.0, 876.0, 38.0];
pub(super) const FIELD_HEIGHT: f32 = 35.0;
// Two text line boxes centered inside the stock title highlight's inset gold face.
pub(super) const FIELD_LABEL_HEIGHT: f32 = 15.0;
pub(super) const FIELD_VALUE_HEIGHT: f32 = 14.0;
pub(super) const FIELD_TEXT_TOP: f32 =
    (FIELD_HEIGHT - FIELD_LABEL_HEIGHT - FIELD_VALUE_HEIGHT) / 2.0;
pub(super) const FIELD_TEXT_LEFT: f32 = 16.0;
pub(super) const FIELD_TEXT_WIDTH: f32 = 374.0;
pub(super) const GROUP_TOP: f32 = 12.0;
pub(super) const FIELD_TOP: f32 = 52.0;

pub(super) fn scale(width: f32, height: f32) -> f32 {
    (height / 768.0).min(width / 960.0).min(2.2)
}

/// A shared, independently testable clamp for bounded diagnostic/ticket scrolling.
pub(super) fn scroll(current: f32, delta: f32, content: f32, visible: f32) -> f32 {
    (current - delta).clamp(0.0, (content - visible).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_and_footer_fit_minimum_and_narrow_recovery_windows() {
        for (w, h) in [
            (800.0, 600.0),
            (1024.0, 768.0),
            (1280.0, 1024.0),
            (640.0, 360.0),
            (640.0, 700.0),
            (3840.0, 2160.0),
        ] {
            let s = scale(w, h);
            assert!(WIDTH * s < w && HEIGHT * s < h);
            for [x, y, width, height] in [LIST, EDITOR, TESTS, STATUS, FOOTER] {
                assert!(x >= 0.0 && y >= 0.0);
                assert!(x + width <= WIDTH && y + height <= HEIGHT);
            }
            const { assert!(STATUS[1] + STATUS[3] < FOOTER[1]) };
        }
    }

    #[test]
    fn largest_group_and_tabs_fit_without_using_footer_space() {
        const {
            assert!(GROUP_TOP + 32.0 < FIELD_TOP);
            assert!(FIELD_TOP + 7.0 * FIELD_HEIGHT + 12.0 <= LIST[3]);
            assert!(LIST[0] + LIST[2] < EDITOR[0]);
            assert!(LIST[1] + LIST[3] < TESTS[1]);
            assert!(TESTS[1] + TESTS[3] < STATUS[1]);
        };
    }

    #[test]
    fn row_text_has_equal_vertical_margins_and_room_for_font_line_height() {
        const {
            assert!(FIELD_TEXT_TOP >= 3.0);
            assert!(FIELD_TEXT_TOP * 2.0 + FIELD_LABEL_HEIGHT + FIELD_VALUE_HEIGHT == FIELD_HEIGHT);
            assert!(FIELD_LABEL_HEIGHT >= 13.0 * 1.15);
            assert!(FIELD_VALUE_HEIGHT >= 12.0 * 1.15);
            assert!(FIELD_TEXT_LEFT >= 12.0);
            assert!(FIELD_TEXT_LEFT * 2.0 + FIELD_TEXT_WIDTH == 406.0);
        };
    }

    #[test]
    fn long_status_is_scrollable_and_short_status_cannot_overscroll() {
        assert_eq!(scroll(0.0, -48.0, 1000.0, 90.0), 48.0);
        assert_eq!(scroll(880.0, -48.0, 1000.0, 90.0), 910.0);
        assert_eq!(scroll(40.0, 60.0, 1000.0, 90.0), 0.0);
        assert_eq!(scroll(400.0, -48.0, 30.0, 90.0), 0.0);
    }
}
