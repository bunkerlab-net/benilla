# SPDX-License-Identifier: GPL-3.0-or-later
"""Authored peer-help inputs; expected results come only from the source executable."""
import candidate_cases

bits = candidate_cases.bits


def unit(guid, group, kind='creature'):
    result = candidate_cases.unit(guid, group, kind)
    result.update(can_check=True, can_call=True, active_victim=0, singleton_attacker=0,
                  authored_accessible=True, authored_visible=True,
                  authored_ai=True, authored_ai_order=0, template_call_for_help_bits=bits(5.))
    return result


def cases():
    rows = []

    def add(name, modify=None, operations=None):
        observer, target, victim, fallback = unit(1, 1), unit(101, 1), unit(77, 2, 'player'), unit(78, 2, 'player')
        target.update(flags=0x80000, active_victim=77)
        if modify:
            modify(observer, target, victim, fallback)
        rows.append(dict(name=name, input=dict(instanceable=False,
            authored_units=[observer, target, victim, fallback],
            operations=operations or ['observe check'])))
        return rows[-1]

    def observer(**values):
        return lambda o, t, v, f: o.update(values)

    def target(**values):
        return lambda o, t, v, f: t.update(values)

    def victim(**values):
        return lambda o, t, v, f: v.update(values)

    add('positive_accessibility_continues_to_attack')
    add('active_victim_precedes_different_attacker', target(singleton_attacker=78))
    add('active_denied_victim_does_not_fall_back', lambda o, t, v, f: (t.update(singleton_attacker=78), v.update(flags=8 | 0x10000)))
    add('singleton_attacker_fallback', target(active_victim=0, singleton_attacker=78))
    add('no_victim_or_attacker_returns_before_flags', lambda o, t, v, f: (t.update(active_victim=0), o.update(flags=0x80000, can_call=False, can_check=False)))
    add('observer_help_disabled', observer(can_check=False))
    add('target_help_disabled', target(can_check=False))
    add('both_help_disabled_observer_first', lambda o, t, v, f: (o.update(can_check=False), t.update(can_check=False)))
    add('live_help_fields_rechecked', operations=['observe enabled', 'set_check 1 0', 'observe observer_disabled',
        'set_check 1 1', 'set_check 101 0', 'observe target_disabled', 'set_check 101 1', 'observe reenabled'])
    add('observer_combat_before_call_flags', observer(flags=0x80000, can_call=False, can_check=False))
    add('observer_cannot_call', observer(can_call=False))
    add('target_cannot_call', target(can_call=False))
    add('target_cannot_call_before_observer_help_disabled', lambda o, t, v, f: (o.update(can_check=False), t.update(can_call=False)))
    add('live_call_fields_rechecked', operations=['observe enabled', 'set_call 1 0', 'observe observer_disabled',
        'set_call 1 1', 'set_call 101 0', 'observe target_disabled', 'set_call 101 1', 'observe reenabled'])
    add('target_retreating_before_confusion_and_stun', target(authored_unit_state=0x400, flags=0x80000 | 0x400000 | 0x40000))
    add('target_confused_before_stunned', target(flags=0x80000 | 0x400000 | 0x40000))
    add('target_stunned_flag', target(flags=0x80000 | 0x40000))
    add('target_confused_state_without_flag_not_checked', target(authored_unit_state=0x2000))
    add('target_stunned_state_without_flag_not_checked', target(authored_unit_state=8))
    add('observer_stunned_state_denies_initiation', observer(authored_unit_state=8))
    add('observer_feigning_state_denies_initiation', observer(authored_unit_state=4))
    add('observer_spawning_denies_initiation', observer(flags=2))
    add('observer_uninteractible_denies_initiation', observer(flags=0x2000000))
    add('observer_cannot_aggro_denies_initiation', observer(can_aggro=False))
    add('observer_forced_attack_bypasses_spawning', observer(flags=2, force_attacking=True))
    add('observer_forced_attack_still_requires_aggro', observer(flags=2, force_attacking=True, can_aggro=False))
    add('observer_initiation_denial_precedes_bad_victim', lambda o, t, v, f: (o.update(can_aggro=False), v.update(evading_home=True)))
    add('victim_evading_denies_sight', victim(evading_home=True))
    add('victim_feigning_denies_sight', victim(feigning=True))
    add('victim_untargetable_denies_attack', victim(flags=8 | 0x10000))
    add('victim_npc_immunity_denies_attack', victim(flags=8 | 0x200))
    add('observer_player_immunity_denies_attack', observer(flags=0x100))
    add('observer_neutral_to_victim_denies_sight', lambda o, t, v, f: o['faction'].update(enemy_mask=0))
    add('inaccessible_victim_stops_before_visibility', victim(authored_accessible=False))
    add('invisible_victim_stops_before_target_range', victim(authored_visible=False))
    add('target_range_failure_stops_before_los', target(authored_within_range=False))
    add('target_los_failure_stops_before_assist', target(authored_los=False))
    add('target_uninteractible_denies_assist_after_geometry', target(flags=0x80000 | 0x2000000))
    add('target_neutral_denies_assist_after_geometry', lambda o, t, v, f: (o['faction'].update(friend_mask=0), t['faction'].update(friend_mask=0)))
    add('target_geometry_not_victim_geometry', victim(authored_within_range=False, authored_los=False))
    add('victim_accessibility_not_target_accessibility', target(authored_accessible=False, authored_visible=False))
    add('world_default_ignores_template_positive', observer(template_call_for_help_bits=bits(31.)))
    add('world_default_ignores_template_zero', observer(template_call_for_help_bits=bits(0.)))
    add('world_default_ignores_template_negative', observer(template_call_for_help_bits=bits(-1.)))
    add('authored_world_radius_override', operations=[f'radius {bits(9.)}', 'observe check'])
    add('fleeing_target_interrupts_panic_after_attack', target(authored_ai_order=2))
    add('retreating_order_without_retreat_state_does_not_interrupt', target(authored_ai_order=3))
    add('target_without_ai_still_attacked', target(authored_ai=False))
    add('fleeing_denied_target_never_interrupts', lambda o, t, v, f: (t.update(authored_ai_order=2), v.update(authored_accessible=False)))
    add('direct_helper_does_not_check_target_combat', target(flags=0))
    add('live_help_ignores_authored_raw_extra_flags', observer(raw_extra_flags=0x802))
    return rows


def command_stream(rows):
    lines = []
    for row in rows:
        x = row['input']
        # Reuse only the authored unit-field serialization, never expected output.
        base = dict(name=row['name'], input=dict(instanceable=x['instanceable'],
            radius_bits=bits(5.), authored_units=x['authored_units'], authored_candidate_sequence=[]))
        lines.extend(candidate_cases.command_stream([base]).decode().splitlines()[:-1])
        for u in x['authored_units']:
            values = [u['guid'], u['can_check'], u['can_call'], u['active_victim'], u['singleton_attacker'],
                u['authored_accessible'], u['authored_visible'], u['authored_ai'], u['authored_ai_order'],
                u['template_call_for_help_bits']]
            lines.append('helper ' + ' '.join(str(int(v)) if isinstance(v, bool) else str(v) for v in values))
        lines.extend(x['operations'])
    return ('\n'.join(lines) + '\n').encode()
