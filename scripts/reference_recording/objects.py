"""Classic 5875 object updates and the visible lifetime of each GUID."""

from dataclasses import dataclass, field
import math
import struct
import time
import zlib


UPDATE_OBJECT, COMPRESSED_UPDATE_OBJECT, DESTROY_OBJECT, MONSTER_MOVE = 0xa9, 0x1f6, 0xaa, 0xdd
ENTRY, HEALTH, MANA, MAX_HEALTH, MAX_MANA, FLAGS, DYNAMIC_FLAGS = 3, 22, 23, 28, 29, 46, 143
QUEST_FIRST, QUEST_COUNT = 0xc6, 20
XP, COIN = 0x2cc, 0x498


class Cursor:
    def __init__(self, data):
        self.data = data
        self.offset = 0

    def unpack(self, format):
        format = '<' + format
        values = struct.unpack_from(format, self.data, self.offset)
        self.offset += struct.calcsize(format)
        return values if len(values) != 1 else values[0]

    def take(self, n):
        end = self.offset + n
        if end > len(self.data):
            raise ValueError('Truncated Classic packet')
        data = self.data[self.offset:end]
        self.offset = end
        return data

    def string(self):
        end = self.data.index(b'\0', self.offset)
        return self.take(end + 1 - self.offset)[:-1]

    def guid(self):
        mask = self.unpack('B')
        value = 0
        for i in range(8):
            if mask & (1 << i):
                value |= self.unpack('B') << (8 * i)
        return value

    def done(self):
        if self.offset != len(self.data):
            raise ValueError(f'{len(self.data) - self.offset} trailing Classic packet bytes')


def packed_guid(guid):
    out = bytearray(1)
    for i, byte in enumerate(guid.to_bytes(8, 'little')):
        if byte:
            out[0] |= 1 << i
            out.append(byte)
    return bytes(out)


def update_payload(opcode, body):
    if opcode == COMPRESSED_UPDATE_OBJECT:
        size, = struct.unpack_from('<I', body)
        inflate = zlib.decompressobj()
        body = inflate.decompress(body[4:], size + 1)
        if not inflate.eof or inflate.unused_data or len(body) != size:
            raise ValueError('Compressed object update length mismatch')
    elif opcode != UPDATE_OBJECT:
        raise ValueError(f'Not an object update: {opcode:#x}')
    return body


def _values(reader):
    count = reader.unpack('B')
    masks = reader.unpack(f'{count}I') if count else ()
    if count == 1:
        masks = (masks,)
    values = {}
    for i, mask in enumerate(masks):
        while mask:
            bit = mask & -mask
            values[32 * i + bit.bit_length() - 1] = reader.unpack('I')
            mask ^= bit
    return values


def _movement(reader):
    update_flags = reader.unpack('B')
    pose = speed = spline = None
    if update_flags & 0x20:
        flags, _, *pose = reader.unpack('II4f')
        if flags & 0x02000000:
            reader.take(24)  # Raw transport GUID, then its local pose.
        if flags & 0x00200000:
            reader.take(4)
        reader.take(4)  # Fall time.
        if flags & 0x2000:
            reader.take(16)
        if flags & 0x04000000:
            reader.take(4)
        _, speed, *_ = reader.unpack('6f')
        if flags & 0x00400000:
            spline_flags = reader.unpack('I')
            for flag, size in ((0x10000, 12), (0x20000, 8), (0x40000, 4)):
                if spline_flags & flag:
                    reader.take(size)
                    break
            elapsed, duration, _, count = reader.unpack('4I')
            reader.take(count * 12)
            end = reader.unpack('3f')
            # The snapshot already reports the current interpolated position.
            if not spline_flags & 0x100000 and duration > elapsed:
                spline = (duration - elapsed, [tuple(pose[:3]), end])
    elif update_flags & 0x40:
        pose = reader.unpack('4f')
    if update_flags & 0x08:
        reader.take(4)
    if update_flags & 0x10:
        reader.take(4)
    if update_flags & 0x04:
        reader.guid()
    if update_flags & 0x02:
        reader.take(4)
    return pose, speed, spline


def _signed(value, bits):
    return value - (1 << bits) if value & (1 << (bits - 1)) else value


def _monster_move(reader):
    guid = reader.guid()
    start = reader.unpack('3f')
    _, kind = reader.unpack('IB')
    if kind == 1:
        reader.done()
        return guid, start, None
    if kind not in (0, 2, 3, 4):
        raise ValueError(f'Unknown creature movement type {kind}')
    reader.take({0: 0, 2: 12, 3: 8, 4: 4}[kind])
    flags, duration, count = reader.unpack('III')
    path = [start]
    if flags & 0x200:
        path.extend(reader.unpack('3f') for _ in range(count))
    elif count:
        end = reader.unpack('3f')
        for _ in range(count - 1):
            packed = reader.unpack('I')
            offset = (_signed(packed & 0x7ff, 11), _signed((packed >> 11) & 0x7ff, 11),
                      _signed(packed >> 22, 10))
            path.append(tuple(v - d * 0.25 for v, d in zip(end, offset)))
        path.append(end)
    reader.done()
    return guid, start, (duration, path)


@dataclass
class VisibleObject:
    guid: int
    generation: int
    kind: int
    fields: dict = field(default_factory=dict)
    base: tuple | None = None
    run_speed: float | None = None
    spline: tuple | None = None

    @property
    def position(self):
        if self.spline is None:
            return self.base
        start, duration, path, lengths = self.spline
        remaining = lengths[-1] * min(1.0, (time.monotonic() - start) / duration)
        for i, (a, b) in enumerate(zip(lengths, lengths[1:])):
            if remaining < b:
                ratio = (remaining - a) / (b - a)
                return tuple(x + (y - x) * ratio for x, y in zip(path[i], path[i + 1]))
        return path[-1]

    def move(self, pose, speed=None, spline=None):
        if pose is not None:
            if not all(math.isfinite(v) for v in pose):
                raise ValueError(f'Nonfinite pose for {self.guid:#x}')
            self.base = tuple(pose[:3])
            self.spline = None
        if speed is not None:
            self.run_speed = speed
        if spline and spline[0] and len(spline[1]) >= 2:
            duration, path = spline
            lengths = [0.0]
            for start, end in zip(path, path[1:]):
                lengths.append(lengths[-1] + math.dist(start, end))
            self.spline = (time.monotonic(), duration * 0.001, path, lengths)


class Objects:
    def __init__(self):
        self.visible = {}
        self.generations = {}

    def update(self, opcode, body):
        """Apply one packet. A reappearing GUID gets a distinct visible lifetime."""
        reader = Cursor(body)
        if opcode == DESTROY_OBJECT:
            self.visible.pop(reader.unpack('Q'), None)
            reader.done()
            return
        if opcode == MONSTER_MOVE:
            guid, start, spline = _monster_move(reader)
            if guid in self.visible:
                self.visible[guid].move(start, spline=spline)
            return
        if opcode not in (UPDATE_OBJECT, COMPRESSED_UPDATE_OBJECT):
            return
        reader = Cursor(update_payload(opcode, body))
        count, transport = reader.unpack('IB')
        if transport not in (0, 1):
            raise ValueError('Invalid object update transport flag')
        for _ in range(count):
            block = reader.unpack('B')
            if block in (4, 5):
                n = reader.unpack('I')
                for _ in range(n):
                    guid = reader.guid()
                    if block == 4:
                        self.visible.pop(guid, None)
                continue
            if block not in (0, 1, 2, 3):
                raise ValueError(f'Unknown object update block {block}')
            guid = reader.guid()
            if block in (2, 3):
                kind = reader.unpack('B')
                generation = self.generations.get(guid, 0) + 1
                self.generations[guid] = generation
                self.visible[guid] = VisibleObject(guid, generation, kind)
            obj = self.visible.get(guid)
            if block in (1, 2, 3):
                movement = _movement(reader)
                if obj is not None:
                    obj.move(*movement)
            if block in (0, 2, 3):
                fields = _values(reader)
                if obj is not None:
                    obj.fields.update(fields)
        reader.done()
