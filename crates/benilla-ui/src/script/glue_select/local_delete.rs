//! Fork-only durable local character confirmation. Realm deletion keeps the stock binding.
//! The stock dialog's OnShow owns the snapshot; a later roster or selection invalidates it.

use std::{cell::RefCell, rc::Rc};

use mlua::{Function, Lua};

use super::{model, GlueCharacter};
use crate::script::{
    glue_core::{call, GlueCall},
    UiScript,
};

#[derive(Clone)]
struct Snapshot {
    identity: Rc<()>,
    index: i32,
    character: GlueCharacter,
}

fn selected(lua: &Lua) -> Option<Snapshot> {
    let m = model(lua);
    let roster = &m.glue.roster;
    let index = roster.cells.selected;
    let character = roster
        .records
        .get(usize::try_from(index).ok()?)?
        .character
        .clone();
    (character.guid != 0).then(|| Snapshot {
        identity: Rc::clone(&roster.delete_identity),
        index,
        character,
    })
}

impl UiScript {
    /// Install after the player's stock GlueXML, only for the local/P2P backend.
    /// A failed installation disables the raw deletion binding, rather than falling back unsafe.
    pub fn install_local_character_delete_guard(&self) -> mlua::Result<()> {
        if self.model_ref().glue.roster.local_delete_guard {
            return Ok(());
        }
        self.model_mut().glue.roster.local_delete_guard = true;
        let pending = Rc::new(RefCell::new(None::<Snapshot>));
        let capture = {
            let pending = Rc::clone(&pending);
            self.lua.create_function(move |lua, ()| {
                *pending.borrow_mut() = selected(lua);
                Ok(())
            })?
        };
        let cancel = {
            let pending = Rc::clone(&pending);
            self.lua.create_function(move |_, ()| {
                pending.borrow_mut().take();
                Ok(())
            })?
        };
        let confirm = self.lua.create_function(
            move |lua, (index, text, expected): (i32, String, String)| {
                if expected.is_empty() || !text.eq_ignore_ascii_case(&expected) {
                    return Ok(());
                }
                let saved = pending.borrow_mut().take();
                if let (Some(saved), Some(current)) = (saved, selected(lua)) {
                    if index.checked_sub(1) == Some(saved.index)
                        && current.index == saved.index
                        && Rc::ptr_eq(&saved.identity, &current.identity)
                        && current.character == saved.character
                    {
                        call(lua, GlueCall::DeleteCharacter(saved.character.guid));
                    }
                }
                Ok(())
            },
        )?;
        // Named parameters are intentional: the real 1.12 Lua grammar rejects expression `...`.
        let install: Function = self
            .lua
            .load(
                r#"
            return function(capture, cancel, confirm)
                local dialog = CharacterDeleteDialog
                local edit = CharacterDeleteEditBox
                assert(dialog and edit, 'local deletion requires the stock dialog')
                local show = dialog:GetScript('OnShow')
                local hide = dialog:GetScript('OnHide')
                dialog:SetScript('OnShow', function()
                    capture()
                    if show then show() end
                end)
                dialog:SetScript('OnHide', function()
                    cancel()
                    if hide then hide() end
                end)
                DeleteCharacter = function(index)
                    if dialog:IsShown() then
                        confirm(index, edit:GetText() or '', DELETE_CONFIRM_STRING or '')
                    end
                end
            end
        "#,
            )
            .eval()?;
        install.call::<()>((capture, cancel, confirm))
    }
}
