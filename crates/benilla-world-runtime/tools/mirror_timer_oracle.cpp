// SPDX-License-Identifier: GPL-3.0-or-later
// Authored operations only. All timer state transitions and getters are upstream.
#include <array>
#include <climits>
#include <cstdint>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <limits>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>

using uint32 = std::uint32_t;
using int32 = std::int32_t;
using int64 = std::int64_t;
static_assert(sizeof(uint32) == 4 && sizeof(int32) == 4 && INT_MAX == 2147483647,
              "source std::abs(int32) profile requires 32-bit int");
// PINNED_TIME_CONSTANTS
// PINNED_SHORT_INTERVAL_TIMER
// This instrumentation changes access checking only, for read-only observations.
// The complete class, constructor, getters and method bodies remain verbatim.
#define private public
// PINNED_MIRROR_TIMER
#undef private
// PINNED_MIRROR_METHODS

static void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
}

static std::string state(const MirrorTimer& timer) {
    std::ostringstream out;
    out << timer.GetScale() << '\t' << timer.GetSpellId() << '\t' << timer.m_status
        << '\t' << timer.IsActive() << '\t' << timer.m_frozen
        << '\t' << timer.GetDuration() << '\t' << timer.m_tracker.GetCurrent()
        << '\t' << timer.m_pulse.GetInterval() << '\t' << timer.m_pulse.GetCurrent()
        << '\t' << timer.GetRemaining() << '\t' << timer.IsFrozen()
        << '\t' << timer.IsRegenerating();
    return out.str();
}

static std::size_t rows = 0, cases = 0, undefined_abs = 0;
struct Case {
    std::string name;
    unsigned step = 0;
    MirrorTimer timer;
    std::ostream& output;

    Case(std::ostream& out, std::string label, unsigned type = 0)
        : name(std::move(label)), timer(static_cast<MirrorTimer::Type>(type)), output(out) {
        require(type < MirrorTimer::NUM_TIMERS, "authored timer type");
        ++cases;
    }

    void emit(const std::string& op, int64 a = 0, uint32 b = 0, uint32 c = 0) {
        require(op == "set_scale" ? a >= INT_MIN && a <= INT_MAX :
                a >= 0 && a <= std::numeric_limits<uint32>::max(), "authored argument range");
        const auto before = state(timer);
        std::string result = "-";
        if (op == "inspect") {}
        else if (op == "start") timer.Start(uint32(a), b);
        else if (op == "start_remaining") timer.Start(uint32(a), b, c);
        else if (op == "stop") timer.Stop();
        else if (op == "set_remaining") timer.SetRemaining(uint32(a));
        else if (op == "set_duration") timer.SetDuration(uint32(a));
        else if (op == "set_frozen") {
            require(a <= 1, "authored boolean argument");
            timer.SetFrozen(a != 0);
        }
        else if (op == "set_scale") timer.SetScale(int32(a));
        else if (op == "fetch_status") result = std::to_string(timer.FetchStatus());
        else if (op == "update") {
            // Domain guard, NOT an upstream result. The original function returns
            // before abs for inactive/frozen timers, even when the scale is INT_MIN.
            if (timer.IsActive() && !timer.IsFrozen() && timer.GetScale() == INT_MIN) {
                result = "undefined_abs";
                ++undefined_abs;
            } else result = std::to_string(timer.Update(uint32(a)));
        }
        else throw std::runtime_error("unknown authored operation");
        output << name << '\t' << step++ << '\t' << timer.GetType() << '\t' << op
               << '\t' << a << '\t' << b << '\t' << c << '\t' << result
               << '\t' << before << '\t' << state(timer) << '\n';
        ++rows;
    }
};

static void sequences(std::ostream& out) {
    for (unsigned type = 0; type < MirrorTimer::NUM_TIMERS; ++type) {
        Case t(out, "constructor_" + std::to_string(type), type);
        t.emit("inspect"); t.emit("fetch_status"); t.emit("stop"); t.emit("update", 0);
        t.emit("update", 0xffffffffu); t.emit("set_duration", 17); t.emit("set_remaining", 9);
        t.emit("set_frozen", 1); t.emit("set_scale", -3); t.emit("fetch_status");
        t.emit("start", 100, 0xffffffffu); t.emit("update", 100);
        t.emit("start_remaining", 70, 100, 42); t.emit("fetch_status");
        t.emit("update", 23); t.emit("update", 1); t.emit("fetch_status");
        t.emit("stop"); t.emit("fetch_status"); t.emit("stop"); t.emit("fetch_status");
    }
    {
        Case t(out, "expiry_pulse_residual");
        t.emit("start", 100, 17); t.emit("fetch_status"); t.emit("update", 99);
        t.emit("update", 1); t.emit("fetch_status"); t.emit("update", 0);
        t.emit("update", 1999); t.emit("update", 1); t.emit("update", 6001);
        t.emit("update", 0); t.emit("update", 0); t.emit("update", 0);
        t.emit("update", 1998); t.emit("update", 1); t.emit("fetch_status");
    }
    {
        Case t(out, "expiry_overshoot_discarded");
        t.emit("start", 100, 18); t.emit("update", 9000); t.emit("update", 0);
        t.emit("update", 1999); t.emit("update", 1); t.emit("start", 100, 19);
        t.emit("update", 101); t.emit("update", 1999); t.emit("update", 1);
    }
    {
        Case t(out, "freeze_scale_and_status");
        t.emit("set_frozen", 1); t.emit("start", 100, 20); t.emit("fetch_status");
        t.emit("update", 500); t.emit("set_scale", 0); t.emit("fetch_status");
        t.emit("set_scale", -2); t.emit("fetch_status"); t.emit("update", 50);
        t.emit("set_frozen", 0); t.emit("fetch_status"); t.emit("set_frozen", 0);
        t.emit("fetch_status"); t.emit("update", 20); t.emit("set_scale", 2);
        t.emit("set_frozen", 1); t.emit("fetch_status"); t.emit("set_frozen", 1);
        t.emit("fetch_status"); t.emit("set_frozen", 0); t.emit("fetch_status");
        t.emit("set_scale", 0); t.emit("fetch_status"); t.emit("update", 19);
        t.emit("update", 1); t.emit("fetch_status"); t.emit("start", 100, 21);
        t.emit("fetch_status"); t.emit("set_scale", -1); t.emit("start", 100, 22);
        t.emit("update", 1); t.emit("start_remaining", 25, 100, 23);
        t.emit("fetch_status"); t.emit("update", 25); t.emit("fetch_status");
    }
    {
        Case t(out, "setters_status_and_stop");
        t.emit("start", 100, 24); t.emit("set_duration", 100); t.emit("fetch_status");
        t.emit("set_remaining", 100); t.emit("fetch_status"); t.emit("set_duration", 101);
        t.emit("set_frozen", 1); t.emit("fetch_status"); t.emit("set_remaining", 101);
        t.emit("fetch_status"); t.emit("set_remaining", 77); t.emit("fetch_status");
        t.emit("set_remaining", 0); t.emit("fetch_status"); t.emit("set_duration", 0);
        t.emit("fetch_status"); t.emit("set_duration", 10); t.emit("set_remaining", 20);
        t.emit("fetch_status"); t.emit("start_remaining", 20, 10, 25);
        t.emit("update", 0); t.emit("set_duration", 0); t.emit("fetch_status");
        t.emit("start", 100, 26); t.emit("update", 100); t.emit("update", 1999);
        t.emit("set_scale", 1); t.emit("update", 99); t.emit("update", 1);
        t.emit("fetch_status"); t.emit("start_remaining", 5, 10, 27);
        t.emit("fetch_status");
    }
    {
        Case t(out, "zero_duration");
        t.emit("start", 0, 28); t.emit("update", 0); t.emit("update", 1);
        t.emit("update", 1998); t.emit("update", 1); t.emit("update", 0);
        t.emit("set_duration", 1); t.emit("update", 1); t.emit("set_remaining", 0);
        t.emit("start_remaining", 1, 0, 29); t.emit("update", 0);
        t.emit("start_remaining", 0, 0, 30); t.emit("update", 2000);
    }
    {
        Case t(out, "int_min_narrow_domain");
        t.emit("set_scale", INT_MIN); t.emit("update", 0); t.emit("update", 0xffffffffu);
        t.emit("set_frozen", 1); t.emit("start", 100, 31); t.emit("update", 0);
        t.emit("update", 1); t.emit("set_frozen", 0); t.emit("update", 0);
        t.emit("update", 1); t.emit("update", 0xffffffffu); t.emit("fetch_status");
        t.emit("set_scale", 0); t.emit("update", 5); t.emit("fetch_status");
        t.emit("start_remaining", 50, 100, 32); t.emit("update", 0);
        t.emit("set_scale", INT_MIN + 1); t.emit("update", 2);
        t.emit("stop"); t.emit("set_scale", INT_MIN); t.emit("update", 2);
    }
    {
        Case t(out, "unsigned_wrap");
        t.emit("start", 0xffffffffu, 33); t.emit("update", 0xfffffffeu);
        t.emit("update", 2); t.emit("update", 0xffffffffu); t.emit("update", 1);
        t.emit("set_remaining", 0xfffffffeu); t.emit("set_duration", 0xfffffffeu);
        t.emit("update", 0xffffffffu); t.emit("set_scale", -65536);
        t.emit("update", 65536); t.emit("update", 65537);
        t.emit("start", 1, 34); t.emit("set_scale", -1); t.emit("update", 1);
        t.emit("update", 0xfffffffeu); t.emit("update", 4001); t.emit("update", 1999);
    }
}

static void numericCorpus(std::ostream& out) {
    constexpr std::array<uint32, 11> words{0, 1, 2, 1999, 2000, 2001, 65536,
                                        0x7fffffffu, 0x80000000u, 0xfffffffeu, 0xffffffffu};
    constexpr std::array<int32, 10> scales{INT_MIN, INT_MIN + 1, -65536, -3, -1,
                                         0, 1, 3, 65536, INT_MAX};
    unsigned index = 0;
    for (auto duration : words) for (auto scale : scales) for (auto diff : words) {
        Case t(out, "numeric_" + std::to_string(index), index % MirrorTimer::NUM_TIMERS);
        ++index;
        t.emit("start", duration, 0xffffffffu);
        t.emit("set_scale", scale);
        t.emit("fetch_status");
        t.emit("update", diff);
        t.emit("update", diff);
        t.emit("fetch_status");
    }
    index = 0;
    for (auto duration : words) for (auto remaining : words) {
        Case t(out, "remaining_" + std::to_string(index++));
        t.emit("start_remaining", remaining, duration, 0x80000000u);
        t.emit("fetch_status"); t.emit("update", 0); t.emit("update", 1);
        t.emit("set_remaining", remaining); t.emit("set_duration", duration);
        t.emit("update", 0xffffffffu); t.emit("fetch_status");
    }
    // Deterministic authored operation selection is independent of timer output.
    // uint32 PRNG arithmetic is deliberate wrapping, not a timer implementation.
    uint32 random = 0x4d495252u;
    for (unsigned type = 0; type < MirrorTimer::NUM_TIMERS; ++type) {
        Case t(out, "mixed_sequence_" + std::to_string(type), type);
        for (unsigned step = 0; step < 256; ++step) {
            random = random * 1664525u + 1013904223u;
            const uint32 a = words[(random >> 8) % words.size()];
            const uint32 b = words[(random >> 16) % words.size()];
            switch ((random >> 24) % 11) {
                case 0: t.emit("start", a, b); break;
                case 1: t.emit("start_remaining", a, b, random); break;
                case 2: t.emit("set_remaining", a); break;
                case 3: t.emit("set_duration", a); break;
                case 4: t.emit("set_scale", scales[(random >> 12) % scales.size()]); break;
                case 5: t.emit("set_frozen", random % 2); break;
                case 6: t.emit("stop"); break;
                case 7: t.emit("fetch_status"); break;
                case 8: t.emit("inspect"); break;
                default: t.emit("update", a); break;
            }
        }
    }
}

int main(int argc, char** argv) {
    require(argc == 2, "usage: oracle.test-fixture output.tsv");
    std::ofstream out(argv[1]);
    out << "# Exact pinned CMaNGOS MirrorTimer and ShortIntervalTimer; authored operations\n"
           "# undefined_abs is a harness domain rejection, never an executed source result\n"
           "# case\tstep\ttype\top\targ1\targ2\targ3\treturn"
           "\tbefore_scale\tbefore_spell_id\tbefore_status\tbefore_active\tbefore_frozen_raw"
           "\tbefore_tracker_interval\tbefore_tracker_current\tbefore_pulse_interval"
           "\tbefore_pulse_current\tbefore_remaining\tbefore_is_frozen\tbefore_is_regenerating"
           "\tafter_scale\tafter_spell_id\tafter_status\tafter_active\tafter_frozen_raw"
           "\tafter_tracker_interval\tafter_tracker_current\tafter_pulse_interval"
           "\tafter_pulse_current\tafter_remaining\tafter_is_frozen\tafter_is_regenerating\n";
    sequences(out);
    numericCorpus(out);
    require(bool(out), "write MirrorTimer fixture");
    std::cout << "Source observations: " << rows << " steps, " << cases << " cases, "
              << undefined_abs << " rejected undefined abs calls\n";
}
