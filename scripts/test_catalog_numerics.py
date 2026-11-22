"""Portable C numeric-prefix conversions and optional POSIX libc comparisons."""
import ctypes
import importlib.util
import os
from pathlib import Path
import random
import struct
import unittest
from unittest.mock import patch

from classic_export import catalog_spawns as numerics
from classic_export.core import ExportError


def libc_or_skip(test):
    if os.name != "posix":
        test.skipTest("POSIX libc comparison is unavailable")
    try:
        libc = ctypes.CDLL(None)
        libc.atoll.argtypes = [ctypes.c_char_p]
        libc.atoll.restype = ctypes.c_longlong
        libc.atof.argtypes = [ctypes.c_char_p]
        libc.atof.restype = ctypes.c_double
        return libc
    except (OSError, AttributeError, TypeError):
        test.skipTest("POSIX libc atof/atoll symbols are unavailable")


class CatalogNumericTests(unittest.TestCase):
    def test_import_never_loads_a_native_library(self):
        spec = importlib.util.spec_from_file_location("classic_export._numeric_import_probe", Path(numerics.__file__))
        module = importlib.util.module_from_spec(spec)
        with patch.object(ctypes, "CDLL", side_effect=TypeError("native library loading is unavailable")):
            spec.loader.exec_module(module)
            self.assertEqual(module.uint(b" -1tail"), 0xffffffff)
            self.assertEqual(module.floating(b" 1.5e1tail"), 15.0)

    def test_integer_prefix_and_glibc_saturation(self):
        for raw, expected in ((None,0),(b"",0),(b"+",0),(b"--1",0),(b"  -1tail",0xffffffff),
                (b"1e3",1),(b"123\0tail",123),(b"\x1c12",0),
                (b"9223372036854775807",0xffffffff),(b"9223372036854775808",0xffffffff),
                (b"-9223372036854775808",0),(b"-9223372036854775809",0),
                (b"999999999999999999999999999999999999",0xffffffff)):
            with self.subTest(raw=raw):
                self.assertEqual(numerics.uint(raw), expected)

    def test_decimal_prefix_exponent_and_signed_zero(self):
        for raw, expected in ((None,0.0),(b"",0.0),(b"-junk",0.0),(b"++1",0.0),
                (b"\t\n\v\f\r -12.5e+1tail",-125.0),(b".5x",.5),(b"1.e2x",100.0),
                (b"1e+tail",1.0),(b"1e-tail",1.0),(b".e3",0.0),(b"1\0e2",1.0)):
            with self.subTest(raw=raw):
                self.assertEqual(struct.pack("<f",numerics.floating(raw)),struct.pack("<f",expected))
        for raw in (b"-0",b"-.0",b"-0e999",b"-1e-9999"):
            self.assertEqual(struct.pack("<f",numerics.floating(raw)),b"\0\0\0\x80")

    def test_existing_limits_and_nondecimal_refusals_remain(self):
        for function in (numerics.uint,numerics.floating):
            with self.assertRaisesRegex(ExportError,"spawn.numeric_limit"):
                function(b"1"*128)
        for raw in (b"0x1p0",b" -0Xff",b"NaN",b"+INF",b"\t-infinity",b"++0x1"):
            with self.subTest(raw=raw), self.assertRaisesRegex(ExportError,"spawn.numeric_unsupported"):
                numerics.floating(raw)

    def test_integer_prefixes_match_posix_libc(self):
        libc = libc_or_skip(self)
        corpus = [b"",b"+",b"-",b"garbage",b"12\0ignored",b"0xFF",b"++1",b"--1",b"\x1c12"]
        for number in (-(1<<100),-(1<<63)-1,-(1<<63),-(1<<63)+1,-(1<<32)-1,-1,0,1,
                       (1<<32)-1,(1<<63)-1,1<<63,1<<100):
            for prefix in (b"",b" ",b"\t\n\v\f\r "):
                for tail in (b"",b"tail",b".9",b"e9",b"\0tail"):
                    corpus.append(prefix+str(number).encode()+tail)
        randomizer = random.Random(8128)
        for _ in range(1000):
            number = randomizer.randrange(-(1<<120),1<<120)
            corpus.append(randomizer.choice((b"",b"+",b" \t"))+str(number).encode()+b"tail")
        for raw in corpus:
            with self.subTest(raw=raw):
                self.assertEqual(numerics.uint(raw),libc.atoll(raw)&0xffffffff)

    def test_decimal_prefix_bits_match_posix_libc(self):
        libc = libc_or_skip(self)
        corpus = [b"",b".",b"-",b"-junk",b"++1",b"1e",b"1e+",b"1e-",b"1.e2tail",b".5junk",
            b"-0",b"-0.0e9999",b"1e309",b"-1e309",b"1e-9999",b"-1e-9999",
            b"2.2250738585072014e-308",b"2.2250738585072011e-308",b"4.9406564584124654e-324",
            b"2.4703282292062327e-324",b"1.7976931348623157e308",b"1.7976931348623159e308",
            b"1.1754943508222875e-38",b"1.401298464324817e-45",b"7.006492321624085e-46",
            b"3.4028234663852886e38",b"3.4028235677973366e38",
            b"1.0000000596046448",b"-1.0000000596046448",
            b"1.00000000000000011102230246251565404236316680908203125",
            b"0."+b"9"*100,b"123\0e999",b"\x1c12"]
        base = list(corpus)
        for raw in base:
            corpus.extend(prefix+raw+tail for prefix in (b" ",b"\t\n\v\f\r ") for tail in (b"tail",b"\0ignored"))
        randomizer = random.Random(18191)
        for _ in range(2000):
            digits = str(randomizer.randrange(10**60)).encode()
            split = randomizer.randrange(len(digits)+1)
            corpus.append(randomizer.choice((b"",b"-",b" +"))+digits[:split]+b"."+digits[split:]
                +b"e"+str(randomizer.randrange(-400,401)).encode()+randomizer.choice((b"",b"tail",b"e+")))
        for raw in corpus:
            with self.subTest(raw=raw):
                expected = struct.pack("<f",ctypes.c_float(libc.atof(raw)).value)
                self.assertEqual(struct.pack("<f",numerics.floating(raw)),expected)
