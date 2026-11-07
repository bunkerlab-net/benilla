template<class Check>
    struct CreatureListSearcher
    {
        CreatureList& i_objects;
        Check& i_check;

        CreatureListSearcher(CreatureList& objects, Check& check) : i_objects(objects), i_check(check) {}

        void Visit(CreatureMapType& m);

        template<class NOT_INTERESTED> void Visit(GridRefManager<NOT_INTERESTED>&) {}
    };
