#!/usr/bin/env python3
"""Offline Classic wire fixtures for the route and object state."""
import struct
from types import SimpleNamespace
import unittest
import zlib

from northshire import Northshire
from objects import COMPRESSED_UPDATE_OBJECT, ENTRY, HEALTH, MANA, MONSTER_MOVE, UPDATE_OBJECT, Cursor, Objects, VisibleObject, packed_guid
import route_packets as p


GUID = 0xF130000006001234
PACKED = bytes.fromhex('cb34120630f1')


class ObjectsTest(unittest.TestCase):
    def test_packed_guid_omits_interior_zero_bytes(self):
        self.assertEqual(packed_guid(GUID), PACKED)
        self.assertEqual(Cursor(PACKED).guid(), GUID)
        self.assertEqual(packed_guid(0), b'\0')

    def test_create_values_out_of_range_and_reappearance(self):
        header = bytes.fromhex('01 000000 00')
        movement = struct.pack('<BII4fI6fI', 0x30, 0, 7, 10, 20, 30, 0, 0, 2.5, 7, 4.5, 4.7, 2.5, 3, 1)
        values = bytes.fromhex('01 08004000 06000000 1f000000')
        create = header + b'\x03' + PACKED + b'\x03' + movement + values
        state = Objects()
        compressed = len(create).to_bytes(4, 'little') + zlib.compress(create)
        state.update(COMPRESSED_UPDATE_OBJECT, compressed)
        old = state.visible[GUID]
        self.assertEqual((old.kind, old.generation, old.position, old.run_speed), (3, 1, (10, 20, 30), 7))
        self.assertEqual(old.fields, {ENTRY: 6, HEALTH: 31})
        state.update(UPDATE_OBJECT, header + b'\0' + PACKED + bytes.fromhex('01 00004000 00000000'))
        self.assertEqual(old.fields[HEALTH], 0)
        state.update(UPDATE_OBJECT, header + bytes.fromhex('04 01000000') + PACKED)
        self.assertNotIn(GUID, state.visible)
        state.update(UPDATE_OBJECT, create)
        self.assertEqual(state.visible[GUID].generation, 2)
        self.assertEqual(state.visible[GUID].fields[HEALTH], 31)
        stop = PACKED + struct.pack('<3fIB', 11, 22, 33, 3, 1)
        state.update(MONSTER_MOVE, stop)
        self.assertEqual(state.visible[GUID].position, (11, 22, 33))

    def test_rejects_truncated_values(self):
        with self.assertRaises(struct.error):
            Objects().update(UPDATE_OBJECT, bytes.fromhex('01000000 00 00') + PACKED + b'\x01\x08')


class RoutePacketsTest(unittest.TestCase):
    def test_movement_contains_ordinary_mover_info_and_no_guid(self):
        self.assertEqual(p.movement_body(1, 1000, (1.0, -2.0, 3.0, 0.5)),
                         bytes.fromhex('01000000 e8030000 0000803f 000000c0 00004040 0000003f 00000000'))

    def test_fireball_uses_a_packed_unit_target(self):
        self.assertEqual(p.cast_body(133, GUID), bytes.fromhex('85000000 0200 cb34120630f1'))
        self.assertEqual(p.quest_body(GUID, 7, 0), bytes.fromhex('34120006000030f1 07000000 00000000'))

    def test_dialog_displays_only_offered_quest_ids(self):
        body = bytes.fromhex('34120006000030f1 01000000 00000000 01000000 07000000 02000000 02000000') + b'Cleanup\0'
        self.assertEqual(p.quest_offers(p.GOSSIP, body), (GUID, [(7, 2)]))

    def test_kill_credit_contains_exact_target_and_required_count(self):
        packet = bytes.fromhex('07000000 06000000 03000000 0a000000 34120006000030f1')
        self.assertEqual(p.kill_credit(packet), (7, 6, 3, 10, GUID))

    def test_loot_keeps_server_slot_identity_and_distinguishes_refusal(self):
        body = bytes.fromhex('34120006000030f1 01 05000000 01 03 f3020000 02000000 0a000000 00000000 00000000 00')
        self.assertEqual(p.loot_window(body), (GUID, None, 5, [(3, 755, 2, 0)]))
        self.assertEqual(p.loot_window(bytes.fromhex('34120006000030f1 00 04')), (GUID, 4, 0, []))


class CastPacingTest(unittest.TestCase):
    def test_next_cast_waits_for_the_previous_mana_debit(self):
        route = Northshire()
        route.guid = 1
        route.event = lambda *_args, **_fields: None
        route.face = lambda _pos: None
        route.pump = lambda _duration: None
        route.objects.visible[1] = VisibleObject(1, 1, 4, {MANA: 33})
        target = SimpleNamespace(guid=GUID, position=(1, 0, 0))
        caster_headers = bytes.fromhex('01010101')
        incoming = iter([
            (p.SPELL_START, caster_headers + struct.pack('<IHI', 133, 2, 1500)),
            (p.SPELL_GO, caster_headers + struct.pack('<I', 133)),
            (UPDATE_OBJECT, bytes.fromhex('01000000 00 00 0101 01 00008000 03000000')),
            (UPDATE_OBJECT, bytes.fromhex('01000000 00 00 0101 01 00008000 21000000')),
            (p.SPELL_START, caster_headers + struct.pack('<IHI', 133, 2, 1500)),
            (p.SPELL_GO, caster_headers + struct.pack('<I', 133)),
            (UPDATE_OBJECT, bytes.fromhex('01000000 00 00 0101 01 00008000 03000000')),
        ])
        observed = []
        submitted = []

        def receive(_deadline):
            opcode, body = next(incoming)
            route.observe(opcode, body)
            observed.append(opcode)
            return opcode, body

        def send(opcode, body):
            submitted.append((opcode, body, list(observed), route.player.fields[MANA]))

        route.world = SimpleNamespace(receive=receive, send=send)
        route.fireball(target, 0)
        route.fireball(target, 0)
        expected_cast = p.cast_body(133, GUID)
        self.assertEqual(submitted, [
            (p.CAST, expected_cast, [], 33),
            (p.CAST, expected_cast, [p.SPELL_START, p.SPELL_GO, UPDATE_OBJECT, UPDATE_OBJECT], 33),
        ])
        self.assertEqual(route.player.fields[MANA], 3)


if __name__ == '__main__':
    unittest.main()
