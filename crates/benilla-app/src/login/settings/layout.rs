//! Bounded connection-setup geometry in the glue screen's 768-high coordinates.
//! This fork-only dialog borrows stock glue chrome; it does not change startup semantics.
pub(super) const WIDTH: f32 = 920.0;
pub(super) const HEIGHT: f32 = 704.0;
pub(super) const LIST: [f32; 4] = [22.0, 112.0, 430.0, 348.0];
pub(super) const EDITOR: [f32; 4] = [466.0, 112.0, 432.0, 348.0];
pub(super) const TESTS: [f32; 4] = [22.0, 468.0, 876.0, 40.0];
pub(super) const STATUS: [f32; 4] = [22.0, 536.0, 876.0, 90.0];
pub(super) const FOOTER: [f32; 4] = [22.0, 646.0, 876.0, 38.0];
pub(super) const FIELD_HEIGHT: f32 = 35.0;
pub(super) const FIELD_TOP: f32 = 14.0;
pub(super) const PAGE_TOP: f32 = 302.0;

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
    fn all_eight_fields_and_paging_fit_without_using_footer_space() {
        const {
            assert!(FIELD_TOP + 8.0 * FIELD_HEIGHT <= PAGE_TOP);
            assert!(PAGE_TOP + 34.0 <= LIST[3]);
            assert!(LIST[0] + LIST[2] < EDITOR[0]);
            assert!(LIST[1] + LIST[3] < TESTS[1]);
            assert!(TESTS[1] + TESTS[3] < STATUS[1]);
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
