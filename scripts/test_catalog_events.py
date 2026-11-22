"""Authored event/weather input cases, imported by the CI exporter suite."""
import copy
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import types
import unittest
from classic_export import catalog_events as e
from classic_export.catalog_io import encode_row
from classic_export.core import ExportError, canonical


class EventCatalogTests(unittest.TestCase):
    def setUp(self):
        self.columns = {
            'game_event':['entry','schedule_type','occurence','length','holiday','linkedto','description'],
            'game_event_time':['entry','start_time','end_time'],
            'game_event_creature':['guid','event'], 'game_event_gameobject':['guid','event'],
            'game_event_creature_data':['guid','entry_id','modelid','equipment_id','spell_start','spell_end','event'],
            'game_event_quest':['quest','event'],
            'game_event_mail':['event','racemask','quest','mailtemplateid','senderentry'],
            'game_weather':['zone']+[s+'_'+w+'_chance' for s in ('spring','summer','fall','winter') for w in ('rain','snow','storm')],
            'creature':['guid'], 'gameobject':['guid'], 'creature_template':['entry'],
            'creature_equip_template':['entry'], 'quest_template':['entry'], 'spell_template':['id']}
        self.rows = {t:[] for t in self.columns}
        for t in ('creature','gameobject','creature_template','creature_equip_template','quest_template','spell_template'):
            self.rows[t] = [[b'7']]
        self.schema = {'tables':{t:{'columns':[{'COLUMN_NAME':c,'DATA_TYPE':'int'} for c in cols]} for t,cols in self.columns.items()}}
        self.blob = b'WDBC'+struct.pack('<IIII',1,10,40,1)+struct.pack('<I',7)+bytes(37)
        def scan(table):
            for ordinal,cells in enumerate(self.rows[table]):
                yield types.SimpleNamespace(ordinal=ordinal,cells=lambda c=cells:c)
        def obj(descriptor):
            return {'schema':canonical(self.schema),'dbcs':canonical({'MailTemplate.dbc':'mail'}),'mail':self.blob}[descriptor]
        self.catalog = types.SimpleNamespace(scan=scan, manifest={'schema_root':'schema','dbc_root':'dbcs','catalog_sha256':'a'*64},
            object=obj,_json=json.loads,relations={t:{'rows':len(r)} for t,r in self.rows.items()})
        self.views = e.EventViews(self.catalog)
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.streams = self.root/'streams'
        self.snapshot = self.root/'initial.sql'
        self.env = self.root/'environment.json'
        self.snapshot.write_bytes(b'CREATE TABLE `game_event_status` (\n  `event` smallint(6) unsigned NOT NULL DEFAULT 0,\n  PRIMARY KEY (`event`)\n);\nLOCK TABLES `game_event_status` WRITE;\n/*!40000 ALTER TABLE `game_event_status` DISABLE KEYS */;\n/*!40000 ALTER TABLE `game_event_status` ENABLE KEYS */;\nUNLOCK TABLES;\n')
        self.environment = {'environment':{'TZ':'UTC','LC_ALL':'C'},'tzdata_utc':{'path':'/usr/share/zoneinfo/Etc/UTC','bytes':114,'sha256':'8b85846791ab2c8a5463c83a5be3c043e2570d7448434d41398969ed47e3e6f2'}}
        self.env.write_bytes(canonical(self.environment))

    def put(self,table,**values):
        self.rows[table].append([None if values.get(c)==None and c in values else str(values.get(c,0)).encode() for c in self.columns[table]])
        self.catalog.relations[table]['rows'] = len(self.rows[table])

    def event(self,entry=1,**values):
        self.put('game_event',entry=entry,schedule_type=values.get('schedule_type',1),occurence=values.get('occurence',10),length=values.get('length',5),linkedto=values.get('linkedto',0))

    def capture(self):
        def query(sql):
            spec = next(s for s in e.specs().values() if s['query']==sql)
            return iter(reversed(list(self.views.expected(spec))))
        e.capture_occurrences(types.SimpleNamespace(query_rows=query),self.streams)

    def report(self):
        self.capture()
        return self.views.report(self.views.verify_occurrences(self.streams),e.initial_state(self.snapshot,self.env),self.root/'records')

    def records(self,table):
        return [json.loads(line) for line in (self.root/'records'/(table+'.jsonl')).read_bytes().splitlines()]

    def test_positive_negative_memberships_and_null_extended_missing_spawn(self):
        self.event()
        self.put('game_event_creature',guid=7,event=1)
        self.put('game_event_creature',guid=7,event=-1)
        self.put('game_event_gameobject',guid=99,event=1)
        self.assertEqual(list(self.views.expected(e.specs()['4'])),[[None,b'99',b'1']])
        result = self.report()
        a,b = self.records('game_event_creature')
        self.assertEqual([a['values']['event'],b['values']['event']],[1,-1])
        self.assertEqual(b['values']['active_when'],'event_inactive')
        self.assertFalse(self.records('game_event_gameobject')[0]['accepted'])
        self.assertFalse(result['complete'])

    def test_linked_calendar_and_ignored_calendar(self):
        self.event(linkedto=2)
        self.event(2,schedule_type=0)
        self.event(3,linkedto=99)
        self.event(4,occurence=0)
        self.put('game_event_time',entry=2,start_time='2020-01-01 00:00:00',end_time='2030-01-01 00:00:00')
        self.report()
        self.assertEqual(self.records('game_event')[0]['values']['linkedto'],2)
        self.assertEqual(self.records('game_event')[2]['diagnostics'],['invalid_link_cleared'])
        self.assertEqual(self.records('game_event')[3]['values']['occurence'],5)
        self.assertEqual(self.records('game_event')[3]['diagnostics'],['disabled_occurrence'])
        self.assertEqual(self.records('game_event_time')[0]['diagnostics'],['calendar_ignored'])

    def test_substitutions_clear_missing_template_equipment_and_spells(self):
        self.event()
        for value in (7,99):
            self.put('game_event_creature_data',guid=7,event=1,entry_id=value,equipment_id=value,spell_start=value,spell_end=value,modelid=42)
        self.report()
        a,b = self.records('game_event_creature_data')
        self.assertEqual(a['diagnostics'],[])
        self.assertEqual(b['values']['modelid'],42)
        self.assertEqual([b['values'][c] for c in ('entry_id','equipment_id','spell_start','spell_end')],[0]*4)
        self.assertEqual(len(b['diagnostics']),4)

    def test_mail_unsigned_event_quest_sender_mask_and_dbc_links(self):
        self.event()
        self.put('game_event_quest',quest=99,event=1)
        self.put('game_event_mail',event=-1,racemask=256,quest=99,mailtemplateid=99,senderentry=99)
        self.put('game_event_mail',event=1,racemask=1,quest=7,mailtemplateid=7,senderentry=7)
        self.report()
        bad,good = self.records('game_event_mail')
        self.assertEqual(bad['values']['event'],65535)
        self.assertEqual(len(bad['diagnostics']),5)
        self.assertTrue(good['accepted'])
        self.assertEqual(self.records('game_event_quest')[0]['diagnostics'],['missing_quest'])

    def test_empty_persisted_state_is_evidence_not_computed_status(self):
        state = e.initial_state(self.snapshot,self.env)
        self.assertEqual(state['events'],[])
        self.assertEqual(state['status_rows'],0)
        self.assertEqual(state['runtime_status'],'not_evaluated')
        self.assertEqual(state['snapshot']['sha256'],hashlib.sha256(self.snapshot.read_bytes()).hexdigest())
        self.assertFalse(state['calendar']['runtime_clock_coverage'])

    def test_nonempty_status_preserves_values_and_rejects_duplicates(self):
        raw = self.snapshot.read_bytes()
        marker = b'/*!40000 ALTER TABLE `game_event_status` ENABLE KEYS */;'
        self.snapshot.write_bytes(raw.replace(marker,b'INSERT INTO `game_event_status` VALUES (7),(2);\n'+marker))
        self.assertEqual(e.initial_state(self.snapshot,self.env)['events'],[7,2])
        self.snapshot.write_bytes(raw.replace(marker,b'INSERT INTO `game_event_status` VALUES (7),(7);\n'+marker))
        with self.assertRaisesRegex(ExportError,'events.initial_state'):
            e.initial_state(self.snapshot,self.env)

    def test_missing_state_schema_and_bad_timezone_tzdata_refuse(self):
        raw = self.snapshot.read_bytes()
        for bad in (b'',raw.replace(b'UNLOCK TABLES;',b''),raw.replace(b'CREATE TABLE',b'NO TABLE')):
            self.snapshot.write_bytes(bad)
            with self.assertRaisesRegex(ExportError,'events.initial_state'):
                e.initial_state(self.snapshot,self.env)
        self.snapshot.write_bytes(raw)
        for key,value in (('TZ','Europe/London'),('LC_ALL','en_US')):
            bad = copy.deepcopy(self.environment)
            bad['environment'][key] = value
            self.env.write_bytes(canonical(bad))
            with self.assertRaisesRegex(ExportError,'events.calendar'):
                e.initial_state(self.snapshot,self.env)
        bad = copy.deepcopy(self.environment)
        bad['tzdata_utc']['sha256'] = '0'*64
        self.env.write_bytes(canonical(bad))
        with self.assertRaisesRegex(ExportError,'events.calendar'):
            e.initial_state(self.snapshot,self.env)

    def test_weather_seasons_reset_each_chance_not_total(self):
        self.put('game_weather',zone=1,spring_rain_chance=101,summer_snow_chance=100,fall_storm_chance=99,winter_rain_chance=80,winter_snow_chance=80)
        result = self.report()
        days = result['weather_calendar']['season_by_yday']
        self.assertEqual(len(days),366)
        self.assertEqual([days[d] for d in (0,76,77,167,168,258,259,349,350,365)],[3,3,0,0,1,1,2,2,3,3])
        record = self.records('game_weather')[0]
        self.assertEqual(record['values']['seasons'],[[25,0,0],[0,100,0],[0,0,99],[80,80,0]])
        self.assertEqual(record['diagnostics'],['spring_rain_chance_reset'])

    def test_empty_event_max_null_and_over_4096_occurrences(self):
        self.assertEqual(list(self.views.expected(e.specs()['0'])),[[None]])
        self.event()
        for _ in range(4101):
            self.put('game_event_creature',guid=7,event=1)
        result = self.report()
        self.assertEqual(result['loaders']['3']['rows'],4101)

    def rewrite(self,raw,rows=1):
        path = self.streams/'3.rows'
        path.write_bytes(raw)
        meta = json.loads((self.streams/'occurrences.json').read_bytes())
        meta['streams']['3'].update(rows=rows,bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
        (self.streams/'occurrences.json').write_bytes(canonical(meta))

    def test_rehashed_forgery_trailing_exhaustion_and_bad_ordinal_refuse(self):
        self.event()
        self.put('game_event_creature',guid=7,event=-1)
        self.capture()
        raw = (self.streams/'3.rows').read_bytes()
        for bad,rows,error in ((b'',0,'events.exhausted'),(raw+b'x',1,'events.trailing'),
            (encode_row(0,[b'7',b'7',b'1']),1,'events.view_mismatch'),
            (encode_row(2,[b'7',b'7',b'-1']),1,'catalog.duplicate_row')):
            self.rewrite(bad,rows)
            with self.assertRaises(ExportError) as caught:
                self.views.verify_occurrences(self.streams)
            self.assertEqual(str(caught.exception),error)

    def test_order_identity_separate_from_join_membership(self):
        self.event()
        for event in (1,-1): self.put('game_event_creature',guid=7,event=event)
        self.capture()
        a = self.views.verify_occurrences(self.streams)['3']
        self.rewrite(b''.join(encode_row(i,[b'7',b'7',str(event).encode()]) for i,event in enumerate((1,-1))),2)
        b = self.views.verify_occurrences(self.streams)['3']
        self.assertEqual(a['multiset_sha256'],b['multiset_sha256'])
        self.assertNotEqual(a['occurrence_sha256'],b['occurrence_sha256'])

    def test_limits_and_wrong_dbc_layout_refuse(self):
        self.catalog.relations['creature']['rows'] = 2000000
        with self.assertRaisesRegex(ExportError,'events.view_limit'):
            self.views.plans(self.root)
        self.catalog.relations['creature']['rows'] = 1
        self.blob = b'WDBC'+bytes(20)
        with self.assertRaisesRegex(ExportError,'events.mail_dbc'):
            self.views.plans(self.root)

    def test_duplicate_join_occurrences_and_missing_event_are_retained(self):
        self.rows['creature'].append([b'7'])
        self.catalog.relations['creature']['rows'] = 2
        self.put('game_event_creature',guid=7,event=99)
        result = self.report()
        self.assertEqual(result['loaders']['3']['rows'],2)
        self.assertEqual(self.records('game_event_creature')[0]['diagnostics'],['missing_event'])

    def test_query_identity_unknown_stream_and_capture_quota_refuse(self):
        from unittest.mock import patch
        self.capture()
        path = self.streams/'occurrences.json'
        original = path.read_bytes()
        meta = json.loads(original)
        meta['streams']['0']['query'] = 'SELECT COUNT(entry) FROM game_event'
        path.write_bytes(canonical(meta))
        with self.assertRaisesRegex(ExportError,'events.query'):
            self.views.verify_occurrences(self.streams)
        meta = json.loads(original)
        meta['streams']['unknown'] = copy.deepcopy(meta['streams']['0'])
        path.write_bytes(canonical(meta))
        with self.assertRaisesRegex(ExportError,'events.occurrences'):
            self.views.verify_occurrences(self.streams)
        target = self.root/'refused-capture'
        with patch.object(e,'MAX_RAW',1):
            with self.assertRaisesRegex(ExportError,'events.byte_limit'):
                e.capture_occurrences(types.SimpleNamespace(query_rows=lambda _:[[None]]),target)
        self.assertFalse((target/'occurrences.json').exists())
