void Creature::CallAssistanceOnPull(Unit* enemy, GuidVector const& receiverList)
{
    if (enemy && !HasCharmer())
    {
        MANGOS_ASSERT(AI());

        for (ObjectGuid receiverGuid : receiverList)
        {
            if (Creature* receiver = GetMap()->GetAnyTypeCreature(receiverGuid))
            {
                receiver->AI()->ReceiveAIEvent(AI_EVENT_CALL_ASSISTANCE, this, enemy, 0);
                receiver->AI()->HandleAssistanceCall(this, enemy); // Special case for type 0 (call-assistance)
            }
        }
    }
}
