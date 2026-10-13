"""Bounded, read-only build-5875-layout WDBC metadata, including packed tables.

Layouts: pinned CMaNGOS DBCStructure.h; packed sizes: benilla-formats characters.
A WDBC has no build stamp. Layout agreement and hashes do not prove authenticity.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

from dataclasses import dataclass
from decimal import Decimal
from fractions import Fraction
import hashlib
import os
from pathlib import Path
import stat
import struct
from typing import Any

from classic_export.core import canonical, check, decode_json, ExportError

MAX_FILE_BYTES = 64 * 1024 * 1024
MAX_ALL_BYTES = 192 * 1024 * 1024
MAX_RECORDS = 100_000
MAX_STRING_BYTES = 16_384
MAX_DECODED_TEXT_BYTES = 16 * 1024 * 1024
MAX_DISTINCT_STRINGS = 262_144
MAX_EXTRACTION_BYTES = 64 * 1024

@dataclass(frozen=True)
class Layout:
    fields: int
    size: int
    strings: tuple[int, ...] = ()

LAYOUTS = {
    "ChrRaces": Layout(29, 116, (6, 15, *range(17, 25), 26, 27, 28)),
    "ChrClasses": Layout(17, 68, (4, *range(5, 13), 14)),
    "CharBaseInfo": Layout(2, 2),
    "CharStartOutfit": Layout(41, 152),
    "Map": Layout(42, 168, (1, *range(4, 12), *range(20, 28), *range(29, 37))),
    "Faction": Layout(37, 148, (*range(19, 27), *range(28, 36))),
    "FactionTemplate": Layout(14, 56),
    "Spell": Layout(173, 692, (*range(120, 128), *range(129, 137),
                                *range(138, 146), *range(147, 155))),
    "SpellCastTimes": Layout(4, 16),
    "SpellCategory": Layout(2, 8),
    "SpellRange": Layout(22, 88, (*range(4, 12), *range(13, 21))),
    "SpellDuration": Layout(4, 16),
    "SpellRadius": Layout(4, 16),
    "SpellFocusObject": Layout(10, 40, tuple(range(1, 9))),
    "SkillLine": Layout(22, 88, (*range(3, 11), *range(12, 20))),
    "SkillRaceClassInfo": Layout(8, 32),
    "SkillTiers": Layout(33, 132),
    "SkillLineAbility": Layout(15, 60),
}

def signed(word: int) -> int:
    return word if word < 0x80000000 else word - 0x100000000


def binary32_bits(decimal: str) -> int:
    """Exact decimal -> IEEE binary32, round-to-nearest ties-to-even, no host float.

    Kept as bits, not used as runtime authority. Underflow to subnormal/zero is
    supported; nonfinite/overflow is rejected. Source normalization already erased -0.
    """
    check(type(decimal) is str and len(decimal) <= 128, "numeric.decimal_domain")
    decoded = Decimal(decimal)
    check(decoded.is_finite() and -200 <= decoded.as_tuple().exponent <= 200,
          "numeric.decimal_domain")
    value = Fraction(decoded)
    sign = 0x80000000 if value < 0 else 0
    value = abs(value)
    if not value:
        return 0
    n, d = value.numerator, value.denominator
    exp = n.bit_length() - d.bit_length()
    if (n < d << exp) if exp >= 0 else (n << -exp < d):
        exp -= 1
    shift = 23 - max(exp, -126)
    if shift >= 0:
        n <<= shift
    else:
        d <<= -shift
    q, r = divmod(n, d)
    if 2*r > d or (2*r == d and q & 1):
        q += 1
    if exp < -126:
        return sign | q
    if q == 1 << 24:
        q >>= 1
        exp += 1
    check(exp <= 127, "numeric.binary32_overflow")
    return sign | ((exp + 127) << 23) | (q - (1 << 23))


def finite_float(word: int, code: str) -> int:
    check(word & 0x7f800000 != 0x7f800000, code)
    return word


@dataclass(frozen=True)
class DbcRow:
    table: str
    key: tuple[int, ...]
    words: tuple[int, ...]
    text: dict[int, str]
    record_sha256: str

    def raw(self) -> dict[str, Any]:
        return {"key": list(self.key), "words": list(self.words),
                "strings": {str(k): v for k, v in self.text.items()},
                "record_sha256": self.record_sha256}

@dataclass(frozen=True)
class DbcTable:
    name: str
    rows: tuple[DbcRow, ...]
    by_id: dict[int, DbcRow]
    sha256: str
    byte_count: int

    def manifest(self) -> dict[str, Any]:
        layout = LAYOUTS[self.name]
        return {"file": self.name + ".dbc", "sha256": self.sha256,
                "bytes": self.byte_count, "records": len(self.rows),
                "fields": layout.fields, "record_bytes": layout.size}


def parse_table(name: str, data: bytes) -> DbcTable:
    check(name in LAYOUTS, "dbc.unknown_table")
    check(20 <= len(data) <= MAX_FILE_BYTES, "dbc.byte_limit_or_truncated_header")
    magic, count, fields, size, string_size = struct.unpack_from("<4s4I", data)
    layout = LAYOUTS[name]
    check(magic == b"WDBC", "dbc.magic")
    check((fields, size) == (layout.fields, layout.size), "dbc.layout")
    check(count <= MAX_RECORDS, "dbc.record_limit")
    end = 20 + count*size
    check(end + string_size == len(data), "dbc.exact_length")
    strings = data[end:]
    check(bool(strings) and strings[0] == 0 and strings[-1] == 0, "dbc.string_block")
    rows, ids, keys = [], {}, set()
    text_cache: dict[int, str] = {}
    decoded_text_bytes = 0
    for index in range(count):
        record = data[20+index*size:20+(index+1)*size]
        if name == "CharBaseInfo":
            words = tuple(record)
            key = words
        else:
            words = struct.unpack("<" + "I"*(size//4), record)
            key = (words[0],)
        check(key not in keys, "dbc.duplicate_key")
        keys.add(key)
        text = {}
        for column in layout.strings:
            offset = words[column]
            if offset in text_cache:
                text[column] = text_cache[offset]
                continue
            check(offset < len(strings), "dbc.string_offset")
            limit = strings.find(b"\0", offset, offset+MAX_STRING_BYTES+1)
            check(limit >= 0, "dbc.string_termination_or_limit")
            decoded_text_bytes += limit-offset
            check(decoded_text_bytes <= MAX_DECODED_TEXT_BYTES
                  and len(text_cache) < MAX_DISTINCT_STRINGS, "dbc.decoded_text_limit")
            try:
                text[column] = text_cache[offset] = strings[offset:limit].decode("utf-8", errors="strict")
            except UnicodeError:
                raise ExportError("dbc.string_encoding") from None
        row = DbcRow(name, key, words, text, hashlib.sha256(record).hexdigest())
        rows.append(row)
        if name != "CharBaseInfo":
            ids[words[0]] = row
    return DbcTable(name, tuple(rows), ids, hashlib.sha256(data).hexdigest(), len(data))


def load_directory(directory: Path) -> tuple[dict[str, DbcTable], list[dict[str, Any]]]:
    """Only fixed basenames. Never follows paths or executable content from metadata."""
    tables, errors, aggregate = {}, [], 0
    for name in sorted(LAYOUTS):
        observed = {}
        try:
            data = read_regular(directory / (name + ".dbc"), MAX_FILE_BYTES)
            aggregate += len(data)
            check(len(data) <= MAX_FILE_BYTES, "dbc.input_byte_limit")
            check(aggregate <= MAX_ALL_BYTES, "dbc.aggregate_byte_limit")
            observed = {"file_sha256": hashlib.sha256(data).hexdigest(), "file_bytes": len(data)}
            tables[name] = parse_table(name, data)
        except FileNotFoundError:
            errors.append({"table": name, "key": [], "reason": "dbc.file_missing"})
        except OSError:
            errors.append({"table": name, "key": [], "reason": "dbc.file_unreadable"})
        except ExportError as error:
            if str(error) in {"dbc.input_byte_limit", "dbc.aggregate_byte_limit"}:
                raise
            errors.append({"table": name, "key": [], "reason": str(error), **observed})
    return tables, errors


def read_regular(path: Path, limit: int) -> bytes:
    """Never wait on a pipe/device; reject symlinks and recheck the opened inode.

    NONBLOCK closes the lstat/open FIFO race on POSIX. Files remain read-only.
    The compiler has no reason to accept a device or streaming source as a DBC.
    """
    check(stat.S_ISREG(path.lstat().st_mode), "input.regular_file_required")
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NONBLOCK", 0)
                         | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_CLOEXEC", 0))
    try:
        check(stat.S_ISREG(os.fstat(descriptor).st_mode), "input.regular_file_required")
        with os.fdopen(descriptor, "rb", closefd=False) as handle:
            return handle.read(limit+1)
    finally:
        os.close(descriptor)


def load_extraction_manifest(directory: Path, tables: dict[str, DbcTable]) -> dict[str, Any]:
    """Require the helper's completed bundle and bind exact metadata bytes.

    The marker is integrity metadata, not a signature or proof of authentic build
    or patch-chain provenance. No imported filename or archive label is opened.
    """
    try:
        data = read_regular(directory / "extraction.json", MAX_EXTRACTION_BYTES)
    except (FileNotFoundError, OSError):
        raise ExportError("extraction.manifest_missing_or_unreadable") from None
    value = decode_json(data, MAX_EXTRACTION_BYTES)
    check(type(value) is dict and set(value) == {"format", "chain_policy", "client_build_claim",
          "client_build_authenticity_verified", "files"}, "extraction.manifest_fields")
    check(value["format"] == "benilla-native-dbc-extraction-v1"
          and value["chain_policy"] == "benilla-formats-vanilla-chain"
          and type(value["client_build_claim"]) is int and value["client_build_claim"] == 5875
          and value["client_build_authenticity_verified"] is False, "extraction.manifest_profile")
    check(set(tables) == set(LAYOUTS), "extraction.complete_valid_dbc_set_required")
    check(type(value["files"]) is list and len(value["files"]) == len(LAYOUTS),
          "extraction.manifest_file_set")
    by_file = {table.name + ".dbc": table.manifest() for table in tables.values()}
    seen = set()
    for row in value["files"]:
        check(type(row) is dict and set(row) == {"file", "winning_archive", "sha256", "bytes",
              "records", "fields", "record_bytes"}, "extraction.manifest_file_fields")
        name = row["file"]
        check(type(name) is str and name in by_file and name not in seen,
              "extraction.manifest_file_set")
        seen.add(name)
        archive = row["winning_archive"]
        check(type(archive) is str and 0 < len(archive) <= 255 and archive not in {".", ".."}
              and not any(c in archive for c in ("/", "\\", "\0")), "extraction.archive_label")
        observed = {key: val for key, val in row.items() if key != "winning_archive"}
        check(canonical(observed) == canonical(by_file[name]), "extraction.dbc_manifest_mismatch")
    return {"file": "extraction.json", "sha256": hashlib.sha256(data).hexdigest(),
            "bytes": len(data), "integrity_verified": True, "authenticity_verified": False,
            "metadata": value}
