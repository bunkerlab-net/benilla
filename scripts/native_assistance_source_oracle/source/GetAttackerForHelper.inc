        Unit* getAttackerForHelper()                        //< Return a possible enemy from this unit to help in combat
        {
            if (GetVictim() != nullptr)
                return GetVictim();

            if (!m_attackers.empty())
                return *(m_attackers.begin());

            return nullptr;
        }
