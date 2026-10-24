// SPDX-License-Identifier: GPL-3.0-or-later
// Authored fixture driver. Selected original source is inserted by the generator.
#include <bit>
#include <cfenv>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <limits>
#include <string>
#include "loadlib/adt.h"
#include "dbcfile.h"
#include "GridMapDefines.h"
#include "mpq_libmpq.h"

static_assert(std::endian::native == std::endian::little);
static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559);
static_assert(sizeof(GridMapFileHeader) == 40);
static_assert(sizeof(GridMapAreaHeader) == 8);
static_assert(sizeof(GridMapHeightHeader) == 16);
static_assert(sizeof(GridMapLiquidHeader) == 16);
static_assert(sizeof(adt_MCLQ) == 812);
static_assert(sizeof(adt_MCNK) == 136);
static_assert(sizeof(adt_MCVT) == 588);
static_assert(sizeof(adt_MHDR) == 72);
static_assert(sizeof(adt_MCIN) == 4104);

// PINNED_GLOBALS
// PINNED_OPTIONS
// PINNED_DBC_READERS
// PINNED_CONVERTER

int main(int argc, char** argv) {
    if (argc < 3 || std::fesetround(FE_TONEAREST) != 0) return 2;
    fixture_root = argv[1];
    // Exact -f 0 configuration; minimum-height defaults remain source values.
    CONF_allow_float_to_int = false;
    if (ReadMapDBC() != 1 || map_ids[0].id != 1) return 3;
    ReadAreaTableDBC();
    ReadLiquidTypeTableDBC();
    for (int i = 2; i < argc; ++i) {
        std::string input = (std::filesystem::path(fixture_root) / (std::string(argv[i]) + ".adt")).string();
        std::string output = (std::filesystem::path(fixture_root) / (std::string(argv[i]) + ".map")).string();
        if (!ConvertADT(input.data(), output.data(), 32, 31)) return 4;
    }
    delete[] areas;
    delete[] LiqType;
    delete[] map_ids;
}
