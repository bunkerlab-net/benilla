// SPDX-License-Identifier: GPL-3.0-or-later
// Test-only caller-local repair. Lookup bodies and selected storage fragments are original.
struct ColdPair {
    dtPolyRef start, end;
    float startDistance, endDistance;
    std::vector<dtPolyRef> lookupStorage;
};
struct Vector3 { float x,y,z; };
// INSERTED_BEFORE_PATHFINDER

ColdPair PathFinder::compatibility_endpoints(const Vector3& startPos, const Vector3& endPos,
    FILE* trace, const char* label) {
    // REPAIRED_SOURCE_ENDPOINT_PREFIX
    return {startPoly,endPoly,distToStartPoly,distToEndPoly,m_pathPolyRefs};
}

static void cold_case(Loaded& mesh,FILE* out,FILE* trace,const char* label,const char* geometry,
    std::array<float,3> from,std::array<float,3> to) {
    PathFinder path(mesh,storage({}),0);
    const auto pair=path.compatibility_endpoints({from[2],from[0],from[1]},{to[2],to[0],to[1]},trace,label);
    Transition transition{"missing"};
    if(pair.start && pair.end) {
        if(pair.startDistance>7.0f || pair.endDistance>7.0f) transition.phase="far";
        else transition=path.selected_transition(pair.start,pair.end,from.data(),to.data());
    }
    std::fprintf(out,"%s\tcompatibility-zero-v1\t%s\t",label,geometry);point(out,from.data());std::fputc('\t',out);point(out,to.data());
    std::fprintf(out,"\t%llu\t%08x\t%llu\t%08x\t",static_cast<unsigned long long>(pair.start),bits(pair.startDistance),static_cast<unsigned long long>(pair.end),bits(pair.endDistance));
    refs(out,pair.lookupStorage);
    std::fprintf(out,"\t%s\t%08x\t%u\t",transition.phase,transition.status,path.m_polyLength);refs(out,path.m_pathPolyRefs);std::fputc('\n',out);
}
int main(int argc,char** argv) {
    require(argc==3,"usage: cold oracle OUTPUT FIXTURES");
    require(std::fegetround()==FE_TONEAREST && (_mm_getcsr()&0x8040)==0,"unsupported floating environment");
    const std::string output=argv[1],fixtures=argv[2],direct=fixtures+"/direct-corridor";
    FILE* out=std::fopen((output+"/cases.test-fixture").c_str(),"w");require(out,"open cold observations");
    FILE* trace=std::fopen((output+"/traces.test-fixture").c_str(),"w");require(trace,"open cold traces");
    std::fprintf(out,"# Selected zero-distance compatibility evidence; original cold undefined branch NOT invoked or claimed\n# label\toracle\tgeometry\tfrom\tto\tstart_ref\tstart_distance\tend_ref\tend_distance\tlookup_storage\tphase\tdirect_status\tafter_active\tafter_storage\n");
    std::fprintf(trace,"# Selected zero-distance compatibility actual original Detour primitive observations\n# label\tendpoint\tordinal\tprimitive\tpolygon\tstatus\tdistance_bits\n");
    Loaded precision(read(direct+"/params.test-fixture"));precision.add(read(fixtures+"/warm-cache/precision.test-fixture"));
    cold_case(precision,out,trace,"cold_same_tail","precision",{5,0,5},{6,0,5});
    cold_case(precision,out,trace,"cold_replacement","precision",{5,0,5},{15,0,5});
    cold_case(precision,out,trace,"cold_nearest10","precision",{-6,0,5},{15,0,5});
    cold_case(precision,out,trace,"cold_first_miss_second_hit","precision",{-11,0,5},{15,0,5});
    cold_case(precision,out,trace,"cold_first_hit_second_miss","precision",{5,0,5},{-11,0,5});
    cold_case(precision,out,trace,"cold_both_miss","precision",{-11,0,5},{-12,0,5});
    cold_case(precision,out,trace,"cold_far","precision",{-8,0,5},{15,0,5});
    cold_case(precision,out,trace,"cold_partial","precision",{5,0,5},{5,0,25});
    Loaded basic(read(fixtures+"/authored.mmap"));basic.add(read(fixtures+"/0003232.mmtile"));basic.add(read(fixtures+"/0003231.mmtile"));
    cold_case(basic,out,trace,"cold_cross_tile","existing",{-400,0,-400},{400,7,-400});
    Loaded grid(read(direct+"/params.test-fixture"));
    for(const char* name:{"grid00.test-fixture","grid10.test-fixture","grid01.test-fixture","grid11.test-fixture"})grid.add(read(direct+"/"+name));
    cold_case(grid,out,trace,"cold_cross_side_order","grid",{-GRID+GRID*.40f,0,-GRID+GRID*.90f},{-GRID+GRID*1.95f,0,-GRID+GRID*1.05f});
    for(int n:{600,1100}) {
        const auto name="long"+std::to_string(n);Loaded mesh(read(direct+"/params.test-fixture"));mesh.add(read(direct+"/"+name+".test-fixture"));
        const float cs=GRID/float(n);const std::array<float,3> start={-GRID+cs*.5f,0,-GRID+cs*50};
        cold_case(mesh,out,trace,(name+"_cold_rejected").c_str(),name.c_str(),start,{-GRID+cs*(float(n)-.5f),0,start[2]});
        if(n==1100) {
            cold_case(mesh,out,trace,"cold_74_exact","long1100",start,{-GRID+cs*73.5f,0,start[2]});
            cold_case(mesh,out,trace,"cold_75_truncated","long1100",start,{-GRID+cs*74.5f,0,start[2]});
        }
    }
    require(std::fclose(out)==0 && std::fclose(trace)==0,"close cold observations");
}
