#!/usr/bin/env python3
"""Exercise Classic 5875 wire framing without a running realm."""

import socket
import struct
import time
import unittest
import zlib

from client import (
    AUTH_SESSION,
    COMPRESSED_UPDATE_OBJECT,
    UPDATE_OBJECT,
    Character,
    WorldConnection,
    parse_single_character,
    update_block_count,
)


class WireTests(unittest.TestCase):
    def test_auth_payload_is_sent_but_not_recorded(self):
        events = []
        client, server = socket.socketpair()
        with client, server:
            world = WorldConnection(client, bytes(range(40)), lambda kind, **fields: events.append((kind, fields)))
            world.send(AUTH_SESSION, b'proof')
            self.assertEqual(server.recv(11), bytes.fromhex('0009ed010000') + b'proof')
        self.assertEqual(events, [('send', {'opcode': AUTH_SESSION, 'bytes': 5,
                                            'hex': '<authentication-excluded>'})])

    def test_fragmented_body_preserves_encrypted_header_state(self):
        client, server = socket.socketpair()
        with client, server:
            world = WorldConnection(client, bytes(range(40)), lambda *_args, **_fields: None)
            world.encrypted = True
            server.sendall(bytes.fromhex('0005f1f3') + b'\x0c')
            with self.assertRaises(TimeoutError):
                world.receive(time.monotonic() + 0.05)
            server.sendall(b'\x00' + bytes.fromhex('f7fef5fb'))
            self.assertEqual(world.receive(time.monotonic() + 1), (0x1ee, b'\x0c\x00'))
            self.assertEqual(world.receive(time.monotonic() + 1), (0x1f1, b''))

    def test_roster_decodes_guid_and_character_identity(self):
        roster = bytes.fromhex('01 7856341200000000') + b'Northshire\x00'
        roster += bytes.fromhex('01 08 00 00 00 00 00 00 01') + bytes(140)
        self.assertEqual(parse_single_character(roster), Character(0x12345678, 'Northshire', 1, 8, 0, 1))
        with self.assertRaisesRegex(RuntimeError, 'exactly one'):
            parse_single_character(b'\x00')
        with self.assertRaisesRegex(RuntimeError, 'Truncated'):
            parse_single_character(roster[:22])

    def test_compressed_and_uncompressed_updates_use_the_same_block_count(self):
        # One out-of-range block with one packed GUID.
        update = bytes.fromhex('01000000 00 04 01000000 01 2a')
        compressed = zlib.compress(update)
        self.assertEqual(update_block_count(UPDATE_OBJECT, update), 1)
        self.assertEqual(update_block_count(COMPRESSED_UPDATE_OBJECT, struct.pack('<I', 12) + compressed), 1)
        with self.assertRaisesRegex(RuntimeError, 'length mismatch'):
            update_block_count(COMPRESSED_UPDATE_OBJECT, struct.pack('<I', 11) + compressed)


if __name__ == '__main__':
    unittest.main()
