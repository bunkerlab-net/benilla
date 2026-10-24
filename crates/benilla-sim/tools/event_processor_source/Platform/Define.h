// SPDX-License-Identifier: GPL-3.0-or-later
// Authored standalone oracle shim: EventProcessor needs only these integer names.
// This is not the upstream Platform/Define.h and carries no platform assumptions.
#pragma once
#include <cstdint>
using uint64 = std::uint64_t;
using uint32 = std::uint32_t;
