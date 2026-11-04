# SPDX-License-Identifier: LGPL-2.1-or-later
# Copyright (C) 2018-2025 Free Software Foundation, Inc.
# Bounded Python translation of the existing source_trig/core.rs contract.
# See crates/benilla-sim/src/source_trig/GLIBC-{NOTICE,LICENSE}.txt.
import math
import struct
from classic_export.core import check, ExportError


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def double(value):
    return struct.unpack('<d', struct.pack('<Q', value))[0]


def sin_cos_degrees(angle):
    check(math.isfinite(angle) and -1800 <= angle <= 1800, 'source_location.rotation_domain')
    radians = f32(double(0x400921fb54442d28) * angle / 180.0)
    raw = bits(radians)
    top = (raw >> 20) & 0x7ff
    cos = list(map(double, (0x3ff0000000000000, 0xbfdffffffd0c621c,
                          0x3fa55553e1068f19, 0xbf56c087e89a359d, 0x3ef99343027bf8c3)))
    sin = list(map(double, (0xbfc555545995a603, 0x3f81107605230bc4, 0xbf2994eb3774cf24)))

    def polynomial(x, squared, quadrant, negate):
        if quadrant & 1 == 0:
            cubed = x * squared
            s1 = sin[1] + squared * sin[2]
            seventh = cubed * squared
            s = x + cubed * sin[0]
            return f32(s + seventh * s1)
        c = [-v for v in cos] if negate else cos
        fourth = squared * squared
        c2 = c[3] + squared * c[4]
        c1 = c[0] + squared * c[1]
        sixth = fourth * squared
        result = c1 + fourth * c[2]
        return f32(result + sixth * c2)

    if top < 0x3f4:
        if top < 0x398:
            return radians, 1.0
        return polynomial(radians, radians * radians, 0, False), polynomial(radians, radians * radians, 1, False)
    quadrant = (math.trunc(radians * double(0x41645f306dc9c883)) + 0x800000) >> 24
    reduced = radians - quadrant * double(0x3ff921fb54442d18)
    sign = (1, -1, -1, 1)[quadrant & 3]
    return (polynomial(reduced * sign, reduced * reduced, quadrant, quadrant & 2 != 0),
            polynomial(reduced * sign, reduced * reduced, quadrant ^ 1, quadrant & 2 != 0))


def yaw_model_point(point, position, rotation, scale):
    # ModelInstance.cpp:31,81. Other transforms need the full existing owner.
    check(rotation[0] == 0 and rotation[2] == 0 and math.isfinite(scale) and 0.001 <= scale <= 1000,
          'source_location.wmo_transform_requires_owner')
    sine, cosine = sin_cos_degrees(rotation[1])
    determinant = f32(f32(cosine * cosine) - f32(-sine * sine))
    inverse = f32(1.0 / determinant)
    c, s = f32(cosine * inverse), f32(sine * inverse)
    p = [f32(a - b) for a, b in zip(point, position)]
    inverse_scale = f32(1.0 / scale)
    try:
        out = [f32(f32(f32(c * p[0]) + f32(s * p[1])) * inverse_scale),
               f32(f32(f32(-s * p[0]) + f32(c * p[1])) * inverse_scale),
               f32(f32(f32(determinant * inverse) * p[2]) * inverse_scale)]
    except OverflowError as error:
        raise ExportError('source_location.wmo_transform_requires_owner') from error
    check(all(math.isfinite(v) for v in out), 'source_location.wmo_transform_requires_owner')
    return out
