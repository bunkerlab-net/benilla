// SPDX-License-Identifier: GPL-3.0-or-later
// Authored analytic geometry around unchanged TargetedMovementGenerator.cpp:1210-1347.
#include <G3D/Vector3.h>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <vector>
#include <algorithm>
#include <type_traits>
using G3D::Vector3; using PointsArray=std::vector<Vector3>;using int32=int32_t;
#define M_PI_F float(M_PI)
constexpr int SPAWN_GROUP_FORMATION_TYPE_RANDOM=0,MOVE_WALK=0;
struct Position { float x,y,z,o; Position(float a,float b,float c,float d):x(a),y(b),z(c),o(d){} };
struct Spline {
    bool final=false; float speed=3.0f; PointsArray points{{0,0,0},{2,1,0},{4,2,0},{100,20,0}};
    bool Finalized() const { return final; } float Speed() const { return speed; }
    Vector3 ComputePosition() const { return {0.25f,0.5f,0}; }
    int GetRawPathIndex() const { return 0; }
    Spline const& _Spline() const { return *this; }
    int last() const { return int(points.size())-1; }
    Vector3 getPoint(int i) const { return points.at(i); }
    int32 ComputeTimeToIndex(int i) const { return i*1000; }
};
struct Unit {
    Spline* movespline; Vector3 pos;
    bool IsFlying() const { return false; } bool IsInWater() const { return false; } bool HasHoverAura() const { return false; }
    float GetPositionX() const { return pos.x; } float GetPositionY() const { return pos.y; } float GetPositionZ() const { return pos.z; }
    float GetOrientation() const { return 0.25f; }
    void MovePositionToFirstCollision(Position& p,float d,float a) { p.x+=d*std::cos(a);p.y+=d*std::sin(a); }
    float GetSpeed(int) { return 2.5f; }
    void UpdateAllowedPositionZ(float&,float&,float& z) { z=0; }
};
struct Formation { int shape=0; bool mmap=true; int GetCurrentShape() { return shape; } bool CanUseMMap() { return mmap; } };
struct Slot { Formation* formation; float angle=3.0f,distance=2.0f; float GetAngle(){return angle;} float GetDistance(){return distance;} Formation* GetFormationData(){return formation;} };
struct PathFinder { PointsArray path; void calculate(Vector3 a, Vector3 b,bool,bool) { path={a,b}; } PointsArray& getPath(){return path;} };
struct FormationMovementGenerator { Unit* i_target; Slot* m_slot; PathFinder* i_path; float m_lastAngle=0; float BuildPath(Unit&,PointsArray&); };
// INSERT_METHODS
uint32_t bits(float f) { uint32_t u;std::memcpy(&u,&f,4);return u; }
int main() {
    std::fprintf(stderr,"global_atan2_is_float=%d\n",int(std::is_same_v<decltype(atan2(float(),float())),float>));
    for(int id=0;id<8;++id) {
        Spline masterSpline,slaveSpline; masterSpline.final=id<2;slaveSpline.final=id%2==0;
        Formation formation;formation.shape=id%2;formation.mmap=id!=7;
        Slot slot{&formation};PathFinder pathfinder;
        Unit master{&masterSpline,{0,0,0}},slave{&slaveSpline,{-2,-3,0}};
        FormationMovementGenerator generator{&master,&slot,&pathfinder};
        if(id==1)slot.distance=0;
        if(id==3)masterSpline.speed=0.25f;
        if(id==4)masterSpline.points={{0,0,0},{0.1f,0.1f,0},{0.2f,0.2f,0},{0.3f,0.3f,0}};
        if(id==5)masterSpline.points={{0,0,0},{-2,1,0},{-4,2,0},{-6,3,0}};
        if(id==6)masterSpline.points={{0,0,0}};
        PointsArray output;auto speed=generator.BuildPath(slave,output);
        std::printf("%d %08x %08x %zu",id,bits(speed),bits(generator.m_lastAngle),output.size());
        for(auto p:output)for(float x:{p.x,p.y,p.z})std::printf(" %08x",bits(x));std::printf("\n");
    }
}
