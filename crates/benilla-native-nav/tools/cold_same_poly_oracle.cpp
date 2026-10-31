// SPDX-License-Identifier: GPL-3.0-or-later
// Cache-only projection of the original constructor, with its declaration order.
// Owner initialization, config defaults and the default-query pointer are not run.
struct ConstructorCache {
    // ORIGINAL_CONSTRUCTOR_CACHE_MEMBERS
    ConstructorCache() :
    // ORIGINAL_CONSTRUCTOR_CACHE_INITIALIZERS
    {}
};
struct ColdEvidence {
    dtPolyRef start=0,end=0;
    float startDistance=0,endDistance=0;
    std::vector<dtPolyRef> afterStart,afterLookup;
};
// INSERTED_BEFORE_PATHFINDER

void PathFinder::compatibility_same_polygon(const Vector3& startPos,const Vector3& endPos,
    ColdEvidence& evidence) {
    // REPAIRED_SOURCE_ENDPOINT_PREFIX
    evidence.start=startPoly;evidence.end=endPoly;
    evidence.startDistance=distToStartPoly;evidence.endDistance=distToEndPoly;
    evidence.afterLookup=m_pathPolyRefs;
    // ORIGINAL_FAR_PREDICATE
    require(startPoly&&endPoly&&startPoly==endPoly&&!farFromPoly,
        "only valid near same-polygon endpoints admitted");
    // ORIGINAL_SAME_POLYGON_TERMINAL
    fail("same-polygon original return not taken");
}

static void cold_same_case(Loaded& mesh,FILE* out,const char* label,
    std::array<float,3> from,std::array<float,3> to,bool normalize_z,bool ignore_normalization) {
    require(!normalize_z&&!ignore_normalization,"only explicit false/false normalization admitted");
    const auto nodes=mesh.query.getNodePool()->getMaxNodes();
    require(nodes==1024,"source MoveMap query capacity required");
    ConstructorCache initial;
    require(initial.m_pointPathLimit==74&&initial.m_polyLength==0&&initial.m_pathPolyRefs.size()==74,
        "original constructor cache dimensions");
    for(auto ref:initial.m_pathPolyRefs)require(ref==0,"original constructor zero cache");
    PathFinder path(mesh,initial.m_pathPolyRefs,initial.m_polyLength,initial.m_pointPathLimit);
    configure(path,world(from),world(to),false,normalize_z,ignore_normalization,false,PATHFIND_BLANK);
    require(path.m_sourceUnit!=nullptr,"normalization must not bypass through null owner");
    ColdEvidence evidence;
    path.compatibility_same_polygon(world(from),world(to),evidence);
    std::fprintf(out,"%s\tcompatibility-zero-v1\tprecision\t",label);
    point(out,world(from));std::fputc('\t',out);point(out,world(to));
    std::fprintf(out,"\t%u\t%u\t%d\t%u\t",unsigned(normalize_z),unsigned(ignore_normalization),nodes,initial.m_polyLength);
    refs(out,initial.m_pathPolyRefs);
    std::fprintf(out,"\t%llu\t%08x\t",static_cast<unsigned long long>(evidence.start),bits(evidence.startDistance));
    refs(out,evidence.afterStart);
    std::fprintf(out,"\t%llu\t%08x\t",static_cast<unsigned long long>(evidence.end),bits(evidence.endDistance));
    refs(out,evidence.afterLookup);result(out,path);
}

int main(int argc,char** argv) {
    require(argc==3,"usage: cold same-polygon oracle OUTPUT FIXTURES");
    require(std::fegetround()==FE_TONEAREST&&(_mm_getcsr()&0x8040)==0,"unsupported floating environment");
    static_assert(MAX_POINT_PATH_LENGTH==74&&MAX_PATH_LENGTH==74&&VERTEX_SIZE==3);
    const std::string output=argv[1],fixtures=argv[2];
    FILE* out=std::fopen((output+"/cases.test-fixture").c_str(),"w");require(out,"open cold same-polygon output");
    std::fprintf(out,"# Selected +0 compatibility then exact same-polygon terminal; original cold undefined branch NOT invoked\n"
        "# Constructor cache initializer projection only; source1024; normalize_z=false and ignore_normalization=false are explicit inputs\n"
        "# Unchanged original dtMeshTile raw-storage lifetime caveat remains; empirical fixture, not a lifetime proof\n"
        "# label\toracle\tgeometry\tfrom\tto\tnormalize_z\tignore_normalization\tquery_nodes\tconstructor_active\tconstructor_storage\tstart_ref\tstart_distance\tafter_start_storage\tend_ref\tend_distance\tlookup_storage\tafter_active\tafter_storage\tpath_type\tactual_end\tpoints\n");
    Loaded mesh(read(fixtures+"/direct-corridor/params.test-fixture"),1024);
    mesh.add(read(fixtures+"/warm-cache/precision.test-fixture"));
    cold_same_case(mesh,out,"cold_same_tail",{5,0,5},{6,0,5},false,false);
    cold_same_case(mesh,out,"cold_same_vertical",{5,0,5},{6,.75f,5},false,false);
    cold_same_case(mesh,out,"cold_same_identical",{5,0,5},{5,0,5},false,false);
    cold_same_case(mesh,out,"cold_same_nearest10",{-6,0,5},{5,0,5},false,false);
    require(std::fclose(out)==0,"close cold same-polygon output");
}
