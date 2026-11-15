// SPDX-License-Identifier: GPL-3.0-or-later
// Harness only. The generator inserts unchanged pinned source implementations.
static_assert(sizeof(decltype(atan2(0.0f,0.0f))) == sizeof(float), "source atan2 overload");
static uint32 word(float f) { uint32 out; std::memcpy(&out, &f, 4); return out; }
static float real(uint32 b) { float out; std::memcpy(&out, &b, 4); return out; }
static void point(Vector3 const& p) { std::cout << ' ' << word(p.x) << ' ' << word(p.y) << ' ' << word(p.z); }
static void state(std::string const& name, int sample, Movement::MoveSpline const& s, std::vector<uint32> const& events) {
    std::cout << name << ' ' << sample << ' ' << s.GetId() << ' ' << s.splineflags.raw() << ' ' << word(s.Speed())
              << ' ' << s.timePassed() << ' ' << s.GetRawPathIndex() << ' ' << s.currentPathIdx()
              << ' ' << s.Initialized() << ' ' << s.Finalized();
    if (s.Initialized()) std::cout << ' ' << s.Duration() << ' ' << word(s.GetElapsedValue());
    else std::cout << " 0 0";
    point(s.CurrentDestination()); point(s.FinalDestination());
    if (s.Initialized()) { auto p = s.ComputePosition(); point(p); std::cout << ' ' << word(p.orientation); }
    else std::cout << " 0 0 0 0";
    auto const& spline = s._Spline();
    std::cout << ' ' << spline.first() << ' ' << spline.last() << ' ' << spline.getPointCount();
    for (auto const& p : spline.getPoints()) point(p);
    std::cout << ' ' << (s.Initialized() ? spline.last()+1 : 0);
    if (s.Initialized()) for (int i=0; i<=spline.last(); ++i) std::cout << ' ' << spline.length(i);
    std::cout << ' ' << events.size();
    for (auto e : events) std::cout << ' ' << e;
    std::cout << '\n';
}
int main(int argc, char** argv) {
    if (std::fegetround()!=FE_TONEAREST || (_mm_getcsr() & ((1u<<15)|(1u<<6)))) return 2;
    if (argc == 2 && std::string(argv[1]) == "--init-arithmetic") {
        for (uint32 v : {0u,0x80000000u,0xbf800000u,0x40c90fdbu,0x50490fdau,0xd0490fdau}) {
            std::cout << "wrap " << v << ' ' << word(G3D::wrap(real(v),0.f,float(G3D::twoPi()))) << '\n';
        }
        for (uint32 v : {0u,0x3f800000u,0xbf800000u,0x4dffffffu,0xce000000u}) {
            PackedOracle b; b.appendPackXYZ(real(v),0.f,0.f);
            std::cout << "pack " << v << ' ' << b.result << '\n';
        }
        for (auto xy : {std::pair<float,float>{4,6},{4,-2},{1,2}}) {
            UnitFacingOracle u{1,2,0.75f};
            std::cout << "orientation " << word(u.x) << ' ' << word(u.y) << ' ' << word(u.o)
                      << ' ' << word(xy.first) << ' ' << word(xy.second) << ' ' << word(u.run(xy.first,xy.second)) << '\n';
        }
        return 0;
    }
    if (argc == 3 && std::string(argv[1]) == "--ub") {
        int which = std::stoi(argv[2]);
        if (which == 9 || which == 12) {
            std::cout << G3D::wrap(real(which == 9 ? 0x50490fdbu : 0x7fc00000u),0.f,float(G3D::twoPi()));
            return 0;
        }
        if (which == 10 || which == 11) {
            PackedOracle b; b.appendPackXYZ(real(which == 10 ? 0x4e000000u : 0xce000001u),0.f,0.f);
            std::cout << b.result; return 0;
        }
        Movement::MoveSplineInitArgs args;
        args.path = {Vector3(0,0,0), Vector3(1,0,0)};
        args.velocity = 1;
        if (which == 0) args.velocity = real(1); // overflowing float duration
        if (which == 1) args.velocity = real(0x7fc00000); // NaN duration
        if (which == 2) args.path.back().x = real(0x7f800000); // infinite duration
        if (which == 3) { args.flags.falling = true; args.path.back() = Vector3(0,0,-2.0e8f); }
        if (which == 4) args.path_Idx_offset = INT32_MAX;
        if (which == 5) {
            args.path.back().x = 2147483648.0f;
            Movement::Spline<int32> spline;
            spline.init_spline(args.path.data(),2,Movement::SplineBase::ModeLinear);
            spline.initLengths();
            return 0;
        }
        if (which == 6) args.path_Idx_offset = INT32_MAX - 1;
        if (which == 7) args.flags.cyclic = true;
        if (which == 8) {
            args.flags.cyclic = args.flags.enter_cycle = true;
            args.path.shrink_to_fit();
        }
        Movement::MoveSpline s; s.Initialize(args);
        if (which == 6 || which == 7) {
            args.flags.done = true;
            s.Initialize(args);
            std::cout << s.currentPathIdx();
        }
        if (which == 4) std::cout << s.currentPathIdx();
        return 0;
    }
    std::string kind, name;
    while (std::cin >> kind >> name) {
        uint32 flags, velocity, id, count, x,y,z; int32 offset;
        std::cin >> flags >> velocity >> id >> offset >> x >> y >> z >> count;
        Movement::MoveSplineInitArgs args; args.flags = Movement::MoveSplineFlag(flags);
        args.velocity=real(velocity); args.splineId=id; args.path_Idx_offset=offset;
        args.facing.f={real(x), real(y), real(z)};
        for (uint32 i=0; i<count; ++i) { std::cin>>x>>y>>z; args.path.emplace_back(real(x),real(y),real(z)); }
        if (kind == "M") {
            Movement::MoveSpline s;
            state(name,-1,s,{});
            s.Initialize(args); state(name,0,s,{});
            uint32 steps; std::cin >> steps;
            for (uint32 i=1; i<=steps; ++i) {
                uint32 op, diff; std::cin >> op >> diff;
                std::vector<uint32> events;
                if (op == 0) { auto handler=[&](Movement::MoveSpline::UpdateResult e) { events.push_back(e); }; s.updateState(diff,handler); }
                if (op == 1) s._Interrupt();
                if (op == 2) s._Finalize();
                if (op == 3) { args.flags.done=true; s.Initialize(args); }
                state(name,i,s,events);
            }
        } else {
            Movement::Spline<int32> s;
            auto mode = flags & 0x200 ? Movement::SplineBase::ModeCatmullrom : Movement::SplineBase::ModeLinear;
            if (flags & 0x100000) s.init_cyclic_spline(args.path.data(),count,mode,flags & 0x200000 ? 1 : 0);
            else s.init_spline(args.path.data(),count,mode);
            s.initLengths();
            uint32 samples; std::cin >> samples;
            for (uint32 sample=0; sample<samples; ++sample) {
                uint32 t; std::cin >> t;
                int index; float u; s.computeIndex(real(t),index,u);
                std::cout << name << ' ' << sample << ' ' << index << ' ' << word(u) << ' ' << s.first() << ' ' << s.last();
                for (int i=0; i<=s.last(); ++i) std::cout << ' ' << s.length(i);
                for (int i=s.first(); i<s.last(); ++i) {
                    Vector3 p,d; s.evaluate_percent(i,real(t),p); s.evaluate_derivative(i,real(t),d);
                    std::cout << ' ' << word(s.SegLength(i)); point(p); point(d);
                }
                std::cout << '\n';
            }
        }
    }
}
