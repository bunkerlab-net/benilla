// SPDX-License-Identifier: GPL-3.0-or-later
// Storage-only harness adaptation for the untouched TerrainBuilder translation
// unit. Its original uint32 read requires four-byte alignment, while its
// TU-local MAP_VERSION_MAGIC initially points at a one-byte-aligned literal.
#pragma once
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include "TerrainBuilder.h"
namespace benilla_original_terrain_storage {
    alignas(uint32) static char version_magic[5];
    struct InitializeVersionMagic {
        InitializeVersionMagic() {
            const char* original=MMAP::MAP_VERSION_MAGIC;
            if(std::memcmp(original,"z1.4",sizeof(version_magic))!=0)std::abort();
            std::memcpy(version_magic,original,sizeof(version_magic));
            if(std::memcmp(version_magic,original,sizeof(version_magic))!=0 ||
               reinterpret_cast<uintptr_t>(version_magic)%alignof(uint32))std::abort();
            MMAP::MAP_VERSION_MAGIC=version_magic;
        }
    };
    static InitializeVersionMagic initialize_version_magic;
}
