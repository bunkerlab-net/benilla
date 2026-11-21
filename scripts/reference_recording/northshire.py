#!/usr/bin/env python3
"""Drive only quests 783 and 7 on the private Northshire recording rig."""

import argparse
import json
import math
from pathlib import Path
import struct
import subprocess
import time

from client import add_fixture_arguments, enter_world, ordinary_logout, private_fixture
from objects import COIN, DYNAMIC_FLAGS, ENTRY, FLAGS, HEALTH, MANA, MAX_HEALTH, MAX_MANA, QUEST_COUNT, QUEST_FIRST, XP
from objects import Cursor, Objects
import route_packets as p


DEPUTY, MARSHAL, VERMIN = 823, 197, 6
INTRO, CLEANUP, REQUIRED = 783, 7, 10
IN_COMBAT = 0x80000
LOOTING = 0x400
UNATTACKABLE = 0x02000002
FIREBALL_MANA, FIREBALL_GCD = 30, 1.5


class RouteStopped(RuntimeError):
    pass


class Northshire:
    def __init__(self):
        self.objects = Objects()
        self.guid = None
        self.spells = None
        self.factions = None
        self.credits = []
        self.loot_received = 0
        self.released = set()
        self.movement_started = False

    def observe(self, opcode, body):
        try:
            self.objects.update(opcode, body)
            r = Cursor(body)
            if opcode == p.QUEST_KILL:
                self.credits.append(p.kill_credit(body))
            elif opcode == p.RELEASED and len(body) == 9 and body[8] == 1:
                self.released.add(r.unpack('Q'))
            elif opcode == p.INITIAL_SPELLS:
                _, count = r.unpack('BH')
                self.spells = {r.unpack('HH')[0] for _ in range(count)}
            elif opcode == p.FACTIONS:
                self.factions = [r.unpack('Bi')[1] for _ in range(r.unpack('I'))]
                r.done()
            elif opcode == p.STANDING and self.factions is not None:
                for _ in range(r.unpack('I')):
                    index, standing = r.unpack('Ii')
                    self.factions[index] = standing
                r.done()
            elif opcode in (0x3e, 0xc5, 0xc7, 0xef) and self.movement_started:
                self.event('movement_refused', opcode=opcode)
                raise RouteStopped(f'Server moved or corrected the client: {opcode:#x}')
        except (ValueError, struct.error) as error:
            raise RouteStopped(f'Cannot decode packet {opcode:#x}: {error}') from error
        player = self.objects.visible.get(self.guid)
        if player is not None and (player.fields.get(HEALTH) == 0 or player.fields.get(190, 0) & 0x10):
            self.event('mage_died', guid=self.guid, pose=getattr(self, 'pose', None))
            raise RouteStopped('Mage died. Preserve this attempt and stop.')

    def run(self, world, event, args):
        self.world, self.event = world, event
        self.started = time.monotonic()
        self.character, self.pose = enter_world(world, mode=args.mode, name=args.name, expected_guid=args.guid, event=event)
        self.guid = self.character.guid
        self.wait_until(lambda: self.guid in self.objects.visible and self.spells is not None and self.factions is not None,
                        'self update, spellbook and reputation')
        if p.FIREBALL not in self.spells:
            raise RouteStopped('The live spellbook does not contain Fireball rank 1')
        event('in_world_start', guid=self.guid, pose=self.pose, run_speed=self.player.run_speed)
        if args.mode == 'create':
            self.navpath, self.mmaps = args.navpath, args.mmaps
            self.spawns = {}
            for line in args.spawns.read_text().splitlines():
                counter, entry, x, y, z = line.split('\t')
                self.spawns.setdefault(int(entry), {})[int(counter)] = tuple(map(float, (x, y, z)))
            if set(self.spawns) != {DEPUTY, MARSHAL, VERMIN} or len(self.spawns[VERMIN]) < REQUIRED:
                raise RouteStopped('Spawn rows do not cover the two quest givers and ten Vermin')
            self.route()
        else:
            expected = json.loads(args.expect_state.read_text())
            actual = self.snapshot()
            expected_pose, actual_pose = expected.pop('pose'), actual.pop('pose')
            if expected != actual or math.dist(expected_pose[:3], actual_pose[:3]) > .025 or abs(
                    math.remainder(expected_pose[3] - actual_pose[3], math.tau)) > .025:
                expected['pose'], actual['pose'] = expected_pose, actual_pose
                self.event('reload_mismatch', expected=expected, actual=actual)
                raise RouteStopped('Live player projection changed on cold reopen')
            self.event('reload_verified', pose=actual_pose, **actual)
        event('in_world_end', guid=self.guid)
        ordinary_logout(world, self.guid, event)

    @property
    def player(self):
        return self.objects.visible[self.guid]

    def snapshot(self):
        fields = self.player.fields
        inventory = []
        # Equipment, bag slots and the base backpack. No bag is equipped by this route.
        for i in range(0x1e6, 0x234, 2):
            guid = fields.get(i, 0) | fields.get(i + 1, 0) << 32
            if guid:
                item = self.objects.visible.get(guid)
                if item is None or ENTRY not in item.fields:
                    raise RouteStopped(f'Inventory item {guid:#x} has no visible item record')
                inventory.append([i, guid, item.fields[ENTRY], item.fields.get(14, 0)])
        return {'guid': self.guid, 'level': fields.get(0x22, self.character.level), 'pose': self.pose,
                'coin': fields.get(COIN, 0), 'xp': fields.get(XP, 0), 'factions': self.factions.copy(),
                'inventory': inventory, 'quest_log': [fields.get(i, 0) for i in range(QUEST_FIRST, QUEST_FIRST + 3 * QUEST_COUNT)]}

    def wait_until(self, predicate, reason, timeout=30):
        end = time.monotonic() + timeout
        while not predicate():
            if time.monotonic() >= end:
                raise RouteStopped(f'Timed out waiting for {reason}')
            self.world.receive(end)

    def pump(self, duration):
        end = time.monotonic() + max(0, duration)
        while time.monotonic() < end:
            try:
                self.world.receive(end)
            except TimeoutError:
                break

    def packet(self, opcodes, *, match=lambda _opcode, _body: True, timeout=22):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            opcode, body = self.world.receive(end)
            if opcode in (p.QUEST_INVALID, p.QUEST_FAILED, p.QUEST_LOG_FULL, p.PICKUP_ERROR):
                raise RouteStopped(f'Server refused route input {opcode:#x}: {body.hex()}')
            if opcode in opcodes and match(opcode, body):
                return opcode, body
        raise RouteStopped(f'No matching {tuple(hex(x) for x in opcodes)} packet')

    def move_packet(self, opcode, flags=0):
        self.movement_started = True
        ms = round((time.monotonic() - self.started) * 1000)
        self.world.send(opcode, p.movement_body(flags, ms, self.pose))

    def face(self, pos):
        x, y, z, before = self.pose
        angle = math.atan2(pos[1] - y, pos[0] - x) % math.tau
        if abs(math.remainder(angle - before, math.tau)) > 0.08:
            self.pose = [x, y, z, angle]
            self.move_packet(p.MOVE_FACING)

    def approach(self, target, within):
        start = tuple(self.pose[:3])
        if math.dist(start, target) <= within:
            return
        result = subprocess.run([str(self.navpath), str(self.mmaps), *map(str, start), *map(str, target)],
                                capture_output=True, text=True, check=True, timeout=5)
        path = [tuple(map(float, line.split())) for line in result.stdout.splitlines()]
        if not path or math.dist(start, path[0]) > 2 or math.dist(target, path[-1]) > 3:
            raise RouteStopped('Ground path does not connect to the live positions')
        speed = self.player.run_speed
        if speed is None or not 2 <= speed <= 10:
            raise RouteStopped(f'Unexpected unmounted run speed {speed}')
        self.event('movement_path', pose=self.pose, destination=target, within=within, ground_points=path, run_speed=speed)
        moving = False
        for point in path[1:]:
            start = tuple(self.pose[:3])
            distance = math.dist(start, point)
            if distance < 0.28:
                continue
            horizontal = math.hypot(point[0] - start[0], point[1] - start[1])
            if horizontal < 0.1 and distance > 1.2:
                raise RouteStopped('Ground corridor requires a vertical jump')
            angle = math.atan2(point[1] - start[1], point[0] - start[0]) % math.tau if horizontal > 0.1 else self.pose[3]
            self.pose[3] = angle
            if not moving:
                self.move_packet(p.MOVE_START, 1)
                moving = True
            elapsed = 0.0
            while elapsed < distance:
                dt = min(0.25, (distance - elapsed) / speed)
                before = time.monotonic()
                self.pump(dt)
                elapsed = min(distance, elapsed + min(0.26, time.monotonic() - before) * speed)
                t = elapsed / distance
                self.pose = [*(s + t * (e - s) for s, e in zip(start, point)), angle]
                if math.dist(self.pose[:3], target) <= within:
                    self.move_packet(p.MOVE_STOP)
                    return
                self.move_packet(p.MOVE_HEARTBEAT, 1)
        if moving:
            self.move_packet(p.MOVE_STOP)
        if math.dist(self.pose[:3], target) > within:
            raise RouteStopped('Ground corridor ended outside the required interaction radius')

    def find(self, entry, *, alive, exclude=()):
        registered = self.spawns[entry]
        visible = [obj for obj in self.objects.visible.values() if obj.kind == 3 and obj.fields.get(ENTRY) == entry
                   and obj.guid & 0xffffff in registered and obj.guid not in exclude and obj.position is not None
                   and (not alive or obj.fields.get(HEALTH, 0) > 0 and not obj.fields.get(FLAGS, 0) & UNATTACKABLE)]
        return sorted(visible, key=lambda obj: (math.dist(self.pose[:3], obj.position), obj.guid))

    def questgiver(self, entry):
        if not self.find(entry, alive=True):
            self.approach(next(iter(self.spawns[entry].values())), 4)
        self.wait_until(lambda: self.find(entry, alive=True), f'visible quest giver {entry}')
        npc = self.find(entry, alive=True)[0]
        self.approach(npc.position, 3.5)
        self.face(npc.position)
        self.pump(0.4)
        return npc

    def dialog(self, npc, quest):
        self.world.send(p.HELLO, p.guid_body(npc.guid))
        opcode, body = self.packet((p.GOSSIP, p.QUEST_LIST, p.DETAILS, p.OFFER_REWARD, p.REQUEST_ITEMS),
                                   match=lambda _, body: body[:8] == p.guid_body(npc.guid))
        if opcode in (p.GOSSIP, p.QUEST_LIST):
            if quest not in dict(p.quest_offers(opcode, body)[1]):
                raise RouteStopped(f'Quest {quest} was not presented by {npc.fields[ENTRY]}')
        elif struct.unpack_from('<I', body, 8)[0] != quest:
            raise RouteStopped(f'Unexpected quest in dialog with {npc.fields[ENTRY]}')
        return opcode, body

    def quest_log(self):
        return {self.player.fields.get(i): self.player.fields.get(i + 1, 0)
                for i in range(QUEST_FIRST, QUEST_FIRST + QUEST_COUNT * 3, 3) if self.player.fields.get(i)}

    def accept(self, npc, quest, dialog=None):
        opcode, _ = dialog if dialog is not None else self.dialog(npc, quest)
        if opcode != p.DETAILS:
            self.world.send(p.QUERY_QUEST, p.quest_body(npc.guid, quest))
            self.packet((p.DETAILS,), match=lambda _, body: body[:12] == p.quest_body(npc.guid, quest))
        self.world.send(p.ACCEPT, p.quest_body(npc.guid, quest))
        self.wait_until(lambda: quest in self.quest_log(), f'quest {quest} in live quest log')
        self.event('quest_accepted', quest=quest, npc=npc.guid, initial_counter=self.quest_log()[quest] & 63)

    def turn_in(self, npc, quest):
        opcode, body = self.dialog(npc, quest)
        if opcode != p.OFFER_REWARD:
            self.world.send(p.REQUEST_REWARD, p.quest_body(npc.guid, quest))
            _, body = self.packet((p.OFFER_REWARD,), match=lambda _, body: body[:12] == p.quest_body(npc.guid, quest))
        if p.reward_offer(body) != (npc.guid, quest, 0):
            raise RouteStopped('Unexpected quest reward choices')
        self.world.send(p.CHOOSE_REWARD, p.quest_body(npc.guid, quest, 0))
        _, body = self.packet((p.REWARDED,), match=lambda _, body: struct.unpack_from('<I', body)[0] == quest)
        quest, status, xp, money, items = p.rewarded(body)
        if status != 3:
            raise RouteStopped('Quest reward packet was not successful')
        self.event('quest_rewarded', quest=quest, npc=npc.guid, xp=xp, money=money, items=items)
        self.wait_until(lambda: quest not in self.quest_log(), f'rewarded quest {quest} removed from log')

    def recover(self):
        f = self.player.fields
        self.wait_until(lambda: not f.get(FLAGS, 0) & IN_COMBAT and f.get(HEALTH, 0) >= f[MAX_HEALTH] * .94
                        and f.get(MANA, 0) >= min(4 * FIREBALL_MANA, f[MAX_MANA]), 'natural health and mana recovery', 140)

    def vermin(self, killed):
        if not self.find(VERMIN, alive=True, exclude=killed):
            unused = [pos for counter, pos in self.spawns[VERMIN].items() if all(guid & 0xffffff != counter for guid in killed)]
            nearest = min(unused, key=lambda pos: math.dist(self.pose[:3], pos))
            self.approach(nearest, 24)
        self.wait_until(lambda: self.find(VERMIN, alive=True, exclude=killed), 'visible live Kobold Vermin')
        return self.find(VERMIN, alive=True, exclude=killed)[0]

    def fireball(self, target, credit_before):
        f = self.player.fields
        if f.get(MANA, 0) < FIREBALL_MANA:
            self.wait_until(lambda: f.get(MANA, 0) >= FIREBALL_MANA or len(self.credits) > credit_before,
                            'mana or in-flight kill credit', 60)
        if len(self.credits) > credit_before:
            return
        start = time.monotonic()
        self.face(target.position)
        self.world.send(p.CAST, p.cast_body(p.FIREBALL, target.guid))
        started = False
        deadline = start + 12
        while time.monotonic() < deadline and len(self.credits) == credit_before:
            op, body = self.world.receive(deadline)
            if op == p.CAST_RESULT:
                spell, status = struct.unpack_from('<IB', body)
                if spell == p.FIREBALL and status != 0:
                    reason = body[5]
                    self.event('cast_refused', spell=spell, target=target.guid, reason=reason)
                    if reason == 0x23:  # Ordinary interruption; let live combat state settle.
                        return
                    raise RouteStopped(f'Fireball refused by the server: {reason:#x}')
            elif op == p.SPELL_START:
                caster, spell, duration = p.spell_start(body)
                if (caster, spell) == (self.guid, p.FIREBALL):
                    started = True
                    self.event('cast_started', spell=spell, target=target.guid, duration_ms=duration)
                    if duration <= 0:
                        raise RouteStopped('Fireball reported a nonpositive cast time')
            elif op == p.SPELL_GO and p.spell_go(body) == (self.guid, p.FIREBALL):
                if not started:
                    raise RouteStopped('Fireball released without a live cast start')
                self.event('cast_released', spell=p.FIREBALL, target=target.guid)
                # The next cast starts only after release and the pinned spell's GCD.
                self.pump(max(0.1, start + FIREBALL_GCD - time.monotonic()))
                return
        if len(self.credits) == credit_before:
            raise RouteStopped('No release or kill credit for the submitted Fireball')

    def kill(self, target, count):
        self.approach(target.position, 27)
        if target.guid not in self.objects.visible or not target.fields.get(HEALTH, 0):
            raise RouteStopped('Selected target is no longer live and visible')
        self.world.send(p.SELECTION, p.guid_body(target.guid))
        self.event('target_selected', entry=VERMIN, guid=target.guid, generation=target.generation, position=target.position)
        before = len(self.credits)
        for _ in range(10):
            if len(self.credits) != before or not target.fields.get(HEALTH, 0):
                break
            self.fireball(target, before)
        self.wait_until(lambda: len(self.credits) > before, f'kill credit for {target.guid:#x}', 12)
        if self.credits[before:] != [(CLEANUP, VERMIN, count, REQUIRED, target.guid)]:
            raise RouteStopped(f'Unexpected quest credit: {self.credits[before:]}')
        self.event('kill_credit', quest=CLEANUP, entry=VERMIN, count=count, required=REQUIRED,
                   guid=target.guid, generation=target.generation)
        self.pump(0.35)

    def loot(self, target):
        if target.guid not in self.objects.visible or not target.fields.get(DYNAMIC_FLAGS, 0) & 1:
            self.event('corpse_loot', guid=target.guid, generation=target.generation, outcome='no lootable corpse')
            return
        self.approach(target.position, 2)
        self.face(target.position)
        # Loot and movement run from different session queues.
        self.pump(0.4)
        self.world.send(p.LOOT, p.guid_body(target.guid))
        _, body = self.packet((p.LOOT_RESPONSE,), match=lambda _, body: body[:8] == p.guid_body(target.guid))
        corpse, error, money, slots = p.loot_window(body)
        if error is not None:
            raise RouteStopped(f'Corpse loot refused: {error}')
        self.wait_until(lambda: self.player.fields.get(FLAGS, 0) & LOOTING, 'live loot window to open')
        taken = []
        if money:
            before = self.player.fields.get(COIN, 0)
            self.world.send(p.LOOT_MONEY)
            self.packet((p.LOOT_CLEAR_MONEY,))
            self.wait_until(lambda: self.player.fields.get(COIN, 0) == before + money, 'loot money in coin field')
            self.loot_received += 1
        for slot, item, count, kind in slots:
            if kind != 0:
                raise RouteStopped(f'Unexpected solo loot slot permissions {kind}')
            self.world.send(p.TAKE_ITEM, bytes([slot]))
            self.packet((p.ITEM_PUSH,), match=lambda _, body: p.picked_up(body) == (self.guid, 0, 0, item, count))
            taken.append((item, count))
            self.loot_received += 1
        if corpse not in self.released and self.player.fields.get(FLAGS, 0) & LOOTING:
            self.world.send(p.RELEASE, p.guid_body(corpse))
            self.wait_until(lambda: corpse in self.released or not self.player.fields.get(FLAGS, 0) & LOOTING,
                            'server corpse loot closure')
        self.event('corpse_loot', guid=corpse, generation=target.generation, money=money, items=taken,
                   outcome='transfer' if money or taken else 'empty',
                   closure='release packet' if corpse in self.released else 'looting flag cleared')

    def route(self):
        self.accept(self.questgiver(DEPUTY), INTRO)
        marshal = self.questgiver(MARSHAL)
        self.turn_in(marshal, INTRO)
        self.accept(marshal, CLEANUP)
        if self.quest_log().get(CLEANUP) != 0:
            raise RouteStopped('Cleanup did not start at zero credit')
        killed = set()
        for count in range(1, REQUIRED + 1):
            self.recover()
            target = self.vermin(killed)
            self.kill(target, count)
            killed.add(target.guid)
            self.loot(target)
        if not self.loot_received:
            raise RouteStopped('No positive loot transfer among the ten credited kills')
        self.turn_in(self.questgiver(MARSHAL), CLEANUP)
        self.pump(0.6)
        self.event('final_projection', **self.snapshot())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('create', 'reconnect'))
    add_fixture_arguments(parser)
    parser.add_argument('--spawns', type=Path)
    parser.add_argument('--navpath', type=Path)
    parser.add_argument('--mmaps', type=Path)
    parser.add_argument('--expect-state', type=Path)
    args = parser.parse_args()
    if args.mode == 'create' and not all((args.spawns, args.navpath, args.mmaps)):
        parser.error('Create requires bound spawn rows and the map-0 path program and products')
    if args.mode == 'reconnect' and args.expect_state is None:
        parser.error('Reconnect requires the saved live projection from the route')
    route = Northshire()
    with private_fixture(parser, args, on_packet=route.observe) as (world, event):
        try:
            route.run(world, event, args)
        except (RouteStopped, TimeoutError, EOFError, subprocess.SubprocessError) as error:
            event('route_stopped', reason=str(error))
            raise


if __name__ == '__main__':
    main()
