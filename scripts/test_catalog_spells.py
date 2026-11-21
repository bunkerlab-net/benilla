"""Authored global spell/condition cases, imported by the exporter CI entrypoint."""
import tempfile
from pathlib import Path
import unittest
from classic_export import catalog_spells as s
from classic_export.core import ExportError


class SpellCatalogTests(unittest.TestCase):
    def chain(self, spell, prev=0, first=None, rank=1, req=0):
        return {"spell_id":spell,"prev_spell":prev,"first_spell":spell if first is None else first,"rank":rank,"req_spell":req}

    def test_conversion_bits_preserve_source_prefix_null_and_signed_zero(self):
        self.assertEqual(s.convert(None,"u32"),0)
        self.assertEqual(s.convert(b" -1tail","u32"),0xffffffff)
        self.assertEqual(s.convert(b"-1","u8"),255)
        self.assertEqual(s.convert(b"65537","u16"),1)
        self.assertEqual(s.convert(b"-0","f32"),0x80000000)
        self.assertEqual(s.convert(b"1.0000000596046448","f32"),0x3f800000)
        self.assertEqual(s.convert(b"-1","u64"),0xffffffffffffffff)
        self.assertEqual(s.convert(b"18446744073709551615","u64"),0xffffffffffffffff)
        self.assertEqual(s.convert(b"a\0ignored","text"),"61")

    def test_source_boolean_is_positive_not_nonzero(self):
        self.assertEqual([s.convert(v,"bool") for v in (None,b"-1",b"0",b"2")],[0,0,0,1])
        self.assertIsNone(s.convert(b"255","unassigned"))
        self.assertIsNone(s.convert(b"comment","ignored"))

    def test_unknown_float_encoding_refuses_instead_of_normalizing(self):
        for raw in (b"nan",b"inf",b"0x1p1"):
            with self.assertRaisesRegex(ExportError,"spawn.numeric_unsupported"):
                s.convert(raw,"f32")

    def test_talent_fallback_survives_whole_empty_sql(self):
        w=[0]*21
        w[4:7]=[10,11,12]
        result=s.chains({10,11,12},{4:w},{},[])
        self.assertEqual(result[0][12],[11,10,3,0])
        self.assertEqual(result[0],result[1])
        self.assertEqual(result[2],[])

    def test_ability_fallback_and_herbalism_exception(self):
        a=[0]*15; a[2]=2366
        b=[0]*15; b[2]=2368
        nodes=s.chains({2366,2368},{},{1:a,2:b},[])[0]
        self.assertEqual(nodes[2368],[2366,2366,2,0])

    def test_malformed_raw_chain_rows_are_source_skips(self):
        rows=[self.chain(99),self.chain(11,prev=99,first=10,rank=2),
              self.chain(11,first=10,rank=2),self.chain(10),self.chain(11,10,10,2)]
        nodes,_,disp,_,_=s.chains({10,11},{},{},list(enumerate(rows)))
        self.assertEqual([r[1] for r in disp],["skip:missing_spell","skip:missing_previous","skip:incompatible_chain","loaded","loaded"])
        self.assertEqual(nodes[11],[10,10,2,0])

    def test_talent_conflict_and_sql_requirement_update(self):
        w=[0]*21; w[4:6]=[10,11]
        rows=[self.chain(11,0,10,2),self.chain(11,10,10,2,99)]
        nodes,_,disp,_,_=s.chains({10,11},{1:w},{},list(enumerate(rows)))
        self.assertEqual(disp,[[0,"skip:dbc_chain_conflict"],[1,"loaded:req_update"]])
        # Existing DBC nodes update req before checking SQL-only required references.
        self.assertEqual(nodes[11][3],99)

    def test_ability_cycles_refuse(self):
        a=[0]*15; a[2]=10; a[8]=11
        b=[0]*15; b[2]=11; b[8]=10
        with self.assertRaisesRegex(ExportError,"spells.chain_cycle"):
            s.chains({10,11},{},{1:a,2:b},[])

    def test_proc_custom_rank_and_fallback_preserve_origin(self):
        nodes={10:[0,10,1,0],11:[10,10,2,0],12:[11,10,3,0]}
        rows=[(0,{"entry":10,"ppmrate":0}),(1,{"entry":11,"ppmrate":0}),(2,{"entry":12,"ppmrate":0x3f800000})]
        loaded,origins,disp=s.rank_inputs("spell_proc_event",rows,{10,11,12},nodes)
        self.assertEqual(origins,{10:10,12:12,11:10})
        self.assertEqual(loaded[11],rows[0][1])
        self.assertEqual(disp[1],[1,"skip:nonfirst_rank"])

    def test_selected_empty_is_not_whole_empty(self):
        rows=[(0,{"entry":10,"threat":1})]
        loaded,_,_=s.rank_inputs("spell_threat",rows,{10,11},{})
        self.assertNotIn(11,loaded)
        self.assertTrue(rows)
        self.assertEqual(s.rank_inputs("spell_threat",[],{10,11},{})[0],{})

    def test_whole_empty_chain_does_not_build_next_cache(self):
        nodes={10:[0,10,1,0],11:[10,10,2,0]}
        loaded,_,_=s.rank_inputs("spell_proc_event",[(0,{"entry":10,"ppmrate":0})],{10,11},nodes,False)
        self.assertEqual(set(loaded),{10})

    def test_repeated_rank_paths_are_not_cycles_and_negative_zero_is_zero(self):
        nodes={10:[0,10,1,0],11:[10,10,2,10]}
        loaded,_,disp=s.rank_inputs("spell_proc_event",[(0,{"entry":10,"ppmrate":0}),
            (1,{"entry":11,"ppmrate":0x80000000})],{10,11},nodes)
        self.assertEqual(set(loaded),{10,11})
        self.assertEqual(disp[1],[1,"skip:nonfirst_rank"])

    def test_missing_spells_and_bad_affect_indices_skip(self):
        self.assertEqual(s.SpellViews.basic_disposition("spell_facing",{"entry":99},{},{}),"skip:missing_spell")
        self.assertEqual(s.SpellViews.basic_disposition("spell_affect",{"entry":10,"effectid":255},{10:b""},{}),"skip:effect_index")

    def test_every_family_field_and_dependency_is_explicit(self):
        self.assertEqual(len(s.TABLES),21)
        self.assertEqual(len(s.POLICY["tables"]["spell_template"]["fields"]),159)
        self.assertEqual(s.POLICY["tables"]["unit_condition"]["fields"][1],["flags","unassigned"])
        self.assertNotIn("Spell.dbc",s.DBC_FILES)
        self.assertIn("spells.registration_and_handlers_unverified",s.REFUSALS)

    def test_record_limits_refuse_without_clipping(self):
        with tempfile.TemporaryDirectory() as directory:
            stream=s.Stream(Path(directory),"authored")
            with self.assertRaisesRegex(ExportError,"spells.record_limit"):
                stream.add("x"*65536)
            self.assertEqual(stream.finish()["rows"],0)

    def test_bounded_view_refuses_excessive_retention(self):
        with self.assertRaisesRegex(ExportError,"spells.view_limit"):
            s.bounded(b"x"*s.DECODED)

    def test_lookup_state_distinguishes_selected_absence_from_whole_empty(self):
        import types
        views=s.SpellViews.__new__(s.SpellViews)
        views.catalog=types.SimpleNamespace(relations={"spell_chain":{"rows":1}},
            lookup=lambda table,key:iter([0] if key == b"present" else []))
        self.assertEqual(views.lookup_state("spell_chain",b"missing"),
            {"whole_empty":False,"selected_empty":True,"rows":0})
        self.assertEqual(views.lookup_state("spell_chain",b"present"),
            {"whole_empty":False,"selected_empty":False,"rows":1})
        views.catalog.relations["spell_chain"]["rows"]=0
        self.assertEqual(views.lookup_state("spell_chain",b"missing"),
            {"whole_empty":True,"selected_empty":True,"rows":0})
        with self.assertRaisesRegex(ExportError,"spells.unknown_relation"):
            views.lookup_state("not_a_spell_table",b"missing")

    def test_condition_source_skips_and_unit_clamps_keep_raw_inputs(self):
        import types
        views=s.SpellViews.__new__(s.SpellViews)
        views.catalog=types.SimpleNamespace(scan=lambda table:iter([]))
        views.schema={t:{"columns":[{"COLUMN_NAME":"entry"}]} for t in
            ("item_template","quest_template","creature_template","game_event")}
        aux={t:[] for t in s.CONDITIONS}
        def condition(entry,kind,a=0,b=0):
            return {"condition_entry":entry,"type":kind&0xffffffff,"value1":a,"value2":b,
                    "value3":0,"value4":0,"flags":0,"comments":None}
        aux["conditions"]=list(enumerate([condition(1,16),condition(2,1,99),condition(3,-3,1),
            condition(4,35,3),condition(5,0),condition(6,7,1,1),condition(7,-3,6)]))
        unit={"id":1,"flags":None}
        for i in range(8):
            unit.update({f"variable_{i}":87,f"op_{i}":7,f"value_{i}":0xffffffff})
        aux["unit_condition"]=[(0,unit)]
        records=[]
        result=views.condition_inputs(aux,{},{"AreaTable.dbc":set(),"Faction.dbc":set()},{},
            types.SimpleNamespace(add=records.append))
        self.assertEqual(list(result["conditions"].values()),["skip:unknown_condition_type",
            "skip:aura","skip:condition_reference","skip:gender","loaded_input",
            "retained:world_config_required","retained:condition_dependency_unverified"])
        self.assertEqual(result["unit_condition"][0],"blocked:flags_unassigned_at_pin")
        self.assertIn(["unit_condition",1,[[0,0,-1]]*8],records)
        self.assertEqual(unit["variable_0"],87)

    def test_uint64_overflow_and_numeric_limit_refuse(self):
        with self.assertRaisesRegex(ExportError,"spells.uint64_overflow"):
            s.convert(b"18446744073709551616","u64")
        with self.assertRaisesRegex(ExportError,"spells.numeric_limit"):
            s.convert(b"1"*128,"u32")

    def test_target_position_validation_preserves_signed_zero(self):
        row={"id":10,"target_map":0,"target_position_x":0x80000000,
            "target_position_y":0,"target_position_z":0,"target_orientation":0}
        words=[0]*len(s.SPELL_FIELDS)
        spells={10:s.PACK.pack(*words)}
        ids={"Map.dbc":{0}}
        self.assertEqual(s.SpellViews.basic_disposition("spell_target_position",row,spells,{},ids),"skip:target_coordinates")
        row["target_position_x"]=0x3f800000
        self.assertEqual(s.SpellViews.basic_disposition("spell_target_position",row,spells,{},ids),"skip:database_target")
        words[s.SPELL_FIELDS.index("effectimplicittargeta1")]=17
        spells[10]=s.PACK.pack(*words)
        self.assertEqual(s.SpellViews.basic_disposition("spell_target_position",row,spells,{},ids),"loaded_input")
        row["target_map"]=99
        self.assertEqual(s.SpellViews.basic_disposition("spell_target_position",row,spells,{},ids),"skip:target_map")

    def test_pet_additional_rows_follow_source_validation_branch(self):
        w=[0]*len(s.SPELL_FIELDS); w[1]=3
        spells={10:s.PACK.pack(*w),11:s.PACK.pack(*([0]*len(w)))}
        rows=list(enumerate([{"spell":10,"pet":1,"aura":99},{"spell":10,"pet":2,"aura":11},
            {"spell":10,"pet":3,"aura":99},{"spell":11,"pet":4,"aura":10}]))
        self.assertEqual(list(s.pet_inputs(rows,spells).values()),["skip:missing_aura",
            "loaded_input","loaded_input:additional_pet","skip:pet_dummy"])
