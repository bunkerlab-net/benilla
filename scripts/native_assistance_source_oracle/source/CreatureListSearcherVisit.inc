template<class Check>
void MaNGOS::CreatureListSearcher<Check>::Visit(CreatureMapType& m)
{
    for (CreatureMapType::iterator itr = m.begin(); itr != m.end(); ++itr)
        if (i_check(itr->getSource()))
            i_objects.push_back(itr->getSource());
}
