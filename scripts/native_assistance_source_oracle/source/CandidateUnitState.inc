enum UnitState
{
    // persistent state (applied by aura/etc until expire)
    UNIT_STAT_MELEE_ATTACKING = 0x00000001,                 // unit is melee attacking someone Unit::Attack
    UNIT_STAT_LOGOUT_TIMER    = 0x00000002,                 // Unit is logging out
    UNIT_STAT_FEIGN_DEATH     = 0x00000004,                 // Unit::SetFeignDeath - a successful feign death is currently active
    UNIT_STAT_STUNNED         = 0x00000008,                 // Aura::HandleAuraModStun
    UNIT_STAT_ROOT            = 0x00000010,                 // Aura::HandleAuraModRoot
    UNIT_STAT_ISOLATED        = 0x00000020,                 // area auras do not affect other players, Aura::HandleAuraModSchoolImmunity
    UNIT_STAT_POSSESSED       = 0x00000040,                 // Aura::HandleAuraModPossess (duplicates UNIT_FLAG_POSSESSED)

    // movement generators begin:
    UNIT_STAT_TAXI_FLIGHT     = 0x00000080,                 // TaxiMovementGenerator on stack
    UNIT_STAT_PROPELLED       = 0x00000100,                 // EffectMovementGenerator on stack
    UNIT_STAT_PANIC           = 0x00000200,                 // PanicMovementGenerator on stack
    UNIT_STAT_RETREATING      = 0x00000400,                 // RetreatMovementGenerator on stack
    UNIT_STAT_DISTRACTED      = 0x00000800,                 // DistractedMovementGenerator on stack
    UNIT_STAT_STAY            = 0x00001000,                 // StayMovementGenerator on stack
    UNIT_STAT_CONFUSED        = 0x00002000,                 // ConfusedMovementGenerator on stack
    UNIT_STAT_CONFUSED_MOVE   = 0x00004000,                 // ^ - spline dispatched
    UNIT_STAT_ROAMING         = 0x00008000,                 // Point/Retreat/Stay/Wander/Waypoint/Effect MovementGenerator on stack
    UNIT_STAT_ROAMING_MOVE    = 0x00010000,                 // ^ - spline dispatched
    UNIT_STAT_CHASE           = 0x00020000,                 // ChaseMovementGenerator on stack
    UNIT_STAT_CHASE_MOVE      = 0x00040000,                 // ^ - spline dispatched
    UNIT_STAT_FOLLOW          = 0x00080000,                 // FollowMovementGenerator on stack
    UNIT_STAT_FOLLOW_MOVE     = 0x00100000,                 // ^ - spline dispatched
    UNIT_STAT_FLEEING         = 0x00200000,                 // FleeingMovementGenerator/PanicMovementGenerator on stack
    UNIT_STAT_FLEEING_MOVE    = 0x00400000,                 // ^ - spline dispatched
    // movemement generators end

    UNIT_STAT_CHANNELING      = 0x00800000,

    // High-Level states (usually only with Creatures)
    UNIT_STAT_NO_COMBAT_MOVEMENT    = 0x01000000,           // Combat Movement for MoveChase stopped
    UNIT_STAT_NO_FOLLOW_MOVEMENT    = 0x02000000,
    UNIT_STAT_RUNNING               = 0x04000000,           // SetRun for waypoints and such
    UNIT_STAT_WAYPOINT_PAUSED       = 0x08000000,           // Waypoint-Movement paused genericly (ie by script)

    UNIT_STAT_IGNORE_PATHFINDING    = 0x10000000,           // do not use pathfinding in any MovementGenerator
    UNIT_STAT_AI_ROOT               = 0x20000000,           // root but from AI script

    // masks (only for check)

    // can't move currently
    UNIT_STAT_CAN_NOT_MOVE    = UNIT_STAT_ROOT | UNIT_STAT_STUNNED | UNIT_STAT_LOGOUT_TIMER | UNIT_STAT_FEIGN_DEATH | UNIT_STAT_AI_ROOT,

    // stay by different reasons
    UNIT_STAT_NOT_MOVE        = UNIT_STAT_ROOT | UNIT_STAT_STUNNED | UNIT_STAT_LOGOUT_TIMER | UNIT_STAT_FEIGN_DEATH |
                                UNIT_STAT_DISTRACTED | UNIT_STAT_AI_ROOT,

    // stay or scripted movement for effect( = in player case you can't move by client command)
    UNIT_STAT_NO_FREE_MOVE    = UNIT_STAT_ROOT | UNIT_STAT_STUNNED | UNIT_STAT_LOGOUT_TIMER | UNIT_STAT_FEIGN_DEATH |
                                UNIT_STAT_TAXI_FLIGHT |
                                UNIT_STAT_CONFUSED | UNIT_STAT_FLEEING | UNIT_STAT_PROPELLED | UNIT_STAT_AI_ROOT,

    // not react at move in sight or other
    UNIT_STAT_CAN_NOT_REACT   = UNIT_STAT_STUNNED | UNIT_STAT_LOGOUT_TIMER | UNIT_STAT_FEIGN_DEATH |
                                UNIT_STAT_CONFUSED | UNIT_STAT_FLEEING | UNIT_STAT_RETREATING,

    // AI disabled by some reason
    UNIT_STAT_LOST_CONTROL    = UNIT_STAT_CONFUSED | UNIT_STAT_FLEEING | UNIT_STAT_POSSESSED,

    // above 2 state cases
    UNIT_STAT_CAN_NOT_REACT_OR_LOST_CONTROL  = UNIT_STAT_CAN_NOT_REACT | UNIT_STAT_LOST_CONTROL,

    // masks (for check or reset)

    // for real move using movegen check and stop (except unstoppable flight)
    UNIT_STAT_MOVING          = UNIT_STAT_ROAMING_MOVE | UNIT_STAT_CHASE_MOVE | UNIT_STAT_FOLLOW_MOVE | UNIT_STAT_FLEEING_MOVE,

    UNIT_STAT_RUNNING_STATE   = UNIT_STAT_CHASE_MOVE | UNIT_STAT_FLEEING_MOVE | UNIT_STAT_RUNNING,

    UNIT_STAT_ALL_STATE       = 0xFFFFFFFF,
    UNIT_STAT_ALL_DYN_STATES  = UNIT_STAT_ALL_STATE & ~(UNIT_STAT_NO_COMBAT_MOVEMENT | UNIT_STAT_RUNNING | UNIT_STAT_WAYPOINT_PAUSED | UNIT_STAT_IGNORE_PATHFINDING),
};
