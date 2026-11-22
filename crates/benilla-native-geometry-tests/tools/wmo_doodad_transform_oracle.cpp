// SPDX-License-Identifier: GPL-3.0-or-later
// Authored, single-reference WMO/MODD inputs; original extraction and G3D math.
#include <algorithm>
#include <array>
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <iterator>
#include <limits>
#include <map>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>
#include "wmo.h"
#include "G3D/Quat.h"
namespace fs = std::filesystem;
using Bytes = std::vector<uint8>;

static void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
}
static std::map<std::string, Bytes> authoredFiles;
MPQFile::MPQFile(const char* filename) : eof(true), buffer(nullptr), pointer(0), size(0) {
    const auto found = authoredFiles.find(filename);
    require(found != authoredFiles.end(), "Only closed-map authored input may be read");
    size = found->second.size(); buffer = new char[size];
    std::copy(found->second.begin(), found->second.end(), buffer); eof = !size;
}
size_t MPQFile::read(void* dest, size_t count) {
    require(pointer >= 0 && pointer + count <= size, "Authored MODF read bounds");
    if (count) std::memcpy(dest, buffer + pointer, count);
    pointer += count; eof = pointer >= size; return count;
}
void MPQFile::seek(int offset) {
    require(offset >= 0 && offset <= size, "Authored MODF seek bounds");
    pointer = offset; eof = pointer >= size;
}
void MPQFile::close() { delete[] buffer; buffer = nullptr; pointer = size = 0; eof = true; }

// PINNED_SOURCE_METHODS

static uint32 bits(float value) { uint32 out; std::memcpy(&out, &value, 4); return out; }
static float fromBits(uint32 value) { float out; std::memcpy(&out, &value, 4); return out; }
static void hex(std::ostream& out, float value) {
    out << '\t' << std::hex << std::setfill('0') << std::setw(8) << bits(value) << std::dec;
}
static void hexVector(std::ostream& out, const Vec3D& value) {
    hex(out, value.x); hex(out, value.y); hex(out, value.z);
}
static std::string hexBytes(const Bytes& bytes) {
    std::ostringstream out;
    for (uint8 value : bytes) out << std::hex << std::setfill('0') << std::setw(2) << unsigned(value);
    return out.str();
}
template<typename T> static void append(Bytes& bytes, const T& value) {
    const uint8* data = reinterpret_cast<const uint8*>(&value);
    bytes.insert(bytes.end(), data, data + sizeof(value));
}
static void appendVector(Bytes& bytes, const Vec3D& value) {
    append(bytes, value.x); append(bytes, value.y); append(bytes, value.z);
}
static void save(const fs::path& path, const Bytes& data) {
    std::ofstream out(path, std::ios::binary);
    require(bool(out), "Open authored file");
    out.write(reinterpret_cast<const char*>(data.data()), data.size());
    require(bool(out), "Write authored file");
}
static Bytes read(const fs::path& path) {
    std::ifstream in(path, std::ios::binary);
    require(bool(in), "Read original output");
    return Bytes(std::istreambuf_iterator<char>(in), std::istreambuf_iterator<char>());
}
static uint32 read32(const Bytes& bytes, size_t offset) {
    require(offset + 4 <= bytes.size(), "Record field bounds");
    uint32 out; std::memcpy(&out, bytes.data() + offset, 4); return out;
}
static float readFloat(const Bytes& bytes, size_t offset) { return fromBits(read32(bytes, offset)); }

struct Case {
    std::string name;
    Vec3D parentPosition{10,20,30}, parentRotation{0,0,0}, childPosition{1,2,3};
    std::array<float,4> quaternion{{0,0,0,1}};
    float scale = 1;
    uint16 flags = 0, selectedSet = 0, parentScale = 1024, reference = 0;
    uint32 sets = 1, start = 0, count = 1, map = 7, tileX = 12, tileY = 34;
    uint32 uniqueId = 0x89abcdefu;
    int parentVertices = 3, childVertices = 3;
    bool parentExists = true, childExists = true, childShort = false;
    std::string path = "World\\Authored\\AUTHORED NAME.MDX";
    std::string rawBasename = "Authored_Name.m2";
};
static std::vector<Case> cases() {
    std::vector<Case> out;
    const auto add = [&](const char* name) -> Case& { out.emplace_back(); out.back().name = name; return out.back(); };
    add("identity_mdx");
    auto& portableMdx = add("portable_mdx"); portableMdx.path = "Models\\Solid.mdx"; portableMdx.rawBasename = "Solid.m2";
    auto& portableMdl = add("portable_mdl"); portableMdl.path = "Models\\Solid.mdl"; portableMdl.rawBasename = "Solid.m2";
    auto& portableM2 = add("portable_m2"); portableM2.path = "Models\\Solid.m2"; portableM2.rawBasename = "Solid.m2";
    add("identity_mdl").path = "World\\Authored\\aUTHORED NAME.MdL";
    add("identity_m2").path = "World\\Authored\\Authored Name.m2";
    add("nonunit_quaternion").quaternion = {{2,-3,4,5}};
    add("unit_quaternion").quaternion = {{0.5f,-0.5f,0.5f,0.5f}};
    add("negated_quaternion").quaternion = {{-0.5f,0.5f,-0.5f,-0.5f}};
    add("zero_quaternion").quaternion = {{0,0,0,0}};
    add("signed_zero_quaternion").quaternion = {{-0.f,0,-0.f,-0.f}};
    add("underflow_quaternion").quaternion = {{fromBits(1),fromBits(2),0,0}};
    add("tiny_nonzero_norm").quaternion = {{0x1p-70f,0,0,0x1p-70f}};
    add("large_finite_norm").quaternion = {{0x1p+60f,-0x1p+60f,0x1p+60f,0x1p+60f}};
    add("overflow_norm_finite_input").quaternion = {{std::numeric_limits<float>::max(),0,0,1}};
    add("parent_rotation").parentRotation = {13,37,-9};
    auto& composed = add("composed_nonunit_rotation");
    composed.parentRotation = {13,37,-9}; composed.quaternion = {{2,-3,4,5}};
    composed.childPosition = {-7,11,0.25f}; composed.scale = 1.75f;
    add("negative_parent_rotation").parentRotation = {-123.25f,-271.5f,405.75f};
    add("large_parent_rotation").parentRotation = {65536,-131072,32768};
    add("max_finite_parent_rotation").parentRotation = {std::numeric_limits<float>::max(),-std::numeric_limits<float>::max(),1};
    add("zero_origin").parentPosition = {0,27,0};
    add("signed_zero_origin").parentPosition = {-0.f,-0.f,-0.f};
    add("only_x_zero").parentPosition = {0,27,4};
    add("only_z_zero").parentPosition = {4,27,0};
    auto& destructible = add("destructible_origin"); destructible.flags = 1; destructible.parentPosition = {0,27,0};
    auto& destructibleNonzero = add("destructible_nonzero"); destructibleNonzero.flags = 1;
    auto& noCollision = add("no_parent_collision_origin"); noCollision.parentVertices = 0; noCollision.parentPosition = {0,27,0};
    add("no_parent_collision_nonzero").parentVertices = 0;
    add("missing_parent_raw").parentExists = false;
    add("other_parent_flags").flags = 0xfffe;
    add("worldspawn").tileX = 65; out.back().tileY = 65;
    add("only_worldspawn_x").tileX = 65;
    add("only_worldspawn_y").tileY = 65;
    add("scale_fraction").scale = 0.03125f;
    add("scale_negative").scale = -2;
    add("scale_zero").scale = 0;
    add("scale_signed_zero").scale = -0.f;
    add("scale_min_subnormal").scale = fromBits(1);
    add("scale_max_finite").scale = std::numeric_limits<float>::max();
    add("ignored_parent_scale").parentScale = 65535;
    auto& largePosition = add("large_finite_position");
    largePosition.parentPosition = {0x1p+100f,-0x1p+100f,0x1p+100f};
    largePosition.childPosition = {0x1p+90f,0x1p+91f,-0x1p+92f};
    largePosition.parentRotation = {13,37,-9};
    add("signed_zero_child_position").childPosition = {-0.f,0,-0.f};
    auto& positive = add("gimbal_positive_exact"); positive.quaternion = {{0,1,0,1}};
    auto& negative = add("gimbal_negative_exact"); negative.quaternion = {{0,-1,0,1}};
    // Adjacent authored binary32 components distinguish branch rounding around +/-1.
    add("gimbal_positive_below").quaternion = {{0,fromBits(0x3f7fffff),0,1}};
    add("gimbal_positive_above").quaternion = {{0,fromBits(0x3f800001),0,1}};
    add("gimbal_negative_below").quaternion = {{0,-fromBits(0x3f7fffff),0,1}};
    add("gimbal_negative_above").quaternion = {{0,-fromBits(0x3f800001),0,1}};
    add("parent_gimbal_positive").parentRotation = {90,0,0};
    add("parent_gimbal_negative").parentRotation = {-90,0,0};
    add("parent_gimbal_before").parentRotation = {fromBits(bits(90.f)-1),0,0};
    add("parent_gimbal_after").parentRotation = {fromBits(bits(90.f)+1),0,0};
    add("parent_gimbal_inside_positive").parentRotation = {89.9f,0,0};
    add("parent_gimbal_outside_positive").parentRotation = {90.1f,0,0};
    add("parent_gimbal_inside_negative").parentRotation = {-89.9f,0,0};
    add("parent_gimbal_outside_negative").parentRotation = {-90.1f,0,0};
    auto& composedPositive = add("gimbal_positive_nonzero_x");
    composedPositive.parentRotation = {90,0,0}; composedPositive.quaternion = {{0.25f,0,0,1}};
    auto& composedNegative = add("gimbal_negative_nonzero_x");
    composedNegative.parentRotation = {-90,0,0}; composedNegative.quaternion = {{0.25f,0,0,1}};
    auto& translatedZero = add("zero_origin_rotated");
    translatedZero.parentPosition = {0,27,0}; translatedZero.parentRotation = {13,37,-9};
    auto& destructibleRotated = add("destructible_rotated");
    destructibleRotated.flags = 1; destructibleRotated.parentRotation = {13,37,-9};
    auto& noCollisionRotated = add("no_parent_collision_rotated");
    noCollisionRotated.parentVertices = 0; noCollisionRotated.parentRotation = {13,37,-9};
    add("out_of_set").count = 0;
    add("missing_selected_set").selectedSet = 1;
    add("no_sets").sets = 0;
    auto& reference = add("nonzero_reference"); reference.reference = 1; reference.start = 1;
    add("missing_child_raw").childExists = false;
    add("empty_child_collision").childVertices = 0;
    add("short_child_header").childShort = true;
    const auto portable = [&](const char* name) -> Case& {
        Case& c = add(name); c.path = "Models\\Solid.mdx"; c.rawBasename = "Solid.m2"; return c;
    };
    portable("portable_nonunit").quaternion = {{2,-3,4,5}};
    portable("portable_parent_rotation").parentRotation = {13,37,-9};
    auto& portableComposed = portable("portable_composed");
    portableComposed.parentRotation = {13,37,-9}; portableComposed.quaternion = {{2,-3,4,5}};
    portableComposed.childPosition = {-7,11,0.25f}; portableComposed.scale = 1.75f;
    portable("portable_scale_zero").scale = 0;
    portable("portable_scale_negative").scale = -2;
    portable("portable_scale_signed_zero").scale = -0.f;
    portable("portable_zero_origin").parentPosition = {0,27,0};
    portable("portable_signed_zero_origin").parentPosition = {-0.f,-0.f,-0.f};
    auto& portableDestructibleOrigin = portable("portable_destructible_origin");
    portableDestructibleOrigin.flags = 1; portableDestructibleOrigin.parentPosition = {0,27,0};
    portable("portable_destructible_nonzero").flags = 1;
    auto& portableDestructibleZero = portable("portable_destructible_signed_zero");
    portableDestructibleZero.flags = 1; portableDestructibleZero.parentPosition = {-0.f,-0.f,-0.f};
    auto& portableNoCollisionOrigin = portable("portable_no_parent_collision_origin");
    portableNoCollisionOrigin.parentVertices = 0; portableNoCollisionOrigin.parentPosition = {0,27,0};
    portable("portable_no_parent_collision_nonzero").parentVertices = 0;
    auto& portableNoCollisionZero = portable("portable_no_parent_collision_signed_zero");
    portableNoCollisionZero.parentVertices = 0; portableNoCollisionZero.parentPosition = {-0.f,-0.f,-0.f};
    auto& mathParent = portable("math_parent_z90");
    mathParent.flags = 1; mathParent.parentPosition = {0,0,0};
    mathParent.parentRotation = {0,90,0}; mathParent.childPosition = {1,0,0};
    auto& mathQuaternion = portable("math_quat_z90_scaled");
    mathQuaternion.flags = 1; mathQuaternion.parentPosition = {0,0,0}; mathQuaternion.quaternion = {{0,0,4,4}};
    auto& mathGimbal = portable("math_gimbal_signed_zero");
    mathGimbal.flags = 1; mathGimbal.parentPosition = {0,0,0};
    mathGimbal.parentRotation = {90,0,0}; mathGimbal.childPosition = {0,0,0}; mathGimbal.quaternion = {{1,0,0,0}};
    return out;
}

static Bytes modf(const Case& c) {
    Bytes bytes;
    append(bytes,uint32(3)); append(bytes,c.uniqueId);
    appendVector(bytes,c.parentPosition); appendVector(bytes,c.parentRotation);
    appendVector(bytes,Vec3D(-10,-20,-30)); appendVector(bytes,Vec3D(40,50,60));
    append(bytes,c.flags); append(bytes,c.selectedSet); append(bytes,uint16(29)); append(bytes,c.parentScale);
    require(bytes.size()==64,"Authored MODF length"); return bytes;
}
static Bytes modd(const Case& c) {
    Bytes bytes; append(bytes,uint32(7)); appendVector(bytes,c.childPosition);
    for (float value:c.quaternion) append(bytes,value);
    append(bytes,c.scale); append(bytes,uint32(0x12345678));
    require(bytes.size()==40,"Authored MODD length"); return bytes;
}
static Bytes rawHeader(int count) {
    Bytes bytes{'V','M','A','P','s','0','5',0}; append(bytes,int32(count)); return bytes;
}

int main(int argc, char** argv) {
    require(argc==2,"Expected output directory");
    static_assert(sizeof(float)==4 && std::numeric_limits<float>::is_iec559,"IEEE binary32");
    static_assert(sizeof(Vec3D)==12 && sizeof(G3D::Vector3)==12,"Source vector ABI");
    static_assert(sizeof(ADT::MODF)==64 && sizeof(WMO::MODD)==40,"Source placement ABI");
    static_assert(sizeof(int)==4 && sizeof(uint16)==2,"Source integer ABI");
    require(std::fesetround(FE_TONEAREST)==0,"Round-to-nearest");
    const fs::path output=fs::absolute(argv[1]);
    const fs::path raw=output.parent_path()/"raw"; fs::create_directory(raw);
    require(raw.string().size()<sizeof(szWorkDirWmo),"Bounded raw directory");
    std::strcpy(szWorkDirWmo,raw.c_str());
    std::ofstream inputs(output/"cases.tsv"), results(output/"results.tsv"), matrices(output/"matrices.tsv");
    inputs << "# case\tparent_flags\tparent_vertices\tparent_exists\tchild_vertices\tchild_exists\tchild_short\tmap\ttile_x\ttile_y\tclient_id\tselected_set\tsets\tset_start\tset_count\treference\tparent_scale\tparent_px\tparent_py\tparent_pz\tparent_rx\tparent_ry\tparent_rz\tchild_px\tchild_py\tchild_pz\tquat_x\tquat_y\tquat_z\tquat_w\tchild_scale\tpath_hex\traw_basename\n";
    results << "# case\tparent_emitted\tparent_px\tparent_py\tparent_pz\tchild_emitted\tisolated_child_id\tflags\trecord_length\tname_length\tname_hex\tpx\tpy\tpz\trx\try\trz\tscale\tfinite_transform\tinvalid\tdivide_by_zero\toverflow\tunderflow\n";
    matrices << "# case\tparent_m00\tparent_m01\tparent_m02\tparent_m10\tparent_m11\tparent_m12\tparent_m20\tparent_m21\tparent_m22\tquat_m00\tquat_m01\tquat_m02\tquat_m10\tquat_m11\tquat_m12\tquat_m20\tquat_m21\tquat_m22\tcomposed_m00\tcomposed_m01\tcomposed_m02\tcomposed_m10\tcomposed_m11\tcomposed_m12\tcomposed_m20\tcomposed_m21\tcomposed_m22\teuler_unique\n";
    for (const Case& c:cases()) {
        fs::remove_all(raw); fs::create_directory(raw);
        const fs::path folder=output/c.name; fs::create_directory(folder);
        const Bytes modfBytes=modf(c), moddBytes=modd(c);
        save(folder/"input.modf",modfBytes); save(folder/"input.modd",moddBytes);
        Bytes paths{'u','n','u','s','e','d',0}; paths.insert(paths.end(),c.path.begin(),c.path.end()); paths.push_back(0);
        save(folder/"input.modn",paths);
        if (c.parentExists) save(raw/"Parent.wmo",rawHeader(c.parentVertices));
        // The declared authored raw basename must match the original helper lookup.
        if (c.childExists) {
            Bytes header=rawHeader(c.childVertices); if(c.childShort) header.resize(10);
            save(raw/c.rawBasename,header);
        }
        authoredFiles.clear(); authoredFiles["Authored.modf"]=modfBytes;
        MPQFile input("Authored.modf"); input.seek(4);
        uniqueObjectIds.clear(); // Isolated original ID allocator, never a global ID claim.
        FILE* parent=std::fopen((folder/"parent.dir").c_str(),"wb"); require(parent,"Open parent output");
        WMOInstance instance(input,"Parent.wmo",c.map,c.tileX,c.tileY,parent);
        require(!std::fclose(parent),"Close parent output");
        WMODoodadData data;
        data.Sets.resize(c.sets);
        for (auto& set:data.Sets) { set={}; set.StartIndex=c.start; set.Count=c.count; }
        data.Paths=std::make_unique<char[]>(paths.size()); std::copy(paths.begin(),paths.end(),data.Paths.get());
        data.Spawns.resize(c.reference+1);
        WMO::MODD& child=data.Spawns[c.reference]; child.NameIndex=7;
        child.Position=c.childPosition;
        child.Rotation.X=c.quaternion[0]; child.Rotation.Y=c.quaternion[1];
        child.Rotation.Z=c.quaternion[2]; child.Rotation.W=c.quaternion[3];
        child.Scale=c.scale; child.Color=0x12345678;
        data.References.insert(c.reference); // Exactly one unordered-set member.
        FILE* stream=std::fopen((folder/"child.dir").c_str(),"wb"); require(stream,"Open child output");
        std::feclearexcept(FE_ALL_EXCEPT);
        Doodad::ExtractSet(data,instance.m_wmo,c.map,c.tileX,c.tileY,stream);
        const int exceptions=std::fetestexcept(FE_ALL_EXCEPT);
        require(!std::fclose(stream),"Close child output");
        const Bytes parentBytes=read(folder/"parent.dir"), childBytes=read(folder/"child.dir");
        inputs << c.name << '\t' << c.flags << '\t' << c.parentVertices << '\t' << c.parentExists
               << '\t' << c.childVertices << '\t' << c.childExists << '\t' << c.childShort
               << '\t' << c.map << '\t' << c.tileX << '\t' << c.tileY << '\t' << c.uniqueId
               << '\t' << c.selectedSet << '\t' << c.sets << '\t' << c.start << '\t' << c.count
               << '\t' << c.reference << '\t' << c.parentScale;
        hexVector(inputs,c.parentPosition); hexVector(inputs,c.parentRotation); hexVector(inputs,c.childPosition);
        for(float value:c.quaternion) hex(inputs,value); hex(inputs,c.scale);
        inputs << '\t' << hexBytes(Bytes(c.path.begin(),c.path.end())) << '\t' << c.rawBasename << '\n';
        results << c.name << '\t' << !parentBytes.empty(); hexVector(results,instance.m_wmo.Position);
        results << '\t' << !childBytes.empty();
        if(childBytes.empty()) results << "\t0\t0\t0\t0\t-\t-\t-\t-\t-\t-\t-\t-\t0";
        else {
            const uint32 length=read32(childBytes,50);
            require(childBytes.size()==54+length,"Exactly one original child record");
            results << '\t' << read32(childBytes,18) << '\t' << read32(childBytes,12)
                    << '\t' << childBytes.size() << '\t' << length << '\t'
                    << hexBytes(Bytes(childBytes.begin()+54,childBytes.end()));
            bool finite=true;
            for (size_t offset=22;offset<50;offset+=4) { const float value=readFloat(childBytes,offset); hex(results,value); finite &= std::isfinite(value); }
            results << '\t' << finite;
        }
        for(int exception:{FE_INVALID,FE_DIVBYZERO,FE_OVERFLOW,FE_UNDERFLOW}) results << '\t' << bool(exceptions&exception);
        results << '\n';
        // Diagnostic evidence uses actual source methods, independently of stream decoding.
        const G3D::Matrix3 parentMatrix=G3D::Matrix3::fromEulerAnglesZYX(G3D::toRadians(c.parentRotation.y),G3D::toRadians(c.parentRotation.x),G3D::toRadians(c.parentRotation.z));
        const G3D::Matrix3 childMatrix=G3D::Quat(c.quaternion[0],c.quaternion[1],c.quaternion[2],c.quaternion[3]).toRotationMatrix();
        const G3D::Matrix3 composed=childMatrix*parentMatrix;
        matrices << c.name;
        for(const auto& matrix:{parentMatrix,childMatrix,composed}) for(unsigned row=0;row<3;++row) for(unsigned col=0;col<3;++col) hex(matrices,matrix[row][col]);
        float x,y,z; matrices << '\t' << composed.toEulerAnglesXYZ(x,y,z) << '\n';
    }
    require(bool(inputs)&&bool(results)&&bool(matrices),"Write evidence tables");
    std::printf("Executed %zu isolated one-candidate original transforms\n",cases().size());
}
