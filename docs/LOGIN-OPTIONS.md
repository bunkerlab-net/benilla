# Login Options

The native login screen's **Settings** button opens the same `BenillaOptionsFrame`
used by the in-world game menu: Controls, Interface, Action Bars, Combat, Chat,
Keybindings, Nameplates, Graphics and Audio, with its actual checkboxes, sliders,
dropdowns, search and keybinding controls. It reads the player's own Classic
1.12.1 FrameXML and the existing Benilla layer. No game files are redistributed.

This is an isolated, temporary UI VM. The normal login VM is restored on closing;
world entry still builds a fresh gameplay VM. No player-login event is synthesized
and no third-party addons run before login. The game HUD is hidden; the existing
login scene remains behind Options. Shared dropdowns, tooltips and confirmation
popups remain visible.

Settings keep the existing game's semantics: immediate controls apply immediately;
Apply commits staged controls; Close or Escape discards still-pending Apply edits.
Committed CVars, saved globals and account bindings survive closing and world entry.
Character-specific bindings, tutorials, cloak/helm visibility and the four
server-owned action-bar toggles are disabled until a character enters the world,
with an explanatory tooltip. Local options are not represented as server settings.

**Connection setup** is a separate native recovery/configuration panel. It edits
executable-adjacent `config.toml`, retains the transactional save/repair behavior,
and exposes explicitly clicked diagnostics and endpoint-ticket reveal/copy.
Startup changes need a restart. It remains usable if the game-data/UI load fails.
An empty P2P passphrase is permitted but provides no privacy; short passphrases are
easy to guess. Exact text is retained and the parser's 1024-byte resource cap applies.

## Verification

Headless shell regressions cover HUD isolation, backdrop and overlay visibility,
server-owned control guards, account-only bindings, scale changes and repeated
show/close. App lifecycle tests cover preserving/restoring the original VM and
idempotent close. The production-bootstrap category test requires the player's
Classic data and skips when it is absent, as other reference-UI tests do.

Visual acceptance still requires a client run with those assets: check every
category, search, dropdowns and dialogs, resize/UI scale, Apply/Close/Escape,
reopening and continuing through login. Headless tests do not establish pixel
parity with a screenshot.
