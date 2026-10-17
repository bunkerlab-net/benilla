class AnyAssistCreatureInRangeCheck
    {
        public:
            AnyAssistCreatureInRangeCheck(Unit* funit, Unit* enemy, float range)
                : i_funit(funit), i_enemy(enemy), i_range(range)
            {
            }
            WorldObject const& GetFocusObject() const { return *i_funit; }
            bool operator()(Creature* u);

        private:
            Unit* const i_funit;
            Unit* const i_enemy;
            float i_range;
    };
