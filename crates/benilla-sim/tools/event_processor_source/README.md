# Exact pinned EventProcessor oracle source

EventProcessor.cpp and EventProcessor.h are byte-identical copies of
CMaNGOS Classic revision `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`src/framework/Utilities/`. Their complete original GPL-2.0-or-later notices
remain in place. CMaNGOS authorship and license texts are under
[../../licenses](../../licenses).

`Platform/Define.h` is an authored standalone shim defining only uint32 and
uint64. The oracle compiles the unedited source implementation, not a second
translation of its methods. Its public test subclass only exposes clock and
aborting for observations. No CMaNGOS database, Playerbots code or game assets
are present.
