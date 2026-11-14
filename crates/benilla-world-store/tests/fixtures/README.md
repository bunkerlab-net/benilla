# Legacy synthetic fixture

`runtime-v1.checkpoint` was generated with the original GPL-3.0-or-later camp
fixture and runtime snapshot format 1, before the loot/item-instance schema
upgrade. It is a fresh tick-zero synthetic world: world ID `[7; 16]`, seed 42,
owner ID `[1; 16]`, character ID `[2; 16]`, original camp rules. These are public
test identifiers, not credentials or a player save. The enclosing store format
is 1. The regression proves a genuine old schema fails closed under runtime v2
without rewriting either world identity or progression.
