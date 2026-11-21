// SPDX-License-Identifier: GPL-3.0-or-later
// Authored numeric inputs only. Every result executes complete original methods
// and original MAPS, WDBC, VMap tree/tile/model readers. No query reply is injected.
static constexpr unsigned WET_STORES = 5, WET_PROFILES = 26;
static void writeWetStores(const std::filesystem::path& output, unsigned variant) {
    const auto dir = output / "terrain-wet-liquid" / ("store" + std::to_string(variant));
    std::filesystem::create_directories(dir);
    std::vector<std::array<uint32,25>> areas;
    for (const auto key : std::vector<std::array<uint32,6>>{
        {1000,0,2000,1000,8,30}, {1001,0,2000,1001,8,0},
        {1002,0,0,1002,8,0}, {1003,0,9999,1003,8,0},
        {1004,0,2000,1004,8,777}, {1005,0,2001,1005,8,0},
        {1006,0,1007,1006,8,0}, {1007,0,1006,1007,8,0},
        {1008,0,0,1008,8,32}, {1009,0,0,1009,8,65536},
        {1010,0,2000,1010,8,0}, {1011,0,0,1011,8,35},
        {1012,0,0,1012,8,36}, {2000,7,2002,2000,8,31},
        {2001,0,2000,2001,8,0}, {2002,0,0,2002,8,33},
        // Flag narrowing and exact mixed-WMO tuple resolution remain real DBC lookups.
        {3000,0,0,70000,8,34}, {3001,0,0,4464,8,32},
        {3002,7,0,1000,8,33}, {0,0,0,9000,8,34}})
        areas.push_back(areaWords(key));
    std::reverse(areas.begin(),areas.end());
    writeAreaDbc(dir / "AreaTable.dbc.test-fixture",areas);
    writeAreaDbc(dir / "Map.dbc.test-fixture",std::vector<std::array<uint32,42>>{mapWords(7,1000),mapWords(0,variant == 3 ? 9999 : 1002)});
    std::vector<std::array<uint32,20>> wmos{
        wmoWords(10,0xf1234567,33025,0xe2345678,1000),
        wmoWords(20,0xf1234567,33025,0xe2345678,1010),
        wmoWords(30,0xf1234567,33025,0xe2345678,3002),
        wmoWords(11,0xf1111111,33032,0xe1111111,1001),
        wmoWords(12,0xf2222222,33025,0xe2222222,1004),
        wmoWords(13,0xf1111111,33025,0xe1111111,3000),
        wmoWords(14,0xf2222222,33026,0xe2222222,1000),
        wmoWords(15,9000,5000,10000,1001),
        wmoWords(16,9001,5000,10001,1008),
        wmoWords(17,9000,5001,10000,1004),
    };
    std::reverse(wmos.begin(),wmos.end());
    writeAreaDbc(dir / "WMOAreaTable.dbc.test-fixture",wmos);
    std::vector<std::array<uint32,4>> liquids{{21,719,3,43},{30,727,2,47},{31,733,0,53},
        {32,739,31,59},{33,743,8,61},{34,751,1,67},{35,757,30,71},{36,761,7,73}};
    if (variant != 1 && variant != 3) liquids.push_back({0,701,2,31});
    for (uint32 type = 1; type <= 20; ++type) {
        if (type == 2 || ((variant == 2 || variant == 3) && type == 1)) continue;
        liquids.push_back({type,703+type,type == 1 && variant == 4 ? 32u : type % 4,37+type});
    }
    // A high-ID present row and a separate missing high ID prove full u32 lookup.
    liquids.push_back({65536,769,31,79});
    std::reverse(liquids.begin(),liquids.end());
    writeAreaDbc(dir / "LiquidType.dbc.test-fixture",liquids);
}
static void loadWetStores(const std::filesystem::path& output, unsigned variant) {
    sWMOAreaInfoByTripple.clear(); sAreaIDByAreaFlag.clear(); sAreaFlagByMapID.clear();
    sAreaStore.Clear(); sMapStore.Clear(); sWMOAreaTableStore.Clear(); sLiquidTypeStore.Clear();
    const auto dir = output / "terrain-wet-liquid" / ("store" + std::to_string(variant));
    require(sAreaStore.Load((dir / "AreaTable.dbc.test-fixture").c_str()),"wet AreaTable original WDBC reader");
    require(sMapStore.Load((dir / "Map.dbc.test-fixture").c_str()),"wet Map original WDBC reader");
    require(sWMOAreaTableStore.Load((dir / "WMOAreaTable.dbc.test-fixture").c_str()),"wet WMOAreaTable original WDBC reader");
    require(sLiquidTypeStore.Load((dir / "LiquidType.dbc.test-fixture").c_str()),"wet LiquidType original WDBC reader");
    require(sLiquidTypeStore.GetFieldCount() == 4 && sizeof(LiquidTypeEntry) == DBCFileLoader::GetFormatRecordSize(LiquidTypefmt),"LiquidType original packed ABI");
    // PINNED_WET_BUILD_DBC_INDICES
}
struct WetProfile {
    float ground = 0.f, level = 8.f;
    uint16 area = 1000, entry = 21;
    uint8 flags = 63;
    bool heightMap = false, holes = false, absent = false;
};
static WetProfile wetProfile(unsigned id) {
    WetProfile p;
    if (id >= 9 && id <= 17) p.area = uint16(1000 + id - 8);
    else switch (id) {
        case 0: p.ground=-4; p.level=5; break;
        case 1: break;
        case 2: p.ground=2; break;
        case 3: p.ground=6; break;
        case 4: p.ground=7; break;
        case 5: p.ground=INVALID_HEIGHT_VALUE; break;
        case 6: p.ground=2; p.heightMap=p.holes=true; break;
        case 7: p.ground=2; p.heightMap=true; break;
        case 8: p.ground=2; p.holes=true; break;
        case 18: p.area=0; break;
        case 19: p.area=65535; break;
        case 20: p.absent=true; break;
        case 21: p.entry=1; break;
        case 22: p.entry=1; p.area=1008; break;
        case 23: p.area=1011; break;
        case 24: p.area=1012; break;
        case 25: p.entry=1; p.area=1009; break;
        default: throw std::runtime_error("unknown wet profile");
    }
    return p;
}
static void writeWetMap(const std::filesystem::path& path,unsigned id) {
    const auto p = wetProfile(id);
    const uint32 heightSize=16+(p.heightMap?(129*129+128*128)*4:0), liquidSize=p.absent?0:16;
    GridMapFileHeader head{areaFourcc("MAPS"),areaFourcc("z1.4"),40,8,48,heightSize,
        p.absent?0:48+heightSize,liquidSize,48+heightSize+liquidSize,512};
    GridMapAreaHeader area{areaFourcc("AREA"),MAP_AREA_NO_AREA,p.area};
    GridMapHeightHeader height{areaFourcc("MHGT"),p.heightMap?0u:MAP_HEIGHT_NO_HEIGHT,p.ground,p.ground};
    GridMapLiquidHeader liquid{areaFourcc("MLIQ"),MAP_LIQUID_NO_TYPE|MAP_LIQUID_NO_HEIGHT,
        p.flags,p.entry,0,0,128,128,p.level};
    FILE* f=std::fopen(path.c_str(),"wb"); require(f,"open wet MAPS");
    writeValue(f,head); writeValue(f,area); writeValue(f,height);
    if(p.heightMap) for(unsigned i=0;i<129*129+128*128;++i) writeValue(f,p.ground);
    if(!p.absent) writeValue(f,liquid);
    for(unsigned i=0;i<256;++i) writeValue(f,uint16(p.holes?0xffff:0));
    require(std::fclose(f)==0,"close wet MAPS");
}
static float wetFromBits(uint32 word) { float value; std::memcpy(&value,&word,4); return value; }
static GridMapLiquidData wetSentinel() { return {0xdecafbad,0xdeadbeef,wetFromBits(0x4a123456),wetFromBits(0xca654321)}; }
static bool sameWetData(const GridMapLiquidData& a,const GridMapLiquidData& b) {
    return a.type_flags==b.type_flags && a.entry==b.entry && bits(a.level)==bits(b.level) && bits(a.depth_level)==bits(b.depth_level);
}
struct WetCount {
    size_t rows=0,wet=0,wetInvalid=0,fallback=0,filtered=0,untouched=0,overwritten=0,areaOccluded=0,mixed=0;
    bool statuses[16]{};
} wetCount;
static std::ofstream wetOutput;
static void wetSample(const std::string& dataset,unsigned store,unsigned profile,const std::string& name,
                      const Vector3& world,float collision,uint8 mask,bool wantsData,
                      VMAP::VMapManager2& manager,GridMap& grid) {
    const int gx=int(32-world.x/SIZE_OF_GRIDS),gy=int(32-world.y/SIZE_OF_GRIDS);
    require(gx>=0&&gx<64&&gy>=0&&gy<64,"wet query inside original GetGrid cast domain");
    TerrainInfo terrain{}; terrain.m_mapId=0; terrain.m_vmgr=&manager; terrain.m_GridMaps[gx][gy]=&grid;
    const float initial=terrain.GetHeightStatic(world.x,world.y,world.z,true,DEFAULT_WATER_SEARCH);
    float level=INVALID_HEIGHT_VALUE,floor=initial; uint32 type=0;
    const bool wet=manager.GetLiquidLevel(0,world.x,world.y,world.z,mask,level,floor,type);
    auto raw=wetSentinel(),result=wetSentinel();
    const auto rawStatus=grid.getLiquidStatus(world.x,world.y,world.z,mask,&raw,collision);
    const auto status=terrain.getLiquidStatus(world.x,world.y,world.z,mask,wantsData?&result:nullptr,collision);
    AreaObservation area{false,world.z,INITIAL_FLAGS,INITIAL_ADT,INITIAL_ROOT,INITIAL_GROUP};
    area.hit=manager.getAreaInfo(0,world.x,world.y,area.z,area.flags,area.adt,area.root,area.group);
    uint32 flags=INITIAL_FLAGS; int32 adt=INITIAL_ADT,root=INITIAL_ROOT,group=INITIAL_GROUP;
    const bool accepted=terrain.GetAreaInfo(world.x,world.y,world.z,flags,adt,root,group);
    const uint16 areaFlag=terrain.GetAreaFlag(world.x,world.y,world.z);
    const auto* areaRow=GetAreaEntryByAreaFlagAndMap(areaFlag,0);
    const auto* parent=areaRow&&areaRow->zone?GetAreaEntryByAreaID(areaRow->zone):nullptr;
    VMAP::LocationInfo loc;
    const bool location=manager.iInstanceMapTrees.at(0)->GetLocationInfo(worldPoint(world),loc);
    require(!wantsData || status!=LIQUID_MAP_NO_WATER || sameWetData(result,wetSentinel()),"failed source result preserves caller data");
    require(wantsData || sameWetData(result,wetSentinel()),"status-only leaves caller data untouched");
    require(!wet || (level>floor&&world.z>floor-2) || status==LIQUID_MAP_NO_WATER,"true raw wet invalid level/z suppresses GridMap fallback");
    if(!wet && status!=LIQUID_MAP_NO_WATER) require(!wantsData||sameWetData(raw,result),"false raw VMap fallback preserves raw GridMap depth and type");
    wetOutput<<dataset<<'\t'<<store<<'\t'<<profile<<'\t'<<name<<"\t0";
    for(float v:{world.x,world.y,world.z,collision})putFloat(wetOutput,v);
    wetOutput<<'\t'<<unsigned(mask)<<'\t'<<unsigned(wantsData)<<'\t'<<unsigned(status)<<'\t'<<result.entry<<'\t'<<result.type_flags;
    putFloat(wetOutput,result.level);putFloat(wetOutput,result.depth_level);putFloat(wetOutput,initial);
    wetOutput<<'\t'<<unsigned(wet);putFloat(wetOutput,level);putFloat(wetOutput,floor);
    wetOutput<<'\t'<<type<<'\t'<<unsigned(rawStatus)<<'\t'<<raw.entry<<'\t'<<raw.type_flags;
    putFloat(wetOutput,raw.level);putFloat(wetOutput,raw.depth_level);putFloat(wetOutput,grid.getHeight(world.x,world.y));
    wetOutput<<'\t'<<grid.getArea(world.x,world.y)<<'\t'<<unsigned(area.hit);putFloat(wetOutput,area.z);
    wetOutput<<'\t'<<area.flags<<'\t'<<uint32(area.adt)<<'\t'<<uint32(area.root)<<'\t'<<uint32(area.group)<<'\t'<<unsigned(accepted)<<'\t'<<areaFlag;
    writeResolvedArea(wetOutput,areaRow);writeResolvedArea(wetOutput,parent);
    wetOutput<<'\t'<<unsigned(location)<<'\t'<<(loc.hitInstance?loc.hitInstance->ID:0)<<'\t'<<loc.rootId<<'\t'<<(loc.hitModel?loc.hitModel->GetWmoID():0)<<'\t'<<gx<<'\t'<<gy<<'\n';
    ++wetCount.rows;wetCount.wet+=wet;wetCount.wetInvalid+=wet&&status==0;wetCount.fallback+=!wet&&status!=0;
    wetCount.filtered+=!wet&&rawStatus!=0&&status==0;wetCount.untouched+=sameWetData(result,wetSentinel());
    wetCount.overwritten+=bits(initial)!=bits(floor);wetCount.areaOccluded+=area.hit&&!accepted;
    wetCount.mixed+=area.hit&&location&&(uint32(area.root)!=loc.rootId||uint32(area.group)!=loc.hitModel->GetWmoID());
    wetCount.statuses[status]=true;
}
static void wetDataset(const std::filesystem::path& root,const std::string& dataset,
                       const std::vector<std::pair<std::string,Vector3>>& points) {
    VMAP::VMapManager2 manager;
    VMAP::StaticMapTree loaded(0,(root/dataset).string());
    require(loaded.InitMap("000.vmtree",&manager),"wet original VMap tree reader");
    require(loaded.LoadMapTile(32,32,&manager),"wet original VMap tile/model readers");
    manager.iInstanceMapTrees[0]=&loaded;
    for(unsigned store=0;store<WET_STORES;++store) {
        loadWetStores(root,store);
        for(unsigned profile=0;profile<WET_PROFILES;++profile) {
            if(store>0 && store<4 && profile!=1 && profile!=9 && profile!=10 && profile!=12 && profile!=16 && profile!=17 && profile!=19 && profile!=22)continue;
            // Source Type32 is only executed where an override is proven to
            // replace it before the shift, or data is nullptr (no lookup/shift).
            if(store==4 && profile!=1 && profile!=9 && profile!=16 && profile!=17 && profile!=23 && profile!=24)continue;
            GridMap grid;
            require(grid.loadData((root/"terrain-wet-liquid"/(std::to_string(profile)+".map.test-fixture")).c_str()),"wet original MAPS loader");
            grid.SetFullyLoaded();
            for(const auto& point:points) {
                if(store==4 && point.first!="type_1")continue;
                const auto world=worldPoint(point.second);
                for(uint8 mask:{uint8(0),uint8(1),uint8(8)})
                    wetSample(dataset,store,profile,point.first,world,2.f,mask,true,manager,grid);
                if(store==0 && (profile==0||profile==1||profile==3||profile==16||profile==22)) {
                    float level=INVALID_HEIGHT_VALUE,floor=INVALID_HEIGHT_VALUE;uint32 type=0;
                    const bool hit=manager.GetLiquidLevel(0,world.x,world.y,world.z,0,level,floor,type);
                    if(hit) for(float collision:{0.f,2.f,2.03128f})
                        for(float threshold:{level-collision,level,level+1.f,floor-2.f})
                            for(float z:{std::nextafter(threshold,-INFINITY),threshold,std::nextafter(threshold,INFINITY)}) {
                                Vector3 q=world;q.z=z;
                                wetSample(dataset,store,profile,point.first+"_boundary_"+std::to_string(bits(collision))+"_"+std::to_string(bits(z)),q,collision,0,true,manager,grid);
                            }
                    wetSample(dataset,store,profile,point.first+"_status_only",world,2,0,false,manager,grid);
                }
            }
            if(store<4 && (profile==1||profile==9||profile==16||profile==17))
                for(const auto& point:points) if((store==0&&profile==1&&point.first.rfind("type_",0)==0) || ((profile==9||profile==16||profile==17)&&(point.first=="type_1"||point.first=="type_2")))
                    for(unsigned mask=0;mask<256;++mask)
                        wetSample(dataset,store,profile,point.first+"_mask_"+std::to_string(mask),worldPoint(point.second),2.f,uint8(mask),true,manager,grid);
        }
    }
}
static void writeWetGeometry(const std::filesystem::path& root) {
    const auto dir=root/"terrain-wet-vmap";
    std::filesystem::create_directories(dir);
    std::vector<std::unique_ptr<Geometry>> geometry;
    std::vector<ModelSpawn> spawns;std::vector<WorldModel*> models;
    auto add=[&](uint32 type,float floor,float level,const std::string& name,float x,uint32 rootId,uint32 groupId,uint16 adt) {
        geometry.push_back(std::make_unique<Geometry>());auto& g=*geometry.back();
        addLiquidFloor(g,groupId,floor,0x2008);
        addLiquid(g,name,1,1,{-2,-1,500},type,{level,level,level,level},{0});
        finishVolume(g,0,rootId,dir/(name+".vmo"));
        spawns.push_back(spawn(g,10000+uint32(spawns.size()),name,{x,0,0},{},1.f,false));
        spawns.back().adtId=adt;models.push_back(&g.model);
    };
    for(uint32 type=0;type<=21;++type)add(type,0,5,"type-"+std::to_string(type)+".wmo",float(type*20),100+type,200+type,uint16(4000+type));
    add(65536,0,5,"type-65536.wmo",440,500,600,4022);
    add(0x12345678,0,5,"type-305419896.wmo",460,501,601,4023);
    add(1,6,6,"equal-floor.wmo",480,502,602,4024);
    add(1,6,std::nextafter(6.f,-INFINITY),"below-floor.wmo",500,503,603,4025);
    add(1,6,std::nextafter(6.f,INFINITY),"above-floor.wmo",520,504,604,4026);
    // The original 0.1 model-space ray bias becomes 3.2 world units at
    // scale32, producing a real floor at0 even for z=-2 boundary queries.
    add(1,0,5,"z-boundary-scale.wmo",4000,505,605,4027);
    spawns.back().iScale=32.f;
    require(authoredBound(spawns.back(),geometry.back()->raw),"source bounds for scaled original geometry");
    // Overlapping wet instances: highest location provides liquid and ADT;
    // raw area metadata comes from the last successful lower instance.
    add(1,6,8,"mixed-high.wmo",600,9000,10000,5000);
    add(1,2,4,"mixed-low.wmo",600,9001,10001,5001);
    for(unsigned reverse=0;reverse<2;++reverse) {
        const auto target=reverse?dir/"reverse":dir;
        if(reverse) {
            std::filesystem::create_directories(target);
            for(const auto& file:std::filesystem::directory_iterator(dir))
                if(file.path().extension()==".vmo")std::filesystem::copy_file(file.path(),target/file.path().filename());
        }
        VMAP::StaticMapTree tree(0,"");populate(tree,spawns,models);
        tree.iTree=AreaLeaf(spawns,bool(reverse));serializeTree(target,tree,spawns);
    }
}
static void terrainWetQueries(const std::filesystem::path& root) {
    std::filesystem::create_directories(root/"terrain-wet-liquid");
    for(unsigned i=0;i<WET_STORES;++i)writeWetStores(root,i);
    for(unsigned i=0;i<WET_PROFILES;++i)writeWetMap(root/"terrain-wet-liquid"/(std::to_string(i)+".map.test-fixture"),i);
    writeWetGeometry(root);
    wetOutput.open(root/"terrain-wet-liquid.tsv");
    wetOutput<<"# Complete unchanged TerrainInfo::getLiquidStatus/GetHeightStatic/GridMap/area/DBC lookup source closure; CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f\n"
        "# All inputs reread by original MAPS/WDBC/VMap readers; authored numeric geometry only; no null grids; source undefined shifts/casts excluded\n"
        "# Float columns are hexadecimal binary32. Failed/data-null outputs retain entry deadbeef flags decafbad level 4a123456 depth ca654321\n"
        "# dataset\tstore\tprofile\tcase\tmap_id\tx\ty\tz\tcollision\tmask\twants_data\tstatus\tentry\ttype_flags\tlevel\tdepth\tinitial_static_height\tvmap_wet\tvmap_level\tvmap_floor\tvmap_type\tgrid_status\tgrid_entry\tgrid_flags\tgrid_level\tgrid_depth\tgrid_height\tgrid_area\traw_area_hit\traw_area_floor\traw_area_flags\traw_area_adt\traw_area_root\traw_area_group\tarea_info_hit\tarea_flag\tarea_present\tarea_id\tarea_map\tarea_zone\tarea_explore\tarea_liquid\tparent_present\tparent_id\tparent_map\tparent_zone\tparent_explore\tparent_liquid\tlocation_hit\tlocation_spawn\tlocation_root\tlocation_group\ttile_x\ttile_y\n";
    std::vector<std::pair<std::string,Vector3>> points;
    for(uint32 type=0;type<=21;++type)points.push_back({"type_"+std::to_string(type),{float(type*20),0,4}});
    points.push_back({"type_65536",{440,0,4}});points.push_back({"type_305419896",{460,0,4}});
    points.push_back({"equal_floor",{480,0,7}});points.push_back({"below_floor",{500,0,7}});
    points.push_back({"above_floor",{520,0,7}});points.push_back({"z_boundary_scale",{4000,0,-2}});
    points.push_back({"mixed",{600,0,7}});points.push_back({"gap",{550,0,4}});
    wetDataset(root,"terrain-wet-vmap",points);
    wetDataset(root,"terrain-wet-vmap/reverse",{{"mixed",{600,0,7}}});
    wetDataset(root,"scopedliquid",{
        {"wet_area_parent",{-3,-2,4}},{"wet_triangle_two",{-3,-.1f,4}},
        {"wet_hole",{1,-2,4}},{"wet_below_floor",{8.239584f,-2.739583f,4}},
        {"dry_group",{399.5f,.5f,7}},{"wet_below_dry",{399.5f,.5f,5}},
        {"hole_group",{499.5f,.5f,7}},{"dry_instance",{599.5f,.5f,7}},
        {"m2_only",{699.5f,.5f,7}},{"m2_over_wmo",{797,-2,7}},
        {"rotated",{99,20,7}},{"scaled",{198.5f,4,1}},
        {"tilted",{299,20,7}},{"zero_cells",{2100,0,4}},{"gap",{50,0,4}}});
    wetDataset(root,"scopedarea/forward",{{"wet_wmo",{230,0,7}},{"mixed_dry",{0,0,7}},{"gap",{15,0,7}}});
    wetDataset(root,"scopedarea/reverse",{{"wet_wmo",{230,0,7}},{"mixed_dry",{0,0,7}},{"gap",{15,0,7}}});
    wetOutput.flush();require(bool(wetOutput),"write complete wet TerrainInfo expectations");
    for(unsigned status:{0,1,2,4,8})require(wetCount.statuses[status],"all liquid source statuses exercised");
    require(wetCount.wetInvalid&&wetCount.fallback&&wetCount.filtered&&wetCount.overwritten&&wetCount.areaOccluded&&wetCount.mixed,"all wet source ordering branches exercised");
    std::printf("Complete TerrainInfo wet oracle: %zu rows, %zu raw wet, %zu wet-invalid suppress fallback, %zu fallback, %zu filtered, %zu untouched, %zu floor overwrites, %zu area occlusions, %zu mixed metadata\n",wetCount.rows,wetCount.wet,wetCount.wetInvalid,wetCount.fallback,wetCount.filtered,wetCount.untouched,wetCount.overwritten,wetCount.areaOccluded,wetCount.mixed);
}
int main(int argc,char** argv) {
    try {
        require(argc==2,"usage: terrain-wet-liquid-oracle output-directory");
        require(authored_area_main(argc,argv)==0,"regenerate immutable previous source fixtures");
        terrainWetQueries(argv[1]);return 0;
    } catch(const std::exception& error) {
        std::fprintf(stderr,"Wet TerrainInfo oracle failed: %s\n",error.what());return 1;
    }
}
