# Bounded ordinary quest abandonment and reacceptance

Source: CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`QuestHandler.cpp::HandleQuestLogRemoveQuest`, `Player.cpp::SetQuestStatus`,
`TakeQuestSourceItem`, `AddQuest`, and `QuestDef.h`. Classic log capacity is20;
TIMED is0x040 (not the KILL_OR_CAST bit0x020).

## Source order and retained state

The stock QuestlogRemove command names one uint8 log slot. Slot20 or greater is
silently ignored by the controller. An empty valid slot still executes the source
slot-clear call. An occupied slot first takes the quest source item, then removes
timed membership if applicable, sets quest statusNONE, and finally clears the slot.
There is no alive, giver, distance or completion guard on this handler.

The catalog admits inert source items through the existing audited inventory host.
Abandonment removes up to the source count before status changes, with ordinary
item-removal callbacks. The native inventory excludes bank/keyring and equipped
containers. A host with bank copies refuses source-item removal explicitly.
Ordinary required deliver items that aren't source items remain. Source-spell,
timer, required-source, and unsupported script dependencies still refuse.

StatusNONE is retained in the quest record, including prior objective counters;
reward history is not deleted. SetQuestStatus's WorldObjects callback sees statusNONE
while the slot is still occupied. Then LogSet clears all three slot words. No
LogComplete callback is invented. This transient view is not a valid checkpoint:
durable restore rejects loggedNONE, rewardedNONE and unlogged active records.

Reacceptance reuses the first free slot, resets applicable objective credit and
recounts deliver progress from inventory. Status-aware eligibility, previous/next
chain checks, starter/finisher menus and markers treat retainedNONE as untaken.
RewardedComplete records remain blocked in the existing nonrepeatable domain.

## Native proof boundary

Native abandonment explicitly refuses quests with a reputation objective. Its
current visibility-history proof owns one acceptance callback per retained record;
reaccepting such a quest needs a composed repeated-visibility receipt. This is an
**unsupported host-proof case**, not an upstream rejection, equipment error or
invented source guard. The refusal happens before any mutation or publication.
Ordinary quests remain supported when global reputation is enabled; their existing
reward and reputation histories stay validated without relaxing the proof.

The real controller routes the stock Lua/UI command through authenticated input,
source reduction, projection and checkpoint outbox. WorldObjects has no invented
NPC-status packet in this no-gameobject slice: normal explicit status/menu queries
show the reoffered quest. Checkpoint failure withholds slot-clear events until retry;
reopen preservesNONE history and does not re-abandon or reward the quest.

The current native format and quest-lifecycle-v5 reject prior checkpoint identities without migration.
Tests use original authored fixtures, actual Lua/controller commands, real simulated
death, checkpoint failure/retry/reopen, replay and rehashed malformed states. This
is not graphical user verification, private content admission, a native capability,
or a claim that a populated Classic world is now available.
