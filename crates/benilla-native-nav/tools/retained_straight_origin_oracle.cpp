// SPDX-License-Identifier: GPL-3.0-or-later
// Independent selected cold-policy origin followed by original retained straight calls.
const char* PathFinder::original_retained_straight(const Vector3& startPos,const Vector3& endPos,
    ColdEvidence& evidence) {
    require(m_polyLength>0,"original retained caller requires a nonempty active scan");
    for(uint32 i=0;i<m_polyLength;++i) {
        require(m_navMesh->isValidPolyRef(m_pathPolyRefs[i]),"valid retained active reference");
    }
    // ORIGINAL_RETAINED_ENDPOINT_PREFIX
    evidence.start=startPoly;evidence.end=endPoly;
    evidence.startDistance=distToStartPoly;evidence.endDistance=distToEndPoly;
    evidence.afterLookup=m_pathPolyRefs;
    // ORIGINAL_FAR_PREDICATE
    require(startPoly&&endPoly&&startPoly!=endPoly&&!farFromPoly,
        "selected distinct near retained endpoints required");
    const char* selected_phase=phase(*this,startPoly,endPoly);
    selected_transition(startPoly,endPoly,startPoint,endPoint);
    return selected_phase;
}

static void origin_step(PathFinder& path,Loaded& mesh,FILE* out,FILE* traces,
    const char* sequence,unsigned step,const char* suffix,
    std::array<float,3> from,std::array<float,3> to) {
    const std::string label=std::string(sequence)+"_"+suffix;
    const auto before=path.m_pathPolyRefs;
    const auto active=path.m_polyLength;
    const auto previous=path.m_type;
    const auto nodes=mesh.query.getNodePool()->getMaxNodes();
    require(nodes==1024&&path.m_navMesh==&mesh.mesh&&&path.observer.query==&mesh.query&&
        path.m_navMeshQuery==&path.observer,"same actual source1024 query allocation");
    require(path.m_filter.getIncludeFlags()==13&&path.m_filter.getExcludeFlags()==0&&
        path.m_pointPathLimit==74,"explicit filter13 and original point limit");
    require(step==0?active==0:active>0,"only initial entry may use cold compatibility");
    if(step)require(!admissible(mesh,before,active,world(from),world(to),.125f,
        false,false,unsigned(previous)),"defined retained straight input domain");
    // Calculate's endpoint/flag inputs only. Never assign previous type or seed cache.
    path.setStartPosition(world(from));path.setEndPosition(world(to));
    path.m_straightLine=true;path.m_useStraightPath=false;
    path.m_forceDestination=false;path.m_ignoreNormalization=false;
    sWorld={false,false};path.owner.width=.125f;path.owner.width_calls=0;
    path.observer.reset();
    require(path.m_sourceUnit!=nullptr,"nonnull diagnostic owner required");
    ColdEvidence evidence;
    const char* selected_phase;
    if(step==0) {
        path.compatibility_same_polygon(world(from),world(to),evidence);
        selected_phase=phase(path,evidence.start,evidence.end);
    } else selected_phase=path.original_retained_straight(world(from),world(to),evidence);
    require(std::strcmp(selected_phase,step==0?"same":"replacement")==0,"actual selected source phase");
    require(step==0?!path.observer.has_ray:path.observer.has_ray,
        "cold shortcut followed by genuine Replacement ray");
    require(path.m_pathPoints.size()<=1026,"straight observer point bound");
    require(path.m_navMesh==&mesh.mesh&&&path.observer.query==&mesh.query&&
        path.m_navMeshQuery==&path.observer,"query identity retained after call");
    std::fprintf(out,"%s\t%s\t%u\t%s\t%d\t%u\t%u\t%u\t",label.c_str(),sequence,step,
        step==0?"compatibility-zero-v1":"original-retained-defined",nodes,
        path.m_filter.getIncludeFlags(),path.m_filter.getExcludeFlags(),path.m_pointPathLimit);
    refs(out,evidence.afterStart);
    std::fprintf(out,"\tprecision\tloaded\t");point(out,world(from));std::fputc('\t',out);point(out,world(to));
    std::fprintf(out,"\t%08x\t0\t0\t0\t0\t%u\t%u\t",bits(path.owner.width),unsigned(previous),active);
    refs(out,before);
    std::fprintf(out,"\t%llu\t%08x\t%llu\t%08x\t",
        (unsigned long long)evidence.start,bits(evidence.startDistance),
        (unsigned long long)evidence.end,bits(evidence.endDistance));
    refs(out,evidence.afterLookup);std::fprintf(out,"\t%s\t",selected_phase);
    if(path.observer.has_ray) {
        const auto& q=path.observer;
        std::fprintf(out,"%08x\t%08x\t",q.ray_status,bits(q.hit));point(out,world(q.normal));
        std::fprintf(out,"\t%d\t",q.ray_active);refs(out,q.ray_storage);std::fputc('\t',out);
        if(path.owner.width_calls)std::fprintf(out,"%08x",bits(q.hit-path.owner.width));
        else std::fputc('-',out);
    } else std::fprintf(out,"-\t-\t-\t-\t-\t-");
    std::fprintf(out,"\t%u",path.owner.width_calls);result(out,path);
    std::fprintf(out,"\t%u\t%u\n",path.observer.height_calls,path.observer.height_failures);
    trace(traces,label.c_str(),path.observer);
}

static void origin_sequence(PathFinder& path,Loaded& mesh,FILE* out,FILE* traces,
    const char* sequence,bool hit_first) {
    PathFinder* const live_object=&path;
    ObservedQuery* const live_observer=path.m_navMeshQuery;
    dtNavMeshQuery* const live_query=&path.observer.query;
    origin_step(path,mesh,out,traces,sequence,0,"cold",{5,0,5},{6,0,5});
    require(path.m_type==PATHFIND_NORMAL&&path.m_polyLength==1,"genuine cold NORMAL terminal");
    if(hit_first) {
        origin_step(path,mesh,out,traces,sequence,1,"hit",{5,0,5},{5,0,25});
        require(path.m_type==PATHFIND_INCOMPLETE&&path.observer.has_ray&&
            path.observer.hit!=FLT_MAX&&path.owner.width_calls==1,"genuine hit INCOMPLETE terminal");
        origin_step(path,mesh,out,traces,sequence,2,"no_hit",{5,0,5},{15,0,5});
        require(path.m_type==PATHFIND_INCOMPLETE,"actual previous INCOMPLETE retained on no-hit");
    } else {
        origin_step(path,mesh,out,traces,sequence,1,"no_hit",{5,0,5},{15,0,5});
        require(path.m_type==PATHFIND_NORMAL,"cold-origin no-hit NORMAL terminal");
    }
    require(path.observer.has_ray&&path.observer.hit==FLT_MAX&&path.owner.width_calls==0,
        "real no-hit ray does not read collision width");
    require(&path==live_object&&path.m_navMeshQuery==live_observer&&
        &path.observer.query==live_query&&live_query==&mesh.query,"one live object and query for complete sequence");
}

int main(int argc,char** argv) {
    require(argc==3,"usage: retained straight origin oracle OUTPUT FIXTURES");
    require(std::fegetround()==FE_TONEAREST&&(_mm_getcsr()&0x8040)==0,"unsupported floating environment");
    static_assert(MAX_POINT_PATH_LENGTH==74&&MAX_PATH_LENGTH==74&&VERTEX_SIZE==3);
    require(SMOOTH_PATH_STEP_SIZE==4.0f,"original straight step");
    const std::string output=argv[1],fixtures=argv[2];
    FILE* out=std::fopen((output+"/cases.test-fixture").c_str(),"wx");
    FILE* traces=std::fopen((output+"/traces.test-fixture").c_str(),"wx");
    require(out&&traces,"create new origin outputs without overwriting");
    std::fprintf(out,"# One original PathFinder/query per independent cold-policy-origin sequence; no original cold UB invoked\n"
        "# Explicit filter13/exclude0; straight true; useStraightPath false; disabled normalization; force false\n"
        "# CASE_HEADER\n");
    std::fprintf(traces,"# TRACE_HEADER\n");
    Loaded normal_mesh(read(fixtures+"/direct-corridor/params.test-fixture"),1024);
    Loaded incomplete_mesh(read(fixtures+"/direct-corridor/params.test-fixture"),1024);
    normal_mesh.add(read(fixtures+"/warm-cache/precision.test-fixture"));
    incomplete_mesh.add(read(fixtures+"/warm-cache/precision.test-fixture"));
    ConstructorCache initial;
    require(initial.m_pointPathLimit==74&&initial.m_polyLength==0&&initial.m_pathPolyRefs.size()==74,
        "original constructor cache dimensions");
    for(auto ref:initial.m_pathPolyRefs)require(ref==0,"original constructor zero reference storage");
    PathFinder normal(normal_mesh,initial.m_pathPolyRefs,initial.m_polyLength);
    PathFinder incomplete(incomplete_mesh,initial.m_pathPolyRefs,initial.m_polyLength);
    for(PathFinder* path:{&normal,&incomplete}) {
        require(path->m_pointPathLimit==initial.m_pointPathLimit&&
            path->m_cachedPoints==initial.m_cachedPoints&&path->m_smoothPathPolyRefs==initial.m_smoothPathPolyRefs&&
            path->m_type==PATHFIND_BLANK,"diagnostic storage matches original constructor cache projection");
        path->m_filter.setIncludeFlags(13);path->m_filter.setExcludeFlags(0);
    }
    require(&normal!=&incomplete&&normal.m_navMesh!=incomplete.m_navMesh&&
        &normal.observer.query!=&incomplete.observer.query,"independent simultaneously live allocations");
    origin_sequence(normal,normal_mesh,out,traces,"normal",false);
    origin_sequence(incomplete,incomplete_mesh,out,traces,"incomplete",true);
    require(std::fclose(out)==0&&std::fclose(traces)==0,"close origin outputs");
}
