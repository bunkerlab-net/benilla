//! Pure confirmation edge: cancellation wins and one snapshot can be submitted only once.
pub(super) fn armed(typed: &str, confirm: &str) -> bool {
    !confirm.is_empty() && typed.eq_ignore_ascii_case(confirm)
}

pub(super) enum Decision<T> {
    Pending,
    Cancelled,
    Confirmed(T),
}

pub(super) fn decide<T>(
    target: &mut Option<T>,
    armed: bool,
    submit: bool,
    cancel: bool,
) -> Decision<T> {
    if cancel {
        target.take();
        Decision::Cancelled
    } else if armed && submit {
        target.take().map_or(Decision::Pending, Decision::Confirmed)
    } else {
        Decision::Pending
    }
}
