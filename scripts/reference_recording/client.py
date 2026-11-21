#!/usr/bin/env python3
"""Qualify character login and ordinary logout on a private Classic 5875 rig."""

import argparse
from dataclasses import dataclass
import hashlib
import json
import math
import os
from pathlib import Path
import secrets
import socket
import struct
import time
import zlib


BUILD = 5875
HUMAN_MAGE = b'\x01\x08\x00'

CHAR_CREATE, CHAR_ENUM, PLAYER_LOGIN = 0x36, 0x37, 0x3d
CHAR_CREATED, ROSTER = 0x3a, 0x3b
LOGOUT, LOGOUT_RESPONSE, LOGOUT_COMPLETE = 0x4b, 0x4c, 0x4d
AUTH_CHALLENGE, AUTH_SESSION, AUTH_RESPONSE = 0x1ec, 0x1ed, 0x1ee
UPDATE_OBJECT, COMPRESSED_UPDATE_OBJECT, VERIFY_WORLD = 0xa9, 0x1f6, 0x236
LOGIN_FAILED = 0x41


class HeaderCipher:
    """One Classic header cipher state, for one direction only."""

    def __init__(self, key):
        self.key = key
        self.index = 0
        self.previous = 0

    def apply(self, header, *, decrypt):
        output = bytearray(header)
        for i, value in enumerate(header):
            key_byte = self.key[self.index]
            if decrypt:
                output[i] = ((value - self.previous) & 255) ^ key_byte
                self.previous = value
            else:
                self.previous = ((value ^ key_byte) + self.previous) & 255
                output[i] = self.previous
            self.index = (self.index + 1) % len(self.key)
        return output


class WorldConnection:
    def __init__(self, sock, key, event):
        if len(key) != 40:
            raise ValueError('Expected a 40-byte world session key')
        self.sock = sock
        self.event = event
        self.tx = HeaderCipher(key)
        self.rx = HeaderCipher(key)
        self.encrypted = False
        self.buffer = bytearray()
        self.pending = None

    def send(self, opcode, body=b''):
        header = struct.pack('>H', len(body) + 4) + struct.pack('<I', opcode)
        if self.encrypted:
            header = self.tx.apply(header, decrypt=False)
        self.sock.sendall(header + body)
        self.event('send', opcode=opcode, bytes=len(body),
                   hex='<authentication-excluded>' if opcode == AUTH_SESSION else body.hex())

    def read_exact(self, n, deadline):
        while len(self.buffer) < n:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError('Timed out waiting for a world packet')
            self.sock.settimeout(remaining)
            part = self.sock.recv(65536)
            if not part:
                raise EOFError('World socket closed')
            self.buffer.extend(part)
        data = bytes(self.buffer[:n])
        del self.buffer[:n]
        return data

    def receive(self, deadline):
        if self.pending is None:
            header = self.read_exact(4, deadline)
            if self.encrypted:
                header = self.rx.apply(header, decrypt=True)
            size = int.from_bytes(header[:2], 'big')
            if size < 2:
                raise RuntimeError(f'Invalid world header size {size}')
            self.pending = (int.from_bytes(header[2:], 'little'), size - 2)
        opcode, size = self.pending
        body = self.read_exact(size, deadline)
        self.pending = None
        self.event('receive', opcode=opcode, bytes=len(body), hex=body.hex())
        return opcode, body

    def expect(self, wanted, timeout=60):
        deadline = time.monotonic() + timeout
        while True:
            if time.monotonic() >= deadline:
                raise TimeoutError(f'World did not send opcode {wanted:#x}')
            opcode, body = self.receive(deadline)
            if opcode == wanted:
                return body
            if opcode == LOGIN_FAILED:
                raise RuntimeError(f'Character login refused: {body.hex()}')

    def authenticate(self, account, key):
        opcode, body = self.receive(time.monotonic() + 30)
        if opcode != AUTH_CHALLENGE or len(body) != 4:
            raise RuntimeError('Expected the 5875 world authentication challenge')
        seed = secrets.token_bytes(4)
        proof = hashlib.sha1(account + b'\0' * 4 + seed + body + key).digest()
        addon = b'Blizzard_BindingUI\0' + struct.pack('<BII', 1, 0x4c1c776d, 0)
        auth = struct.pack('<II', BUILD, 0) + account + b'\0' + seed + proof
        self.send(AUTH_SESSION, auth + struct.pack('<I', len(addon)) + zlib.compress(addon))
        self.encrypted = True
        response = self.expect(AUTH_RESPONSE)
        if response[:1] != b'\x0c':
            raise RuntimeError(f'World authentication refused: {response.hex()}')


@dataclass(frozen=True)
class Character:
    guid: int
    name: str
    race: int
    player_class: int
    gender: int
    level: int


def parse_single_character(roster):
    if roster[:1] != b'\x01' or len(roster) < 19:
        raise RuntimeError('Expected a roster with exactly one character')
    name_end = roster.find(b'\0', 9)
    if name_end <= 9 or len(roster) < name_end + 10:
        raise RuntimeError('Truncated or unnamed roster entry')
    character = Character(int.from_bytes(roster[1:9], 'little'), roster[9:name_end].decode('ascii'),
                          *roster[name_end+1:name_end+4], roster[name_end+9])
    if not character.guid:
        raise RuntimeError('World returned an empty character GUID')
    return character


def update_block_count(opcode, body):
    if opcode == COMPRESSED_UPDATE_OBJECT:
        if len(body) < 4:
            raise RuntimeError('Missing compressed update length')
        declared_size, = struct.unpack_from('<I', body)
        inflate = zlib.decompressobj()
        body = inflate.decompress(body[4:], declared_size + 1)
        if not inflate.eof or inflate.unused_data or len(body) != declared_size:
            raise RuntimeError('Compressed object update length mismatch')
    elif opcode != UPDATE_OBJECT:
        raise ValueError(f'Not an object update opcode: {opcode:#x}')
    if len(body) < 5:
        raise RuntimeError('Missing object update header')
    blocks, = struct.unpack_from('<I', body)
    if blocks > len(body) - 5 or body[4] not in (0, 1):
        raise RuntimeError('Invalid object update header')
    return blocks


def qualify(world, *, mode, name, expected_guid, dwell, event):
    encoded_name = name.encode('ascii')
    if mode == 'create':
        world.send(CHAR_ENUM)
        if world.expect(ROSTER) != b'\0':
            raise RuntimeError('Character creation requires an empty roster')
        appearance = b'\0' * 6
        world.send(CHAR_CREATE, encoded_name + b'\0' + HUMAN_MAGE + appearance)
        status = world.expect(CHAR_CREATED)
        if status != b'\x2e':
            raise RuntimeError(f'Character creation refused: {status.hex()}')
    world.send(CHAR_ENUM)
    character = parse_single_character(world.expect(ROSTER))
    if (character.name != name or bytes((character.race, character.player_class, character.gender))
            != HUMAN_MAGE or (expected_guid is not None and character.guid != expected_guid)):
        raise RuntimeError(f'Unexpected character in roster: {character}')
    if character.level != 1:
        raise RuntimeError(f'Expected a level-one character: {character}')
    event('character', **vars(character))
    world.send(PLAYER_LOGIN, struct.pack('<Q', character.guid))
    map_id, *pose = struct.unpack('<Iffff', world.expect(VERIFY_WORLD))
    if map_id != 0 or not all(math.isfinite(value) for value in pose):
        raise RuntimeError(f'Unexpected world location: map {map_id}, pose {pose}')
    event('world_verified', guid=character.guid, map=map_id, pose=pose)

    deadline = time.monotonic() + 45
    while time.monotonic() < deadline:
        opcode, body = world.receive(deadline)
        if opcode in (UPDATE_OBJECT, COMPRESSED_UPDATE_OBJECT):
            blocks = update_block_count(opcode, body)
            if blocks:
                event('in_world_start', blocks=blocks)
                break
    else:
        raise TimeoutError('No object update arrived after world login')
    deadline = time.monotonic() + dwell
    while time.monotonic() < deadline:
        try:
            opcode, body = world.receive(deadline)
        except TimeoutError:
            break
        if opcode in (UPDATE_OBJECT, COMPRESSED_UPDATE_OBJECT):
            event('object_update', blocks=update_block_count(opcode, body))
    event('in_world_end')
    world.send(LOGOUT)
    status = world.expect(LOGOUT_RESPONSE, timeout=15)
    if len(status) != 5 or status[:4] != b'\0' * 4:
        raise RuntimeError(f'Ordinary logout refused: {status.hex()}')
    if world.expect(LOGOUT_COMPLETE, timeout=35):
        raise RuntimeError('Unexpected logout completion payload')
    event('logout_complete', guid=character.guid)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('create', 'reconnect'))
    parser.add_argument('--port', required=True, type=int)
    parser.add_argument('--account', required=True)
    parser.add_argument('--name', required=True)
    parser.add_argument('--session-key-file', required=True, type=Path)
    parser.add_argument('--events', required=True, type=Path)
    parser.add_argument('--guid', type=int)
    parser.add_argument('--dwell', type=float, default=5)
    args = parser.parse_args()
    if (args.guid is not None) != (args.mode == 'reconnect'):
        parser.error('Reconnect requires --guid; create must not supply it')
    if not 1 <= args.dwell <= 30:
        parser.error('--dwell must be between 1 and 30 seconds')
    for variable, value in {'WOW_UNATTENDED': '1', 'WOW_NOSOUND': '1', 'WOW_GM': 'off', 'WOW_GOD': 'off'}.items():
        if os.environ.get(variable) != value:
            parser.error(f'Private recording requires {variable}={value}')
    account = args.account.encode('ascii')
    key = bytes.fromhex(args.session_key_file.read_text())
    os.umask(0o077)
    with args.events.open('x', buffering=1) as events:
        def event(kind, **details):
            events.write(json.dumps({'kind': kind, 'monotonic_ns': time.monotonic_ns(), **details}) + '\n')

        with socket.create_connection(('127.0.0.1', args.port), timeout=15) as sock:
            world = WorldConnection(sock, key, event)
            world.authenticate(account, key)
            qualify(world, mode=args.mode, name=args.name, expected_guid=args.guid,
                    dwell=args.dwell, event=event)


if __name__ == '__main__':
    main()
