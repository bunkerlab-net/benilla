// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, ObjectMgr.h:113-128.
// Upstream GPL-2.0-or-later, Copyright CMaNGOS contributors.
use super::*;
/// BroadcastText::GetText, without EventAI's separate male-default override.
pub fn localized_broadcast_text<'a>(
    base: &'a EventAiBroadcastSource,
    translated: Option<&'a EventAiBroadcastLocale>,
    locale: EventAiLocale,
    gender: u8,
) -> Result<&'a str, NativeError> {
    if gender > 2
        || translated.is_some_and(|row| row.id != base.id || row.locale != locale.source_name())
    {
        return Err(NativeError::AuthorityBinding);
    }
    let female = gender != 0 && base.text1.as_deref().is_some_and(|s| !s.is_empty());
    let default = if female { &base.text1 } else { &base.text };
    Ok(translated
        .filter(|_| locale != EventAiLocale::EnUs)
        .and_then(|row| {
            if female {
                row.text1.as_deref()
            } else {
                row.text.as_deref()
            }
        })
        .filter(|s| !s.is_empty())
        .or(default.as_deref())
        .unwrap_or(""))
}
