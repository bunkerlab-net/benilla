// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Project contributors, Player::Create at 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// Upstream GPL-2.0-or-later; this modified standalone oracle GPL-3.0-or-later.
// No game/database rows. WITHOUT ANY WARRANTY. See PROVENANCE.md.
#include <cstdint>
#include <iostream>

int main() {
    uint32_t item_class, subclass, category, buy_count;
    int32_t stackable;
    while (std::cin >> item_class >> subclass >> category >> buy_count >> stackable) {
        int32_t count = buy_count;
        if (item_class == 0 && subclass == 5) { // ITEM_CLASS_CONSUMABLE / ITEM_SUBCLASS_FOOD
            switch (category) {
                case 11:
                    if (stackable > 4) count = 4;
                    break;
                case 59:
                    if (stackable > 2) count = 2;
                    break;
            }
        }
        std::cout << count << '\n';
    }
}
