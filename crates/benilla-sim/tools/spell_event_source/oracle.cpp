// SPDX-License-Identifier: GPL-3.0-or-later
// Authored bounded host. Only the included upstream fragments and queue are
// exact source: this harness does NOT implement the full Spell/Unit lifecycle.
#include "../event_processor_source/EventProcessor.h"
#include "UniqueTrackablePtr.h"
#include "SpellState.inc"
#include <iostream>
#include <map>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>

static bool quiet = false;
template<class... Args> void trace(Args const&... args) {
    if (quiet) return;
    bool first=true;
    ((std::cout << (std::exchange(first,false)?"":" ") << args), ...);
    std::cout << '\n';
}
struct World { static bool stopped; static bool IsStopped() { return stopped; } };
bool World::stopped=false;
struct Log {
    // Exact constructor's leak diagnostic, with synthetic spell ID as identity.
    void outError(char const*, char const*, uint32, uint32 id) { trace("W",id); }
} sLog;
constexpr uint32 TYPEID_PLAYER=4;
struct Spell;
struct Unit;
struct Processor : EventProcessor {
    uint64 now() const { return m_time; }
    bool aborting() const { return m_aborting; }
    void reset() { m_time=0; m_aborting=false; }
};
struct SpellInfo { uint32 Id; bool channeled=false; };
bool IsChanneledSpell(SpellInfo const* info) { return info->channeled; }
struct Unit {
    Processor m_events;
    uint64 map_time=0;
    bool other_cast=false;
    std::map<uint64,Spell*> spells;
    std::map<uint64,std::pair<char,BasicEvent*>> events;
    uint32 GetTypeId() const { return TYPEID_PLAYER; }
    uint32 GetGUIDLow() const { return 1; }
    bool IsNonMeleeSpellCasted(bool,bool,bool) const { return other_cast; }
    void reset();
    void start(uint64 id,uint32 timer,int64_t delay,bool current,bool fail,uint64 created);
    void update(uint32 diff,uint64 map,bool clear);
    void dump();
    ~Unit() { reset(); }
};
struct Spell {
    Unit& owner;
    uint64 id;
    SpellInfo info;
    SpellInfo* m_spellInfo=&info;
    SpellState state=SPELL_STATE_CASTING,dispatch_state=SPELL_STATE_CASTING;
    uint32 timer;
    int64_t travel;
    uint64 delay_start=0,created;
    bool updated=false;
    bool m_referencedFromCurrentSpell=false,m_executedCurrently=false;
    uint64 next_offset=0;
    Spell(Unit& unit,uint64 i,uint32 t,int64_t d,bool c,uint64 born)
      :owner(unit),id(i),info{static_cast<uint32>(i),false},timer(t),travel(d),created(born),m_referencedFromCurrentSpell(c) {
        owner.spells.emplace(id,this);
    }
    ~Spell() { trace("F",id); owner.spells.erase(id); }
#include "SpellDeletable.inc"
    SpellState getState() const { return state; }
    Unit* GetTrueCaster() { return &owner; }
    Unit* GetCaster() { return &owner; }
    uint64 GetDelayStart() const { return delay_start; }
    uint64 GetDelayMoment() const { return static_cast<uint64>(travel); }
    void SetDelayStart(uint64 now) { delay_start=now; trace("T",id,now); }
    void release() {
        trace("L",id);
        if (travel>=0) state=SPELL_STATE_TRAVELING;
        else state=SPELL_STATE_FINISHED;
    }
    void update(uint32 diff) {
        struct CaptureState { Spell& spell; ~CaptureState() { spell.dispatch_state=spell.state; } } capture{*this};
        trace("V",id,diff,static_cast<int>(state),timer);
        // Authored selected host behavior, based on Spell.cpp:3519-3574.
        // Target validity, movement, spell effects, RNG and channel math omitted.
        if (!updated) {
            updated=true;
            if (created==owner.map_time) { trace("Z",id); return; }
        }
        if (state==SPELL_STATE_CASTING) {
            timer=diff>=timer?0:timer-diff;
            if (!timer) release();
        }
    }
    uint64 handle_delayed(uint64 offset) {
        uint64 next=std::exchange(next_offset,0);
        trace("G",id,offset,next);
        if (!next) state=SPELL_STATE_FINISHED;
        return next;
    }
    void handle_immediate() { trace("M",id); state=SPELL_STATE_CHANNELING; }
    void cancel() { trace("C",id); state=SPELL_STATE_FINISHED; }
};
#include "SpellEvent.declaration.inc"
#include "SpellEvent.methods.inc"

struct TracedSpellEvent : SpellEvent {
    Unit& owner;
    uint64 id;
    TracedSpellEvent(Spell* spell):SpellEvent(spell),owner(spell->owner),id(spell->id) {
        owner.events.emplace(id,std::make_pair('S',this));
    }
    ~TracedSpellEvent() override { trace("D",id); owner.events.erase(id); }
    bool Execute(uint64 now,uint32 diff) override {
        trace("E",id,now,diff,m_addTime,m_execTime);
        auto spell=GetSpell();
        auto start=spell->delay_start;
        auto next=spell->next_offset;
        spell->dispatch_state=spell->state;
        bool result=SpellEvent::Execute(now,diff);
        trace("O",id,now,static_cast<int>(spell->dispatch_state),spell->IsDeletable(),start,
              spell->travel,next,spell->info.channeled,owner.other_cast,result,
              spell->delay_start,result?0:m_execTime);
        return result;
    }
    void Abort(uint64 now) override {
        trace("A",id,now,m_addTime,m_execTime);
        SpellEvent::Abort(now);
    }
};
struct AiNotify : BasicEvent {
    Unit& owner; uint64 id;
    AiNotify(Unit& unit,uint64 i):owner(unit),id(i) { owner.events.emplace(id,std::make_pair('I',this)); }
    ~AiNotify() override { trace("D",id); owner.events.erase(id); }
    // Authored stand-in for the one-shot callback, no nearby-object visitor.
    bool Execute(uint64 now,uint32 diff) override {
        trace("I",id,now,diff,m_addTime,m_execTime); return true;
    }
    void Abort(uint64 now) override { trace("A",id,now,m_addTime,m_execTime); }
};
void Unit::reset() {
    quiet=true; World::stopped=true;
    m_events.KillAllEvents(true);
    // Only test isolation cleanup: the source custom deleter may leak a
    // nondeletable spell. That leak is retained in snapshots before reset.
    while (!spells.empty()) delete spells.begin()->second;
    events.clear(); m_events.reset(); map_time=0; other_cast=false;
    World::stopped=false; quiet=false;
}
void Unit::start(uint64 id,uint32 timer,int64_t delay,bool current,bool fail,uint64 created) {
    if (!id || events.count(id) || spells.count(id)) throw std::runtime_error("duplicate/zero identity");
    Spell* spell=new Spell(*this,id,timer,delay,fail?false:current,created);
    auto event=new TracedSpellEvent(spell);
    // Authored selected SpellStart host: queue before PreCastCheck result.
    m_events.AddEvent(event,m_events.CalculateTime(1));
    if (fail) spell->state=SPELL_STATE_FINISHED;
    else if (!timer) spell->release();
}
void Unit::update(uint32 diff,uint64 map,bool clear) {
    map_time=map; m_events.Update(diff);
    // Authored current-reference phase after queue, Unit.cpp:4094-4102.
    if (clear) for (auto const& row:spells) {
        auto spell=row.second;
        if (spell->state==SPELL_STATE_FINISHED && spell->m_referencedFromCurrentSpell) {
            trace("J",spell->id); spell->m_referencedFromCurrentSpell=false;
        }
    }
}
void Unit::dump() {
    trace("S",m_events.now(),map_time,m_events.GetEvents().size(),events.size(),spells.size(),m_events.aborting());
    for (auto const& row:m_events.GetEvents()) {
        uint64 id=0; char kind='?';
        for (auto const& e:events) if (e.second.second==row.second) { id=e.first; kind=e.second.first; }
        if (!id) throw std::runtime_error("unknown queue object");
        auto e=row.second;
        trace("Q",id,kind,row.first,e->m_addTime,e->m_execTime,e->to_Abort);
    }
    for (auto const& row:spells) {
        auto s=row.second;
        trace("H",s->id,static_cast<int>(s->state),s->timer,s->delay_start,s->travel,s->m_referencedFromCurrentSpell,s->m_executedCurrently,s->updated);
    }
}
int main() {
    Unit unit;
    std::string line;
    while (std::getline(std::cin,line)) {
        if (line.empty() || line[0]=='#') continue;
        std::cout<<'>'<<line<<'\n';
        std::istringstream in(line);
        char op; uint64 id=0,a=0,b=0,c=0,d=0; int64_t delay=0;
        in>>op;
        auto spell=[&](){ auto i=unit.spells.find(id); if(i==unit.spells.end()) throw std::runtime_error("missing spell"); return i->second; };
        auto event=[&](){ auto i=unit.events.find(id); if(i==unit.events.end()) throw std::runtime_error("missing event"); return i->second.second; };
        switch(op) {
            case 'P': { std::string name; in>>name; unit.reset(); break; }
            case 'N': in>>id>>a>>delay>>b>>c>>d; unit.start(id,static_cast<uint32>(a),delay,b,c,d); break;
            case 'U': in>>a>>b>>c; unit.update(static_cast<uint32>(a),b,c); break;
            case 'I': in>>id>>a; if(unit.events.count(id)) throw std::runtime_error("duplicate identity"); unit.m_events.AddEvent(new AiNotify(unit,id),unit.m_events.CalculateTime(a)); break;
            case 'R': in>>id>>a>>b; spell()->m_referencedFromCurrentSpell=a; spell()->m_executedCurrently=b; break;
            case 'C': in>>id; spell()->cancel(); break;
            case 'B': in>>id; event()->to_Abort=true; break;
            case 'X': in>>a>>b; World::stopped=b; unit.m_events.KillAllEvents(a); break;
            case 'K': in>>id; unit.m_events.KillEvent(event()); break;
            case 'G': in>>id>>a; spell()->next_offset=a; break;
            case 'Y': in>>id>>a; spell()->info.channeled=true; unit.other_cast=a; break;
            default: throw std::runtime_error("unknown command");
        }
        if (in.fail()) throw std::runtime_error("invalid command");
        std::string extra; if(in>>extra) throw std::runtime_error("trailing command input");
        unit.dump();
    }
}
