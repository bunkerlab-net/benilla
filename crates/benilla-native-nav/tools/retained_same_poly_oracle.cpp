// SPDX-License-Identifier: GPL-3.0-or-later
// One live object: selected repaired cold entry followed by original defined warm entries.
void PathFinder::original_retained_same_polygon(const Vector3& startPos,const Vector3& endPos,
    ColdEvidence& evidence) {
    require(m_polyLength>0,"retained prefix requires a nonempty active scan");
    for(uint32 i=0;i<m_polyLength;++i) {
        require(m_navMesh->isValidPolyRef(m_pathPolyRefs[i]),"valid retained active reference");
    }
    // ORIGINAL_RETAINED_ENDPOINT_PREFIX
    evidence.start=startPoly;evidence.end=endPoly;
    evidence.startDistance=distToStartPoly;evidence.endDistance=distToEndPoly;
    evidence.afterLookup=m_pathPolyRefs;
    // ORIGINAL_FAR_PREDICATE
    require(startPoly&&endPoly&&startPoly==endPoly&&!farFromPoly,
        "only valid near retained same-polygon endpoints admitted");
    // ORIGINAL_SAME_POLYGON_TERMINAL
    fail("retained same-polygon original return not taken");
}

static ColdEvidence retained_same_step(PathFinder& path,FILE* out,unsigned step,const char* label,
    std::array<float,3> from,std::array<float,3> to,bool normalize_z,bool ignore_normalization,
    const char* sequence=nullptr) {
    require(!normalize_z&&!ignore_normalization,"only explicit false/false normalization admitted");
    require(path.m_straightLine,"explicit straight-line scalar input required");
    const auto nodes=path.m_navMeshQuery->getNodePool()->getMaxNodes();
    require(nodes==1024,"source MoveMap query capacity required");
    const auto before=path.m_pathPolyRefs;
    const auto active=path.m_polyLength;
    require(step==0?active==0:active>0,"only initial step may enter the repaired cold prefix");
    configure(path,world(from),world(to),false,normalize_z,ignore_normalization,false,path.m_type);
    require(path.m_sourceUnit!=nullptr,"normalization must not bypass through null owner");
    ColdEvidence evidence;
    if(step==0)path.compatibility_same_polygon(world(from),world(to),evidence);
    else path.original_retained_same_polygon(world(from),world(to),evidence);
    if(out) {
    const auto row_label=sequence?std::string(sequence)+"_"+label:std::string(label);
    std::fprintf(out,"%s\t%s\t%u\t%s\tprecision\t",row_label.c_str(),sequence?sequence:"retained_main",step,
        step==0?"compatibility-zero-v1":"original-retained-defined");
    point(out,world(from));std::fputc('\t',out);point(out,world(to));
    std::fprintf(out,"\t%u\t%u\t%d",unsigned(normalize_z),unsigned(ignore_normalization),nodes);
    if(sequence)std::fprintf(out,"\t%u\t%u\t%u",path.m_filter.getIncludeFlags(),
        path.m_filter.getExcludeFlags(),path.m_pointPathLimit);
    std::fprintf(out,"\t%u\t",active);
    refs(out,before);
    std::fprintf(out,"\t%llu\t%08x\t",static_cast<unsigned long long>(evidence.start),bits(evidence.startDistance));
    refs(out,evidence.afterStart);
    std::fprintf(out,"\t%llu\t%08x\t",static_cast<unsigned long long>(evidence.end),bits(evidence.endDistance));
    refs(out,evidence.afterLookup);result(out,path);
    }
    require(path.m_polyLength==1&&path.m_type==PATHFIND_NORMAL,"successful original same-polygon terminal");
    require(path.m_straightLine,"same-polygon terminal retains straight-line scalar input");
    return evidence;
}

static bool same_point(const Vector3& a,const Vector3& b) {
    return bits(a.x)==bits(b.x)&&bits(a.y)==bits(b.y)&&bits(a.z)==bits(b.z);
}

static void same_terminal(const PathFinder& a,const PathFinder& b) {
    require(a.m_navMesh!=b.m_navMesh&&a.m_navMeshQuery!=b.m_navMeshQuery,
        "comparison uses separate simultaneously live native allocations");
    require(a.m_pathPolyRefs==b.m_pathPolyRefs&&a.m_polyLength==b.m_polyLength&&
        a.m_pointPathLimit==b.m_pointPathLimit&&a.m_filter.getIncludeFlags()==b.m_filter.getIncludeFlags()&&
        a.m_filter.getExcludeFlags()==b.m_filter.getExcludeFlags(),"complete lookup dependency state matches");
    require(a.m_type==b.m_type&&a.m_straightLine==b.m_straightLine&&a.m_useStraightPath==b.m_useStraightPath&&
        a.m_forceDestination==b.m_forceDestination&&a.m_ignoreNormalization==b.m_ignoreNormalization&&
        same_point(a.m_startPosition,b.m_startPosition)&&same_point(a.m_endPosition,b.m_endPosition)&&
        same_point(a.m_actualEndPosition,b.m_actualEndPosition)&&a.m_pathPoints.size()==b.m_pathPoints.size(),
        "full supported same-polygon terminal semantics match");
    for(size_t i=0;i<a.m_pathPoints.size();++i)
        require(same_point(a.m_pathPoints[i],b.m_pathPoints[i]),"exact binary32 terminal points match");
    require(a.m_cachedPoints==b.m_cachedPoints&&a.m_smoothPathPolyRefs==b.m_smoothPathPolyRefs,
        "unreached path scratch state remains identical");
}

static void same_lookup(const ColdEvidence& a,const ColdEvidence& b) {
    require(a.start==b.start&&a.end==b.end&&bits(a.startDistance)==bits(b.startDistance)&&
        bits(a.endDistance)==bits(b.endDistance)&&a.afterStart==b.afterStart&&a.afterLookup==b.afterLookup,
        "exact source lookup observations match");
}

static void fixed_cache_comparison(FILE* out,const std::string& fixtures,unsigned filter) {
    Loaded full_mesh(read(fixtures+"/direct-corridor/params.test-fixture"),1024);
    Loaded replay_mesh(read(fixtures+"/direct-corridor/params.test-fixture"),1024);
    full_mesh.add(read(fixtures+"/warm-cache/precision.test-fixture"));
    replay_mesh.add(read(fixtures+"/warm-cache/precision.test-fixture"));
    ConstructorCache initial;
    PathFinder full(full_mesh,initial.m_pathPolyRefs,initial.m_polyLength,initial.m_pointPathLimit);
    PathFinder replay(replay_mesh,initial.m_pathPolyRefs,initial.m_polyLength,initial.m_pointPathLimit);
    full.m_straightLine=replay.m_straightLine=true;
    full.m_filter.setIncludeFlags(filter);replay.m_filter.setIncludeFlags(filter);
    const auto full_name="fixed_full_"+std::to_string(filter),replay_name="fixed_replay_"+std::to_string(filter);
    const auto full_cold=retained_same_step(full,out,0,"cold",{5,0,5},{6,0,5},false,false,full_name.c_str());
    const auto replay_cold=retained_same_step(replay,out,0,"cold",{5,0,5},{6,0,5},false,false,replay_name.c_str());
    same_lookup(full_cold,replay_cold);same_terminal(full,replay);
    const auto fixed=full.m_pathPolyRefs;
    const std::array<std::array<float,3>,4> starts={{{6,0,5},{5,-2,5},{5,-3,5},{-6,0,5}}};
    const std::array<std::array<float,3>,4> ends={{{7,.75f,5},{6,-2,5},{6,-3,5},{5,0,5}}};
    const char* labels[]={"active","tail","nearest5","nearest10"};
    for(unsigned step=1;step<4096;++step) {
        const unsigned index=(step-1)%4;
        require(full.m_pathPolyRefs==fixed&&full.m_polyLength==1,"every omitted entry starts at fixed cache");
        retained_same_step(full,step<=4?out:nullptr,step,labels[index],starts[index],ends[index],false,false,
            full_name.c_str());
        require(full.m_pathPolyRefs==fixed&&full.m_polyLength==1&&full.m_pointPathLimit==74&&
            full.m_filter.getIncludeFlags()==filter&&full.m_filter.getExcludeFlags()==0,
            "every omitted entry leaves the full lookup dependency state unchanged");
    }
    auto a=retained_same_step(full,out,4096,"head",{6,0,5},{7,.75f,5},false,false,full_name.c_str());
    auto b=retained_same_step(replay,out,1,"head",{6,0,5},{7,.75f,5},false,false,replay_name.c_str());
    require(full.m_pathPolyRefs==fixed&&replay.m_pathPolyRefs==fixed,"4097 entries reduce to cold plus final");
    same_lookup(a,b);same_terminal(full,replay);
    const std::array<std::array<float,3>,3> next_starts={{{15,0,5},{5,0,5},{10,0,5}}};
    const std::array<std::array<float,3>,3> next_ends={{{16,.75f,5},{6,0,5},{10,0,6}}};
    const char* next_labels[]={"append","return","edge"};
    for(unsigned i=0;i<3;++i) {
        a=retained_same_step(full,out,4097+i,next_labels[i],next_starts[i],next_ends[i],false,false,full_name.c_str());
        b=retained_same_step(replay,out,2+i,next_labels[i],next_starts[i],next_ends[i],false,false,replay_name.c_str());
        same_lookup(a,b);same_terminal(full,replay);
    }
    require(full.m_pathPolyRefs.size()==fixed.size()+1&&full.m_pathPolyRefs[0]==fixed[0],
        "continuation appends B then returns to prior A without append");
}

static void active_dependency_counterexample(FILE* out,const std::string& fixtures,unsigned filter) {
    Loaded full_mesh(read(fixtures+"/direct-corridor/params.test-fixture"),1024);
    Loaded dropped_mesh(read(fixtures+"/direct-corridor/params.test-fixture"),1024);
    full_mesh.add(read(fixtures+"/warm-cache/precision.test-fixture"));
    dropped_mesh.add(read(fixtures+"/warm-cache/precision.test-fixture"));
    ConstructorCache initial;
    PathFinder full(full_mesh,initial.m_pathPolyRefs,initial.m_polyLength,initial.m_pointPathLimit);
    PathFinder dropped(dropped_mesh,initial.m_pathPolyRefs,initial.m_polyLength,initial.m_pointPathLimit);
    full.m_straightLine=dropped.m_straightLine=true;
    full.m_filter.setIncludeFlags(filter);dropped.m_filter.setIncludeFlags(filter);
    const auto full_name="active_full_"+std::to_string(filter),dropped_name="active_dropped_"+std::to_string(filter);
    retained_same_step(full,out,0,"cold",{5,0,5},{6,0,5},false,false,full_name.c_str());
    retained_same_step(dropped,out,0,"cold",{5,0,5},{6,0,5},false,false,dropped_name.c_str());
    retained_same_step(full,out,1,"append",{15,0,5},{16,0,5},false,false,full_name.c_str());
    retained_same_step(dropped,out,1,"append",{15,0,5},{16,0,5},false,false,dropped_name.c_str());
    same_terminal(full,dropped);
    const auto before=full.m_pathPolyRefs;
    const auto returning=retained_same_step(full,out,2,"return",{5,0,5},{6,0,5},false,false,full_name.c_str());
    require(returning.afterLookup==before&&full.m_pathPolyRefs.size()==before.size()&&
        full.m_pathPolyRefs[0]!=before[0],"no-append return changes active slot zero");
    const auto a=retained_same_step(full,out,3,"edge",{10,0,5},{10,0,6},false,false,full_name.c_str());
    const auto b=retained_same_step(dropped,out,2,"edge",{10,0,5},{10,0,6},false,false,dropped_name.c_str());
    require(a.start!=b.start&&a.end!=b.end&&a.startDistance==0&&a.endDistance==0&&
        b.startDistance==0&&b.endDistance==0&&full.m_pathPolyRefs[0]!=dropped.m_pathPolyRefs[0],
        "dropping no-append return changes later source active-first edge lookup");
    require(full.m_pathPolyRefs.size()==dropped.m_pathPolyRefs.size(),"equal backing sizes in negative vector");
    for(size_t i=1;i<full.m_pathPolyRefs.size();++i)
        require(full.m_pathPolyRefs[i]==dropped.m_pathPolyRefs[i],"equal complete inactive storage and ordered tails");
}

int main(int argc,char** argv) {
    require(argc==3,"usage: retained same-polygon oracle OUTPUT FIXTURES");
    require(std::fegetround()==FE_TONEAREST&&(_mm_getcsr()&0x8040)==0,"unsupported floating environment");
    static_assert(MAX_POINT_PATH_LENGTH==74&&MAX_PATH_LENGTH==74&&VERTEX_SIZE==3);
    const std::string output=argv[1],fixtures=argv[2];
    FILE* out=std::fopen((output+"/cases.test-fixture").c_str(),"w");require(out,"open retained same-polygon output");
    std::fprintf(out,"# One live PathFinder and source1024 query: selected +0 cold entry then original defined retained entries\n"
        "# Retained prefix keeps original float locals; nonempty valid active scan initializes both before use\n"
        "# Unchanged original dtMeshTile raw-storage lifetime caveat remains; empirical fixture, not a lifetime proof\n"
        "# label\tsequence\tstep\toracle\tgeometry\tfrom\tto\tnormalize_z\tignore_normalization\tquery_nodes\tbefore_active\tbefore_storage\tstart_ref\tstart_distance\tafter_start_storage\tend_ref\tend_distance\tlookup_storage\tafter_active\tafter_storage\tpath_type\tactual_end\tpoints\n");
    Loaded mesh(read(fixtures+"/direct-corridor/params.test-fixture"),1024);
    mesh.add(read(fixtures+"/warm-cache/precision.test-fixture"));
    const dtPolyRef A=mesh.mesh.getPolyRefBase(static_cast<const dtNavMesh&>(mesh.mesh).getTile(0)),B=A+1;
    ConstructorCache initial;
    require(initial.m_pointPathLimit==74&&initial.m_polyLength==0&&initial.m_pathPolyRefs.size()==74,
        "original constructor cache dimensions");
    for(auto ref:initial.m_pathPolyRefs)require(ref==0,"original constructor zero cache");
    PathFinder path(mesh,initial.m_pathPolyRefs,initial.m_polyLength,initial.m_pointPathLimit);
    // Explicit scalar input only; the complete Calculate path is not executed.
    path.m_straightLine=true;
    dtNavMeshQuery* const live_query=path.m_navMeshQuery;
    auto observed=retained_same_step(path,out,0,"cold_initial",{5,0,5},{6,0,5},false,false);
    require(observed.start==A&&path.m_pathPolyRefs.size()==75,"cold appends original A tail");
    retained_same_step(path,out,1,"retained_active_vertical",{6,0,5},{7,.75f,5},false,false);
    observed=retained_same_step(path,out,2,"retained_tail_squared4",{5,-2,5},{6,-2,5},false,false);
    require(observed.startDistance==2&&observed.endDistance==2&&path.m_pathPolyRefs.size()==75,
        "squared distance four exceeds active three but satisfies tail seven");
    observed=retained_same_step(path,out,3,"retained_nearest_equal",{5,-3,5},{6,-3,5},false,false);
    require(observed.startDistance==3&&observed.endDistance==3&&path.m_pathPolyRefs.size()==75,
        "equal nearest distance does not append");
    observed=retained_same_step(path,out,4,"retained_new_polygon",{15,0,5},{16,.75f,5},false,false);
    require(observed.start==B&&observed.end==B&&observed.afterStart.size()==76&&
        observed.afterLookup==observed.afterStart&&path.m_pathPolyRefs[74]==A&&path.m_pathPolyRefs[75]==B,
        "start appends new B and end reuses B tail on the same object");
    observed=retained_same_step(path,out,5,"retained_new_tail_squared4",{15,-2,5},{16,-2,5},false,false);
    require(observed.start==B&&observed.startDistance==2&&path.m_pathPolyRefs.size()==76,
        "new B tail reused after original shortcut");
    observed=retained_same_step(path,out,6,"retained_return_prior",{5,0,5},{6,0,5},false,false);
    require(observed.start==A&&path.m_pathPolyRefs[0]==A&&path.m_pathPolyRefs.size()==76,
        "prior A tail survives B shortcut and is reused");
    retained_same_step(path,out,7,"retained_identical",{5,0,5},{5,0,5},false,false);
    // Authored coordinates exactly hit the existing guarded precision geometry's
    // squared-distance boundary; float words are input points, never answers.
    const std::array<float,3> at3={-1.f,std::bit_cast<float>(uint32(0x3f7ffff8)),std::bit_cast<float>(uint32(0xbf800004))};
    const std::array<float,3> at7={-2.f,std::bit_cast<float>(uint32(0x3f7ffff8)),std::bit_cast<float>(uint32(0xbfb504f5))};
    float closest[3];
    require(mesh.query.closestPointOnPoly(A,at3.data(),closest,nullptr)==DT_SUCCESS&&dtVdistSqr(at3.data(),closest)==3,
        "authored active equality point");
    require(mesh.query.closestPointOnPoly(A,at7.data(),closest,nullptr)==DT_SUCCESS&&dtVdistSqr(at7.data(),closest)==7,
        "authored tail equality point");
    observed=retained_same_step(path,out,8,"retained_active_squared3_equal",at3,at3,false,false);
    require(observed.start==A&&path.m_pathPolyRefs.size()==76,"active equality remains accepted");
    observed=retained_same_step(path,out,9,"retained_tail_squared7_equal",at7,at7,false,false);
    require(observed.start==A&&path.m_pathPolyRefs.size()==76,"tail equality remains accepted");
    observed=retained_same_step(path,out,10,"retained_nearest10_equal",{-6,0,5},{5,0,5},false,false);
    require(observed.start==A&&observed.startDistance==6&&path.m_pathPolyRefs.size()==76,
        "larger nearest search retains equal-distance storage");
    require(path.m_navMeshQuery==live_query&&live_query==&mesh.query,"same live query throughout sequence");
    require(std::fclose(out)==0,"close retained same-polygon output");
    out=std::fopen((output+"/comparisons.test-fixture").c_str(),"w");require(out,"open retained comparison output");
    std::fprintf(out,"# Independent live allocations: 4097 fixed-cache entries versus two, then append/return/edge\n"
        "# Active counterexample: A B A edge versus A B edge; exact original retained source, dry filters 1 and 13\n"
        "# Initial compatibility-zero-v1 only; original cold undefined branch NOT invoked; raw-storage lifetime caveat remains\n"
        "# label\tsequence\tstep\toracle\tgeometry\tfrom\tto\tnormalize_z\tignore_normalization\tquery_nodes\tinclude_filter\texclude_filter\tpoint_limit\tbefore_active\tbefore_storage\tstart_ref\tstart_distance\tafter_start_storage\tend_ref\tend_distance\tlookup_storage\tafter_active\tafter_storage\tpath_type\tactual_end\tpoints\n");
    for(unsigned filter:{1u,13u}) {
        fixed_cache_comparison(out,fixtures,filter);
        active_dependency_counterexample(out,fixtures,filter);
    }
    require(std::fclose(out)==0,"close retained comparison output");
}
