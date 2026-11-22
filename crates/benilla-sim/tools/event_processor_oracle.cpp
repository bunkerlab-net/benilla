// SPDX-License-Identifier: GPL-3.0-or-later
// Authored synthetic differential driver around the byte-identical CMaNGOS
// Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f EventProcessor implementation.
// Source GPL-2.0-or-later, driver GPL-3.0-or-later. No database/game inputs.
#include "event_processor_source/EventProcessor.h"
#include <iostream>
#include <map>
#include <sstream>
#include <stdexcept>
#include <string>
struct Probe;
struct Event : BasicEvent {
    Probe& owner;
    uint64 id;
    int behavior;
    uint64 arg;
    bool deletable;
    Event(Probe&, uint64, int, uint64, bool);
    ~Event() override;
    bool Execute(uint64 now, uint32 diff) override;
    void Abort(uint64 now) override;
    bool IsDeletable() const override { return deletable; }
};
struct Probe : EventProcessor {
    std::map<uint64, Event*> objects;
    bool quiet = false;
    void add(uint64 id, uint64 at, int behavior, uint64 arg, bool deletable) {
        if (objects.count(id)) throw std::runtime_error("aliased oracle event");
        AddEvent(new Event(*this, id, behavior, arg, deletable), at);
    }
    Event* get(uint64 id) { auto it=objects.find(id); return it==objects.end()?nullptr:it->second; }
    void dump() {
        std::map<uint64, Event*> detached=objects;
        for (auto const& row : m_events) detached.erase(static_cast<Event*>(row.second)->id);
        std::cout<<"S "<<m_time<<' '<<m_aborting<<' '<<m_events.size()<<' '<<detached.size()<<'\n';
        for (auto const& row : m_events) {
            auto e=static_cast<Event*>(row.second);
            std::cout<<"Q "<<e->id<<' '<<row.first<<' '<<e->m_addTime<<' '<<e->m_execTime<<' '<<e->to_Abort<<'\n';
        }
        for (auto const& row : detached) {
            auto e=row.second;
            std::cout<<"H "<<e->id<<' '<<e->m_addTime<<' '<<e->m_execTime<<' '<<e->to_Abort<<'\n';
        }
    }
    void reset() {
        quiet=true;
        KillAllEvents(true);
        while(!objects.empty()) delete objects.begin()->second;
        m_time=0; m_aborting=false; quiet=false;
    }
    ~Probe() { reset(); }
};
Event::Event(Probe& p,uint64 i,int b,uint64 a,bool d):owner(p),id(i),behavior(b),arg(a),deletable(d) { owner.objects[id]=this; }
Event::~Event() { if(!owner.quiet) std::cout<<"D "<<id<<'\n'; owner.objects.erase(id); }
bool Event::Execute(uint64 now,uint32 diff) {
    std::cout<<"E "<<id<<' '<<now<<' '<<diff<<' '<<m_addTime<<' '<<m_execTime<<'\n';
    if(behavior==1 && arg>0) { --arg; owner.AddEvent(this,now,false); return false; }
    if(behavior==2) return false;
    if(behavior==3) owner.ModifyEventTime(owner.get(arg),now);
    if(behavior==4 && owner.get(arg)) owner.get(arg)->to_Abort=true;
    if(behavior==5) owner.add(arg,now,0,0,true);
    if(behavior==6 && arg>0) { --arg; owner.AddEvent(this,now+1,false); return false; }
    if(behavior==7) owner.KillEvent(this);
    if(behavior==8) owner.ModifyEventTime(this,now+99);
    if(behavior==9) owner.KillEvent(owner.get(arg));
    return true;
}
void Event::Abort(uint64 now) {
    if(owner.quiet) return;
    std::cout<<"A "<<id<<' '<<now<<' '<<m_addTime<<' '<<m_execTime<<'\n';
    if(behavior==3) owner.ModifyEventTime(owner.get(arg),now);
    if(behavior==4 && owner.get(arg)) owner.get(arg)->to_Abort=true;
    if(behavior==5) owner.add(arg,now,0,0,true);
    if(behavior==9) owner.KillEvent(owner.get(arg));
    if(behavior==10) deletable=!deletable;
}
int main() {
    Probe p;
    std::string line;
    while(std::getline(std::cin,line)) {
        if(line.empty() || line[0]=='#') continue;
        std::cout<<">"<<line<<'\n';
        std::istringstream in(line);
        char op; uint64 id=0, at=0, arg=0; int behavior=0, flag=0;
        in>>op;
        switch(op) {
            case 'P': p.reset(); break;
            case 'N': in>>id>>at>>behavior>>arg>>flag; p.add(id,at,behavior,arg,flag); break;
            case 'M': in>>id>>at; p.ModifyEventTime(p.get(id),at); break;
            case 'B': in>>id>>flag; if(p.get(id)) p.get(id)->to_Abort=flag; break;
            case 'L': in>>id>>flag; if(p.get(id)) p.get(id)->deletable=flag; break;
            case 'T': in>>id>>behavior>>arg; if(p.get(id)) { p.get(id)->behavior=behavior; p.get(id)->arg=arg; } break;
            case 'R': in>>id>>at>>flag; if(!p.get(id)) throw std::runtime_error("missing re-add"); p.AddEvent(p.get(id),at,flag); break;
            case 'K': in>>id; p.KillEvent(p.get(id)); break;
            case 'U': in>>at; p.Update(static_cast<uint32>(at)); break;
            case 'X': in>>flag; p.KillAllEvents(flag); break;
            default: throw std::runtime_error("invalid oracle command");
        }
        if(in.fail()) throw std::runtime_error("invalid oracle input");
        p.dump();
    }
}
