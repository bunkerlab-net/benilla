class UnitLambdaEvent : public BasicEvent
{
    public:
    UnitLambdaEvent(Unit& owner, std::function<void(Unit&)> const& func) : m_owner(owner), m_func(func) {}

    virtual bool Execute(uint64 /*e_time*/, uint32 /*p_time*/)
    {
        m_func(m_owner);
        return true;
    }

    Unit& m_owner;
    std::function<void(Unit&)> m_func;
};
