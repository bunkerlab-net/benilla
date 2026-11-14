# SPDX-License-Identifier: GPL-3.0-or-later
"""Authored inputs only. No expected result is supplied to the C++ executable."""
import copy
import struct


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def unit(guid, group, kind='creature'):
    return dict(guid=guid, kind=kind, flags=8 if kind == 'player' else 0,
        raw_extra_flags=2, authored_entry=6, has_faction=True,
        faction=dict(faction=group * 100, group_mask=group,
                     enemy_mask=2 if group == 1 else 1, friend_mask=group,
                     enemies=[0, 0, 0, 0], friends=[0, 0, 0, 0]),
        authored_within_range=True, authored_los=True, evading_home=False,
        feigning=False, ignoring_feign=False, can_enter_combat=True,
        unattackable_aura=False, authored_unit_state=0, can_aggro=True,
        force_attacking=False, pet_actions_disabled=False, authored_alive=True)


def cases():
    rows = []
    def add(name, modify=None, sequence=(101,), instanceable=False):
        units = [unit(1, 1), unit(77, 2, 'player'), unit(101, 1)]
        if modify:
            modify(*units)
        rows.append(dict(name=name, input=dict(instanceable=instanceable,
            radius_bits=bits(5.), authored_units=units,
            authored_candidate_sequence=list(sequence))))
        return rows[-1]
    def set_candidate(**values):
        return lambda owner, enemy, candidate: candidate.update(values)
    def set_enemy(**values):
        return lambda owner, enemy, candidate: enemy.update(values)
    def candidate_faction(**values):
        return lambda owner, enemy, candidate: candidate['faction'].update(values)
    add('hostile_defensive_eligible')
    add('hostile_without_defensive_eligible', set_candidate(raw_extra_flags=0))
    add('hostile_no_call_assist_still_candidate', set_candidate(raw_extra_flags=0x802))
    add('self_rejected_before_all_predicates', sequence=(1,))
    add('same_entry_distinct_guid_eligible', sequence=(101,))
    add('outdoor_in_combat_rejected_before_relation', set_candidate(flags=0x80000))
    add('owner_uninteractible_rejected_before_relation', lambda owner, enemy, candidate: owner.update(flags=0x2000000))
    add('candidate_neutral_to_owner_rejected', candidate_faction(friend_mask=0))
    # Owner reciprocally likes group 1, so remove both routes for this control.
    rows[-1]['input']['authored_units'][0]['faction']['friend_mask'] = 0
    add('candidate_neutral_to_player_attackable_not_enemy', candidate_faction(enemy_mask=0))
    add('candidate_friendly_to_player_not_attackable', candidate_faction(enemy_mask=0, friend_mask=3))
    add('hostile_enemy_mask_precedes_friend_mask', candidate_faction(friend_mask=3))
    add('reciprocal_friend_mask_assists', candidate_faction(friend_mask=0))
    add('enemy_faction_list_matches', candidate_faction(enemy_mask=0, enemies=[200, 0, 0, 0]))
    add('enemy_faction_list_later_match', candidate_faction(enemy_mask=0, enemies=[900, 200, 0, 0]))
    add('enemy_faction_list_slot_zero_disables_later_match', candidate_faction(enemy_mask=0, enemies=[0, 200, 0, 0]))
    add('friend_faction_list_matches', candidate_faction(friend_mask=0, friends=[100, 0, 0, 0]))
    add('friend_faction_list_later_match', candidate_faction(friend_mask=0, friends=[900, 100, 0, 0]))
    add('friend_faction_list_slot_zero_disables_later_match', candidate_faction(friend_mask=0, friends=[0, 100, 0, 0]))
    for row in rows[-3:]:
        row['input']['authored_units'][0]['faction']['friend_mask'] = 0
    add('reciprocal_friend_faction_assists', candidate_faction(friend_mask=0))
    rows[-1]['input']['authored_units'][0]['faction'].update(friend_mask=0, friends=[100, 0, 0, 0])
    add('reciprocal_friend_slot_zero_disables_later_match', candidate_faction(friend_mask=0))
    rows[-1]['input']['authored_units'][0]['faction'].update(friend_mask=0, friends=[0, 100, 0, 0])
    add('missing_candidate_template_neutral', set_candidate(has_faction=False))
    add('missing_owner_template_neutral', lambda owner, enemy, candidate: owner.update(has_faction=False))
    add('missing_enemy_template_attackable_not_enemy', set_enemy(has_faction=False))
    add('enemy_evading_precedes_attack', set_enemy(evading_home=True))
    add('enemy_feigning_precedes_attack', set_enemy(feigning=True))
    add('ignoring_enemy_feign_continues', lambda owner, enemy, candidate: (enemy.update(feigning=True), candidate.update(ignoring_feign=True)))
    add('disabled_pet_enemy_precedes_attack', set_enemy(kind='pet', flags=0, pet_actions_disabled=True))
    add('enabled_pet_enemy_continues', set_enemy(kind='pet', flags=0))
    for name, flag in [('spawning', 2), ('not_attackable_1', 0x80), ('untargetable', 0x10000), ('taxi', 0x100000), ('uninteractible', 0x2000000), ('immune_to_npc', 0x200)]:
        add('enemy_' + name + '_cannot_attack', set_enemy(flags=8 | flag))
    add('candidate_immune_to_player_cannot_attack', set_candidate(flags=0x100))
    add('npc_reverse_hostility_attackable_but_not_enemy', lambda owner, enemy, candidate: (enemy.update(kind='creature', flags=0), candidate['faction'].update(enemy_mask=0)))
    add('range_failure_skips_los', set_candidate(authored_within_range=False))
    add('los_failure_after_range', set_candidate(authored_los=False))
    add('semantic_denial_skips_false_range_and_los', set_candidate(flags=0x80000, authored_within_range=False, authored_los=False))
    add('outdoor_dead_candidate_not_checked', set_candidate(authored_alive=False))
    add('outdoor_cannot_aggro_not_checked', set_candidate(can_aggro=False))
    add('outdoor_stunned_candidate_not_checked', set_candidate(authored_unit_state=8))
    add('outdoor_dead_enemy_not_checked', set_enemy(authored_alive=False))
    add('dungeon_in_combat_bypasses_outdoor_combat_guard', set_candidate(flags=0x80000), instanceable=True)
    add('dungeon_neutral_player_can_join_without_is_enemy', candidate_faction(enemy_mask=0), instanceable=True)
    add('dungeon_owner_uninteractible_bypassed', lambda owner, enemy, candidate: owner.update(flags=0x2000000), instanceable=True)
    add('dungeon_cannot_enter_combat_rejected', set_candidate(can_enter_combat=False), instanceable=True)
    add('dungeon_evading_candidate_rejected', set_candidate(evading_home=True), instanceable=True)
    add('dungeon_cannot_aggro_rejected', set_candidate(can_aggro=False), instanceable=True)
    add('dungeon_stunned_candidate_rejected', set_candidate(authored_unit_state=8), instanceable=True)
    add('dungeon_feigning_candidate_rejected', set_candidate(feigning=True), instanceable=True)
    add('dungeon_unattackable_aura_rejected', set_candidate(unattackable_aura=True), instanceable=True)
    add('dungeon_enemy_immunity_rejected', set_enemy(flags=8 | 0x200), instanceable=True)
    row = add('authored_order_duplicates_and_membership', sequence=(1, 102, 103, 101, 102))
    eligible = copy.deepcopy(row['input']['authored_units'][2]); eligible['guid'] = 102
    denied = copy.deepcopy(eligible); denied['guid'] = 103; denied['flags'] = 0x80000
    omitted = copy.deepcopy(eligible); omitted['guid'] = 104
    row['input']['authored_units'].extend([eligible, denied, omitted])
    add('authored_empty_candidate_sequence', sequence=())
    return rows


def command_stream(rows):
    lines = []
    for row in rows:
        x = row['input']
        lines.append(f"case {row['name']} {int(x['instanceable'])}")
        for u in x['authored_units']:
            f = u['faction']
            values = [u['guid'], u['kind'], u['flags'], u['raw_extra_flags'], u['authored_entry'],
                u['has_faction'], f['faction'], f['group_mask'], f['enemy_mask'], f['friend_mask'],
                *f['enemies'], *f['friends'], u['authored_within_range'], u['authored_los'],
                u['evading_home'], u['feigning'], u['ignoring_feign'], u['can_enter_combat'],
                u['unattackable_aura'], u['authored_unit_state'], u['can_aggro'], u['force_attacking'],
                u['pet_actions_disabled'], u['authored_alive']]
            lines.append('unit ' + ' '.join(str(int(v)) if isinstance(v, bool) else str(v) for v in values))
        sequence = x['authored_candidate_sequence']
        lines.append('visit ' + ' '.join(map(str, [x['radius_bits'], len(sequence), *sequence])))
    return ('\n'.join(lines) + '\n').encode()
