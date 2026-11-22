"""The Classic 5875 packets used by the Northshire quest route."""
import struct

from objects import Cursor, packed_guid


MOVE_START, MOVE_STOP, MOVE_HEARTBEAT, MOVE_FACING = 0xb5, 0xb7, 0xee, 0xda
SELECTION, CAST = 0x13d, 0x12e
CAST_RESULT, SPELL_START, SPELL_GO, SPELL_DELAYED = 0x130, 0x131, 0x132, 0x1e2
GOSSIP, HELLO, QUEST_LIST, QUERY_QUEST, DETAILS, ACCEPT = 0x17d, 0x184, 0x185, 0x186, 0x188, 0x189
REQUEST_ITEMS, REQUEST_REWARD, OFFER_REWARD, CHOOSE_REWARD = 0x18b, 0x18c, 0x18d, 0x18e
QUEST_INVALID, REWARDED, QUEST_FAILED, QUEST_LOG_FULL, QUEST_KILL = 0x18f, 0x191, 0x192, 0x195, 0x199
LOOT, LOOT_MONEY, RELEASE, LOOT_RESPONSE, RELEASED = 0x15d, 0x15e, 0x15f, 0x160, 0x161
LOOT_REMOVED, LOOT_CLEAR_MONEY, ITEM_PUSH, PICKUP_ERROR, TAKE_ITEM = 0x162, 0x165, 0x166, 0x112, 0x108
FACTIONS, STANDING, INITIAL_SPELLS = 0x122, 0x124, 0x12a
FIREBALL = 133


def movement_body(flags, milliseconds, pose):
    return struct.pack('<II4fI', flags, milliseconds & 0xffffffff, *pose, 0)


def guid_body(guid):
    return struct.pack('<Q', guid)


def quest_body(guid, quest, reward=None):
    body = struct.pack('<QI', guid, quest)
    return body if reward is None else body + struct.pack('<I', reward)


def cast_body(spell, target):
    return struct.pack('<IH', spell, 2) + packed_guid(target)


def quest_offers(opcode, body):
    r = Cursor(body)
    npc = r.unpack('Q')
    if opcode == GOSSIP:
        r.take(4)
        items = r.unpack('I')
        for _ in range(items):
            r.take(6)
            r.string()
        count = r.unpack('I')
    elif opcode == QUEST_LIST:
        r.string()
        r.take(8)
        count = r.unpack('B')
    else:
        raise ValueError(f'Not a quest-list opcode: {opcode:#x}')
    quests = []
    for _ in range(count):
        qid, icon, _ = r.unpack('III')
        r.string()
        quests.append((qid, icon))
    r.done()
    return npc, quests


def reward_offer(body):
    r = Cursor(body)
    npc, quest = r.unpack('QI')
    r.string()
    r.string()
    _, emotes = r.unpack('II')
    r.take(emotes * 8)
    choices = r.unpack('I')
    return npc, quest, choices


def kill_credit(body):
    return struct.unpack('<IIIIQ', body)


def rewarded(body):
    r = Cursor(body)
    quest, status, xp, money, count = r.unpack('5I')
    items = [r.unpack('II') for _ in range(count)]
    r.done()
    return quest, status, xp, money, items


def spell_start(body):
    r = Cursor(body)
    r.guid()  # Cast item or true caster.
    caster = r.guid()
    spell, _, duration = r.unpack('IHI')
    return caster, spell, duration


def spell_go(body):
    r = Cursor(body)
    r.guid()
    return r.guid(), r.unpack('I')


def loot_window(body):
    r = Cursor(body)
    corpse, kind = r.unpack('QB')
    if kind == 0:
        error = r.unpack('B')
        r.done()
        return corpse, error, 0, []
    money, count = r.unpack('IB')
    slots = []
    for _ in range(count):
        slot, item, quantity, _, _, _, slot_kind = r.unpack('B5IB')
        slots.append((slot, item, quantity, slot_kind))
    r.done()
    return corpse, None, money, slots


def picked_up(body):
    guid, received, created, _, _, _, item, _, _, count = struct.unpack('<QIIIBIIIiI', body)
    return guid, received, created, item, count
