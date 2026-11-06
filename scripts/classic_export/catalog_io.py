"""Immutable catalog-v3 frames and bounded indexed access; no gameplay admission.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

import hashlib
from collections import OrderedDict
import os
from pathlib import Path
import sqlite3
import stat
import struct
import sys
import tempfile
import time

from .core import canonical, check, decode_json, digest
from . import catalog_schema as schema_contract
from .catalog_paths import Directory, read_at

PAGE = 4096
CHUNK = 1048576
FRAME = 65536
BATCH = 256
CACHE = 8388608
DECODED = 33554432
MAX_ROWS = 2000000
MAX_INDEX = 134217728
MAX_RAW = 1073741824
LIMITS = {"manifest": CHUNK, "chunk": CHUNK, "frame": FRAME, "page": PAGE,
          "batch_rows": BATCH, "batch_bytes": CHUNK, "cache": CACHE,
          "decoded_batch": DECODED, "rows": MAX_ROWS, "index": MAX_INDEX,
          "raw": MAX_RAW, "depth": 8, "relations": 512, "columns": 8192,
          "dbc_total": 134217728, "dbc_file": 67108864}
HEADER = struct.Struct("<8sHHIQQ")
DATA_HEADER = struct.Struct("<8sHHIQII")
DESC = struct.Struct("<32sQII")
KEY = struct.Struct("<IIQ64s")
CHILD = struct.Struct("<32sQ")
ROW = struct.Struct("<IQHH")
MAGIC = b"BNCAT3\0\0"
INDEX_MAGIC = b"BNCIDX3\0"
FORMAT = "benilla-classic-catalog-v3"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def rss_kib():
    # VmHWM measures this address space; getrusage can retain the launcher's pre-exec peak.
    try:
        for line in Path("/proc/self/status").read_text().splitlines():
            if line.startswith("VmHWM:"):
                return int(line.split()[1])
    except (OSError, ValueError, IndexError):
        return None
    return None


def json_budget(raw):
    """Count values before json.loads allocates metadata containers."""
    at, nodes = 0, 0
    def space():
        nonlocal at
        while at < len(raw) and raw[at] in b" \t\r\n":
            at += 1
    def string():
        nonlocal at
        check(at < len(raw) and raw[at] == 34,"json.invalid")
        at += 1
        while at < len(raw):
            if raw[at] == 34:
                at += 1
                return
            at += 2 if raw[at] == 92 else 1
        check(False,"json.invalid")
    def value(depth):
        nonlocal at,nodes
        nodes += 1
        check(depth <= 32 and nodes <= 100000,"catalog.json_limit")
        space()
        check(at < len(raw),"json.invalid")
        token = raw[at]
        if token in (123,91):
            closing = 125 if token == 123 else 93
            at += 1
            space()
            if at < len(raw) and raw[at] == closing:
                at += 1
                return
            while True:
                if token == 123:
                    space()
                    string()
                    space()
                    check(at < len(raw) and raw[at] == 58,"json.invalid")
                    at += 1
                value(depth+1)
                space()
                check(at < len(raw),"json.invalid")
                if raw[at] == closing:
                    at += 1
                    return
                check(raw[at] == 44,"json.invalid")
                at += 1
        elif token == 34:
            string()
        else:
            start = at
            while at < len(raw) and raw[at] not in b",]} \t\r\n":
                at += 1
            check(at > start,"json.invalid")
            check(raw[start:at] not in (b"NaN",b"Infinity",b"-Infinity"),"json.invalid")
    value(0)
    space()
    check(at == len(raw),"json.invalid")


def fields(value, keys, diagnostic):
    check(type(value) is dict and set(value) == set(keys), diagnostic)


def u64(value):
    check(type(value) is int and 0 <= value <= (1 << 64)-1, "catalog.overflow")
    return value


def plus(a, b):
    return u64(u64(a) + u64(b))


def key_for(cells, key_columns):
    out = bytearray()
    for col in key_columns:
        value = cells[col]
        out += struct.pack("<I", 0xffffffff if value is None else len(value))
        if value is not None:
            out += value
    check(len(out) <= FRAME, "catalog.long_key")
    return bytes(out)


def encode_row(ordinal, cells):
    bitmap = bytearray((len(cells)+7)//8)
    payload = bytearray()
    for i, cell in enumerate(cells):
        if cell is None:
            bitmap[i//8] |= 1 << (i % 8)
        else:
            check(type(cell) is bytes and len(cell) <= FRAME, "catalog.frame_limit")
            payload += struct.pack("<I", len(cell)) + cell
    size = ROW.size + len(bitmap) + len(payload)
    check(size <= FRAME, "catalog.frame_limit")
    return ROW.pack(size, ordinal, len(cells), len(bitmap)) + bitmap + payload


def decode_row(data, expected_ordinal, columns):
    check(len(data) >= ROW.size, "catalog.truncated")
    length, ordinal, count, bits = ROW.unpack_from(data)
    check(length == len(data) and length <= FRAME, "catalog.frame_limit")
    check(ordinal == expected_ordinal, "catalog.duplicate_row")
    check(count == columns and bits == (count+7)//8, "catalog.frame")
    check(ROW.size+bits <= len(data), "catalog.truncated")
    if count % 8:
        check(data[ROW.size+bits-1] >> (count % 8) == 0, "catalog.frame")
    at = ROW.size + bits
    cells = []
    for i in range(count):
        if data[ROW.size+i//8] & (1 << (i % 8)):
            cells.append(None)
        else:
            check(at+4 <= len(data), "catalog.truncated")
            size, = struct.unpack_from("<I", data, at)
            at += 4
            check(size <= FRAME and plus(at, size) <= len(data), "catalog.truncated")
            cells.append(data[at:at+size])
            at += size
    check(at == len(data), "catalog.trailing")
    return cells


class StoreWriter:
    def __init__(self, root):
        self.root = Path(root)
        self.root.mkdir()
        (self.root / "objects").mkdir()
        (self.root / "indexes").mkdir()
        self.raw_bytes = self.index_bytes = self.max_frame = self.max_chunk = 0
        self.rows = self.max_decoded = 0

    def put(self, data, directory="objects"):
        h = sha(data)
        path = self.root / directory / h
        if path.exists():
            check(path.read_bytes() == data, "catalog.hash")
        else:
            with path.open("xb") as handle:
                handle.write(data)
                handle.flush()
                os.fsync(handle.fileno())
            if directory == "indexes":
                self.index_bytes += len(data)
                check(self.index_bytes <= MAX_INDEX, "catalog.index_limit")
            else:
                self.raw_bytes += len(data)
                check(self.raw_bytes <= MAX_RAW, "catalog.raw_limit")
        return {"sha256": h, "bytes": len(data)}

    def page(self, kind, sid, first, entries):
        check(len(entries) <= PAGE-HEADER.size, "catalog.index")
        size = {1: DESC.size, 2: DESC.size, 3: KEY.size, 4: KEY.size+CHILD.size}[kind]
        check(len(entries) % size == 0, "catalog.index")
        data = HEADER.pack(INDEX_MAGIC, 3, kind, sid, first, len(entries)//size) + entries
        return self.put(data.ljust(PAGE, b"\0"), "indexes")

    def ordinal_tree(self, sid, descriptors):
        level = []
        for at in range(0, len(descriptors), 84):
            batch = descriptors[at:at+84]
            first = batch[0][1]
            count = sum(x[2] for x in batch)
            page = self.page(1, sid, first, b"".join(DESC.pack(bytes.fromhex(h), s, n, b) for h,s,n,b in batch))
            level.append((page["sha256"], first, count, PAGE))
        if not level:
            p = self.page(1, sid, 0, b"")
            level = [(p["sha256"], 0, 0, PAGE)]
        height = 0
        while len(level) > 1:
            next_level = []
            for at in range(0, len(level), 84):
                batch = level[at:at+84]
                p = self.page(2, sid, batch[0][1], b"".join(DESC.pack(bytes.fromhex(h),s,n,b) for h,s,n,b in batch))
                next_level.append((p["sha256"],batch[0][1],sum(x[2] for x in batch),PAGE))
            level = next_level
            height += 1
            check(height < LIMITS["depth"], "catalog.depth")
        return {"sha256":level[0][0],"bytes":PAGE,"count":level[0][2],"height":height}

    def key_record(self, key, ordinal):
        check(len(key) <= FRAME, "catalog.long_key")
        value = key if len(key) <= 64 else bytes.fromhex(self.put(key)["sha256"])
        return KEY.pack(len(key), 0, ordinal, value.ljust(64, b"\0"))

    def key_tree(self, sid, cursor):
        level, entries, last, count = [], bytearray(), None, 0
        def finish():
            p = self.page(3, sid, 0, bytes(entries))
            level.append((last, p["sha256"], count))
        for key, ordinal in cursor:
            if len(entries)+KEY.size > PAGE-HEADER.size:
                finish()
                entries, count = bytearray(), 0
            entries += self.key_record(key, ordinal)
            last = (key, ordinal)
            count += 1
        if entries or not level:
            finish()
        height = 0
        while len(level) > 1:
            next_level = []
            for at in range(0, len(level), 33):
                batch = level[at:at+33]
                payload = b"".join(self.key_record(k[0],k[1])+CHILD.pack(bytes.fromhex(h),n) for k,h,n in batch)
                p = self.page(4, sid, 0, payload)
                next_level.append((batch[-1][0],p["sha256"],sum(n for _,_,n in batch)))
            level = next_level
            height += 1
            check(height < LIMITS["depth"], "catalog.depth")
        return {"sha256":level[0][1],"bytes":PAGE,"count":level[0][2],"height":height}

    def relation(self, sid, columns, key_columns, rows):
        descriptors, payload, count, start, ordinal = [], bytearray(), 0, 0, 0
        with tempfile.TemporaryDirectory(prefix=".key-sort-", dir=self.root) as temporary:
            db = sqlite3.connect(str(Path(temporary)/"keys.sqlite"))
            db.execute("PRAGMA cache_size=-1024")
            db.execute("PRAGMA temp_store=FILE")
            db.execute("CREATE TABLE keys (k BLOB NOT NULL, ordinal INTEGER PRIMARY KEY)")
            def flush():
                data = DATA_HEADER.pack(MAGIC,3,1,sid,len(descriptors),count,len(payload)) + payload
                check(len(data) <= CHUNK, "catalog.chunk_limit")
                d = self.put(data)
                descriptors.append((d["sha256"],start,count,d["bytes"]))
                self.max_chunk = max(self.max_chunk,len(data))
            for cells in rows:
                check(len(cells) == columns, "catalog.frame")
                frame = encode_row(ordinal,cells)
                self.max_frame = max(self.max_frame,len(frame))
                decoded = sys.getsizeof(cells)+sum(sys.getsizeof(c) for c in cells)
                self.max_decoded = max(self.max_decoded,decoded)
                check(decoded <= DECODED, "catalog.batch_limit")
                if count and (count == BATCH or len(payload)+len(frame)+HEADER.size > CHUNK):
                    flush()
                    payload, count, start = bytearray(), 0, ordinal
                payload += frame
                count += 1
                db.execute("INSERT INTO keys VALUES (?,?)",(key_for(cells,key_columns),ordinal))
                ordinal += 1
                self.rows += 1
                check(self.rows <= MAX_ROWS, "catalog.row_limit")
            if count:
                flush()
            db.commit()
            ordinal_root = self.ordinal_tree(sid,descriptors)
            key_root = self.key_tree(sid,db.execute("SELECT k,ordinal FROM keys ORDER BY k,ordinal"))
            db.close()
        return {"id":sid,"columns":columns,"key_columns":key_columns,"rows":ordinal,
                "ordinal":ordinal_root,"key":key_root}


class Catalog:
    """Open handles and one checked chunk, not a decoded per-consumer database."""
    def __init__(self, root, candidate=False, cache_limit=CACHE, batch_limit=BATCH):
        self.root = Path(root)
        self.handles = {}
        self.directories = []
        self.reads = self.read_bytes = self.max_read = self.max_frame = self.max_decoded = 0
        self.max_depth = self.cache_peak = 0
        self.cached = None
        self.pages = OrderedDict()
        self.index_bytes = self.raw_bytes = 0
        self.cache_limit, self.batch_limit = cache_limit, batch_limit
        self.epoch = 0
        started = time.monotonic()
        try:
            self._root = self._directory(self.root)
            self.root = self._root.path
            self._objects = self._directory("objects",self._root)
            self._indexes = self._directory("indexes",self._root)
            manifest = self._file("candidate.json" if candidate else "manifest.json",self._root,CHUNK)
            raw = self._read_fd(manifest,0,os.fstat(manifest).st_size)
            self.manifest = self._json(raw)
            fields(self.manifest, ["format","core_revision","database_revision","source_snapshot","schema_root",
                   "loader_coverage_root","relation_root","index_root","dbc_root","registration_root",
                   "occurrence_root","scope","limits_policy","completeness","catalog_sha256"], "catalog.manifest")
            m = self.manifest
            check(m["format"] == FORMAT and m["core_revision"] == schema_contract.CORE_REVISION
                  and m["database_revision"] == schema_contract.DATABASE_REVISION, "catalog.version")
            check(canonical(m["scope"]) == canonical({"primary_maps":[0,1],"global":True,"other_maps":True}), "catalog.scope")
            check(m["source_snapshot"] == "phase0-sequential-diagnostic-not-F1" and m["completeness"] is False
                  and m["registration_root"] is None and m["occurrence_root"] is None, "catalog.incomplete")
            check(m["limits_policy"] == LIMITS, "catalog.limits")
            check(m["catalog_sha256"] == digest({k:v for k,v in m.items() if k != "catalog_sha256"}), "catalog.hash")
            schema = self._json(self.object(m["schema_root"]))
            coverage = self._json(self.object(m["loader_coverage_root"]))
            self.schema_report = schema_contract.verify_schema(schema,coverage)
            self.relations = self._json(self.object(m["relation_root"]))
            check(set(self.relations) == set(schema["tables"]), "catalog.table")
            check(m["index_root"] == digest({n:{k:r[k] for k in ("ordinal","key")} for n,r in self.relations.items()}), "catalog.hash")
            self.seen = set()
            self.total_rows = 0
            rows_hash = hashlib.sha256(b"benilla-catalog-rows-v3\0")
            for sid,(name, relation) in enumerate(sorted(self.relations.items()),1):
                fields(relation,["id","columns","key_columns","rows","ordinal","key"],"catalog.relation")
                check(u64(relation["id"]) == sid and u64(relation["columns"]) == len(schema["tables"][name]["columns"]), "catalog.schema_mismatch")
                expected_keys = primary_columns(schema["tables"][name])
                check(type(relation["key_columns"]) is list and all(type(i) is int and 0 <= i <= (1 << 64)-1 for i in relation["key_columns"]),"catalog.index")
                check(relation["key_columns"] == expected_keys, "catalog.index")
                rows = u64(relation["rows"])
                self.total_rows = plus(self.total_rows,rows)
                check(self.total_rows <= MAX_ROWS, "catalog.row_limit")
                self._root_descriptor(relation["ordinal"],rows)
                self._root_descriptor(relation["key"],rows)
                end = 0
                chunk_number = 0
                for h,start,count,size in self._ordinal(relation["ordinal"],sid,0):
                    check(start == end and count > 0 and count <= 4096, "catalog.range")
                    self._unique("objects",h)
                    data = self.object({"sha256":h,"bytes":size})
                    chunk_rows = self._frames(data,relation,start,count,chunk_number)
                    for ordinal,frame,cells in chunk_rows:
                        rows_hash.update(struct.pack("<IQI",sid,ordinal,len(frame)))
                        rows_hash.update(frame)
                    end = plus(start,count)
                    chunk_number += 1
                check(end == rows, "catalog.range")
                last, count = None, 0
                for key,ordinal in self._keys(relation["key"],sid,0):
                    check(last is None or last < (key,ordinal), "catalog.duplicate_row")
                    check(ordinal < rows, "catalog.index")
                    cells = self.row(name,ordinal)
                    check(key_for(cells,expected_keys) == key, "catalog.index")
                    last, count = (key,ordinal), plus(count,1)
                check(count == rows, "catalog.index")
            dbc = self._json(self.object(m["dbc_root"]))
            total = 0
            for name, descriptor in sorted(dbc.items()):
                check(type(name) is str and name.endswith(".dbc") and "/" not in name and "\\" not in name, "catalog.path")
                size = u64(descriptor["bytes"])
                check(size <= LIMITS["dbc_file"], "catalog.raw_limit")
                total = plus(total,size)
                check(total <= LIMITS["dbc_total"], "catalog.raw_limit")
                self.verify_blob(descriptor)
            self.dbc_count, self.dbc_bytes = len(dbc), total
            self.rows_sha256 = rows_hash.hexdigest()
            self.open_seconds = time.monotonic()-started
            self._checked_identities()
        except BaseException:
            self.close()
            raise

    def _directory(self,path,parent=None):
        directory = Directory(path) if parent is None else parent.child(path)
        self.directories.append(directory)
        return directory

    def _file(self,name,parent,limit):
        fd = parent.open_file(name)
        meta = os.fstat(fd)
        if not stat.S_ISREG(meta.st_mode) or meta.st_size > limit:
            os.close(fd)
            check(False,"catalog.file")
        self.handles[(parent,name)] = (fd,self._identity(meta))
        if parent in (self._objects,self._indexes):
            if parent == self._indexes:
                self.index_bytes += meta.st_size
                check(self.index_bytes <= MAX_INDEX,"catalog.index_limit")
            else:
                self.raw_bytes += meta.st_size
                check(self.raw_bytes <= MAX_RAW,"catalog.raw_limit")
        return fd

    @staticmethod
    def _identity(meta):
        return (meta.st_dev,meta.st_ino,meta.st_size,meta.st_mtime_ns,meta.st_ctime_ns)

    def _checked_identities(self):
        for fd,identity in self.handles.values():
            check(self._identity(os.fstat(fd)) == identity,"catalog.changed")

    def _read_fd(self,fd,offset,size):
        check(size <= CHUNK and plus(offset,size) <= os.fstat(fd).st_size,"catalog.truncated")
        check(size <= self.cache_limit,"catalog.cache_limit")
        self.max_read = max(self.max_read,size)
        self.reads += 1
        self.read_bytes += size
        data = read_at(fd,size,offset)
        check(len(data) == size,"catalog.truncated")
        return data

    def _json(self,raw):
        json_budget(raw)
        value = decode_json(raw,CHUNK)
        check(canonical(value) == raw,"catalog.json")
        return value

    def _descriptor(self,d,limit):
        fields(d,["sha256","bytes"],"catalog.descriptor")
        h = d["sha256"]
        check(type(h) is str and len(h) == 64 and all(c in "0123456789abcdef" for c in h),"catalog.path")
        check(u64(d["bytes"]) <= limit,"catalog.overflow")
        return h

    def object(self,descriptor,directory="objects"):
        h = self._descriptor(descriptor,CHUNK)
        parent = self._objects if directory == "objects" else self._indexes
        fd, identity = self.handles.get((parent,h),(None,None))
        if fd is None:
            fd = self._file(h,parent,CHUNK)
            identity = self.handles[(parent,h)][1]
        check(self._identity(os.fstat(fd)) == identity,"catalog.changed")
        check(identity[2] == descriptor["bytes"],"catalog.truncated")
        if directory == "indexes" and h in self.pages:
            self.pages.move_to_end(h)
            return self.pages[h]
        data = self._read_fd(fd,0,descriptor["bytes"])
        check(sha(data) == h,"catalog.hash")
        check(self._identity(os.fstat(fd)) == identity,"catalog.changed")
        if directory == "indexes":
            if len(self.pages) == 256:
                self.pages.popitem(last=False)
            self.pages[h] = data
        self.cache_peak = max(self.cache_peak,len(data)+(len(self.cached[1]) if self.cached else 0)+len(self.pages)*PAGE+PAGE*8+FRAME)
        check(self.cache_peak <= self.cache_limit,"catalog.cache_limit")
        return data

    def verify_blob(self,descriptor):
        h = self._descriptor(descriptor,LIMITS["dbc_file"])
        fd = self._file(h,self._objects,LIMITS["dbc_file"]) if (self._objects,h) not in self.handles else self.handles[(self._objects,h)][0]
        identity = self.handles[(self._objects,h)][1]
        check(identity[2] == descriptor["bytes"],"catalog.truncated")
        state = hashlib.sha256()
        for at in range(0,descriptor["bytes"],FRAME):
            state.update(self._read_fd(fd,at,min(FRAME,descriptor["bytes"]-at)))
        check(state.hexdigest() == h,"catalog.hash")
        check(self._identity(os.fstat(fd)) == identity,"catalog.changed")

    def blob_range(self, descriptor, offset, size):
        """Read bounded ranges from the already verified immutable blob handle."""
        h = self._descriptor(descriptor, LIMITS["dbc_file"])
        check(type(size) is int and 0 <= size <= FRAME, "catalog.frame_limit")
        check(plus(offset, size) <= descriptor["bytes"], "catalog.truncated")
        fd, identity = self.handles[(self._objects, h)]
        check(self._identity(os.fstat(fd)) == identity, "catalog.changed")
        peak = ((len(self.cached[1]) if self.cached else 0) + len(self.pages)*PAGE
                + PAGE*8 + FRAME*4)
        check(peak <= self.cache_limit, "catalog.cache_limit")
        self.cache_peak = max(self.cache_peak, peak)
        data = self._read_fd(fd, offset, size)
        check(self._identity(os.fstat(fd)) == identity, "catalog.changed")
        return data

    def _root_descriptor(self,d,count):
        fields(d,["sha256","bytes","count","height"],"catalog.descriptor")
        self._descriptor({k:d[k] for k in ("sha256","bytes")},PAGE)
        check(d["bytes"] == PAGE and u64(d["count"]) == count,"catalog.index")
        check(u64(d["height"]) < 8,"catalog.depth")

    def _unique(self,directory,h):
        check((directory,h) not in self.seen,"catalog.duplicate_descriptor")
        self.seen.add((directory,h))
        check(len(self.seen) <= MAX_INDEX//PAGE+MAX_ROWS,"catalog.index_limit")

    def _page(self,h,sid,kind,unique=False):
        if unique:
            self._unique("indexes",h)
        data = self.object({"sha256":h,"bytes":PAGE},"indexes")
        magic,version,actual,schema,first,count = HEADER.unpack_from(data)
        check(magic == INDEX_MAGIC and version == 3 and actual == kind and schema == sid,"catalog.index")
        width = DESC.size if kind < 3 else KEY.size+(CHILD.size if kind == 4 else 0)
        check(count <= (PAGE-HEADER.size)//width,"catalog.index")
        check(not any(data[HEADER.size+count*width:]),"catalog.trailing")
        return data,first,count

    def _ordinal(self,root,sid,start,unique=True):
        height = root["height"]
        self.max_depth = max(self.max_depth,height+1)
        data,first,count = self._page(root["sha256"],sid,1 if height == 0 else 2,unique)
        check(first == start,"catalog.range")
        end = start
        for i in range(count):
            h,s,n,b = DESC.unpack_from(data,HEADER.size+i*DESC.size)
            check(s == end and n > 0,"catalog.range")
            if height == 0:
                check(b <= CHUNK,"catalog.overflow")
                yield h.hex(),s,n,b
            else:
                check(b == PAGE,"catalog.index")
                child = {"sha256":h.hex(),"bytes":b,"count":n,"height":height-1}
                yield from self._ordinal(child,sid,s,unique)
            end = plus(s,n)
        check(end-start == root["count"],"catalog.range")

    def _key(self,data,at):
        size,reserved,ordinal,raw = KEY.unpack_from(data,at)
        check(size <= FRAME,"catalog.long_key")
        check(reserved == 0,"catalog.index")
        if size <= 64:
            check(not any(raw[size:]),"catalog.trailing")
            key = raw[:size]
        else:
            check(not any(raw[32:]),"catalog.trailing")
            key = self.object({"sha256":raw[:32].hex(),"bytes":size})
        return key,ordinal

    def _keys(self,root,sid,depth,unique=True):
        check(depth < 8,"catalog.depth")
        height = root["height"]
        self.max_depth = max(self.max_depth,depth+1)
        data,first,count = self._page(root["sha256"],sid,3 if height == 0 else 4,unique)
        check(first == 0,"catalog.index")
        width = KEY.size+(CHILD.size if height else 0)
        total,last = 0,None
        for i in range(count):
            at = HEADER.size+i*width
            pair = self._key(data,at)
            check(last is None or last < pair,"catalog.duplicate_row")
            if height:
                h,n = CHILD.unpack_from(data,at+KEY.size)
                child = {"sha256":h.hex(),"bytes":PAGE,"height":height-1,"count":n}
                child_last = None
                for item in self._keys(child,sid,depth+1,unique):
                    check(last is None or last < item,"catalog.index")
                    child_last = item
                    yield item
                check(child_last == pair,"catalog.index")
                total = plus(total,n)
            else:
                total = plus(total,1)
                yield pair
            last = pair
        check(total == root["count"],"catalog.index")

    def _frames(self,data,relation,start,count,chunk_number=None):
        check(len(data) >= HEADER.size,"catalog.truncated")
        magic,version,kind,sid,chunk,n,payload = DATA_HEADER.unpack_from(data)
        check(magic == MAGIC and version == 3 and kind == 1 and sid == relation["id"] and n == count
              and (chunk_number is None or chunk == chunk_number),"catalog.frame")
        check(payload <= len(data)-DATA_HEADER.size,"catalog.truncated")
        check(payload == len(data)-DATA_HEADER.size,"catalog.trailing")
        at = HEADER.size
        for ordinal in range(start,start+count):
            check(at+4 <= len(data),"catalog.truncated")
            size, = struct.unpack_from("<I",data,at)
            check(ROW.size <= size <= FRAME,"catalog.frame_limit")
            check(plus(at,size) <= len(data),"catalog.truncated")
            frame = memoryview(data)[at:at+size]
            cells = decode_row(frame,ordinal,relation["columns"])
            self.max_frame = max(self.max_frame,size)
            decoded = sys.getsizeof(cells)+sum(sys.getsizeof(v) for v in cells)
            self.max_decoded = max(self.max_decoded,decoded)
            check(decoded <= DECODED,"catalog.batch_limit")
            yield ordinal,frame,cells
            at += size
        check(at == len(data),"catalog.trailing")

    def row(self,name,ordinal):
        check(name in self.relations,"catalog.table")
        r = self.relations[name]
        check(type(ordinal) is int and 0 <= ordinal < r["rows"],"catalog.range")
        # The ordinal tree is shallow; locate the child without scanning earlier row frames.
        h,start,count,size = self._locate(r["ordinal"],r["id"],ordinal)
        if self.cached is None or self.cached[0] != h:
            self.cached = None
            self.cached = (h,self.object({"sha256":h,"bytes":size}))
            offsets, at = [], HEADER.size
            for number,frame,cells in self._frames(self.cached[1],r,start,count):
                offsets.append((at,at+len(frame)))
                at += len(frame)
            self.cached = (*self.cached,offsets)
        else:
            fd,identity = self.handles[(self._objects,h)]
            check(self._identity(os.fstat(fd)) == identity,"catalog.changed")
        a,b = self.cached[2][ordinal-start]
        return decode_row(memoryview(self.cached[1])[a:b],ordinal,r["columns"])

    def _locate(self,root,sid,ordinal):
        data,first,count = self._page(root["sha256"],sid,1 if root["height"] == 0 else 2)
        for i in range(count):
            h,s,n,b = DESC.unpack_from(data,HEADER.size+i*DESC.size)
            if s <= ordinal < plus(s,n):
                if root["height"] == 0:
                    return h.hex(),s,n,b
                return self._locate({"sha256":h.hex(),"bytes":PAGE,"height":root["height"]-1,"count":n},sid,ordinal)
        check(False,"catalog.index")

    def scan(self,name,start_ordinal=0,batch_rows=BATCH):
        check(name in self.relations,"catalog.table")
        check(type(batch_rows) is int and 1 <= batch_rows <= self.batch_limit and batch_rows <= BATCH,"catalog.batch_limit")
        r = self.relations[name]
        check(type(start_ordinal) is int and 0 <= start_ordinal <= r["rows"],"catalog.range")
        for h,start,count,size in self._ordinal(r["ordinal"],r["id"],0,False):
            if start+count <= start_ordinal:
                continue
            data = self.object({"sha256":h,"bytes":size})
            for ordinal,frame,cells in self._frames(data,r,start,count):
                if ordinal >= start_ordinal:
                    self.epoch += 1
                    yield RowView(self,self.epoch,name,ordinal)
        self.epoch += 1

    def lookup(self,name,key):
        check(name in self.relations,"catalog.table")
        check(type(key) is bytes and len(key) <= FRAME,"catalog.long_key")
        r = self.relations[name]
        yield from self._lookup(r["key"],r,key)

    def _lookup(self,root,relation,key):
        data,first,count = self._page(root["sha256"],relation["id"],3 if root["height"] == 0 else 4)
        width = KEY.size+(CHILD.size if root["height"] else 0)
        for i in range(count):
            at = HEADER.size+i*width
            k,ordinal = self._key(data,at)
            if root["height"]:
                if k >= key:
                    h,n = CHILD.unpack_from(data,at+KEY.size)
                    yield from self._lookup({"sha256":h.hex(),"bytes":PAGE,"height":root["height"]-1,"count":n},relation,key)
                if k > key:
                    break
            elif k == key:
                yield ordinal
            elif k > key:
                break

    def report(self):
        return {"catalog_sha256":self.manifest["catalog_sha256"],"rows_sha256":self.rows_sha256,
                "tables":len(self.relations),"columns":self.schema_report["columns"],"rows":self.total_rows,
                "dbc_files":self.dbc_count,"dbc_bytes":self.dbc_bytes,"complete":False,
                "raw_bytes":self.raw_bytes,"index_bytes":self.index_bytes,
                "catalog_bytes":sum(identity[2] for fd,identity in self.handles.values()),
                "read_calls":self.reads,"read_bytes":self.read_bytes,"max_read":self.max_read,
                "max_frame":self.max_frame,"decoded_row_bytes":self.max_decoded,"cache_peak":self.cache_peak,
                "index_depth":self.max_depth,"rss_kib":rss_kib(),
                "open_seconds":self.open_seconds}

    def close(self):
        for fd,_ in self.handles.values():
            os.close(fd)
        self.handles.clear()
        for fd in reversed(self.directories):
            fd.close()
        self.directories.clear()

    def __enter__(self):
        return self

    def __exit__(self,*_):
        self.close()


def publish(catalog,rust_report):
    identity_fields = ("catalog_sha256","rows_sha256","tables","columns","rows","dbc_files","dbc_bytes","complete")
    check(type(rust_report) is dict and all(rust_report.get(k) == catalog.report()[k] for k in identity_fields),
          "catalog.reader_parity")
    catalog._checked_identities()
    catalog._objects.sync()
    catalog._indexes.sync()
    payload = canonical(catalog.manifest)
    # Publish the checked bytes, not a replaceable candidate pathname.
    import secrets
    name = ".publish-"+secrets.token_hex(16)
    descriptor = catalog._root.open_file(name,create=True)
    try:
        with os.fdopen(descriptor,"wb") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        catalog._root.link(name,"manifest.json")
        catalog._root.sync()
    finally:
        catalog._root.unlink(name)


class RowView:
    def __init__(self,catalog,epoch,name,ordinal):
        self._catalog,self._epoch,self.relation,self.ordinal = catalog,epoch,name,ordinal

    def cells(self):
        check(self._catalog.epoch == self._epoch,"catalog.expired_view")
        return tuple(None if v is None else bytes(v) for v in self._catalog.row(self.relation,self.ordinal))


def primary_columns(table):
    names = [c["COLUMN_NAME"] for c in table["columns"]]
    keys = sorted((i for i in table["indexes"] if i["INDEX_NAME"] == "PRIMARY"), key=lambda x:x["SEQ_IN_INDEX"])
    # No declared key means the full row is the logical multikey; occurrences stay distinct.
    return [names.index(k["COLUMN_NAME"]) for k in keys] if keys else list(range(len(names)))


def capture(source,root,dbc_directory=None,pilot_only=False):
    started = time.monotonic()
    schema = schema_contract.inventory(source)
    coverage = schema_contract.coverage_inventory()
    schema_contract.verify_schema(schema,coverage)
    writer = StoreWriter(root)
    schema_root = writer.put(canonical(schema))
    coverage_root = writer.put(canonical(coverage))
    relations = {}
    for sid,(name,table) in enumerate(sorted(schema["tables"].items()),1):
        rows = source.rows(name) if not pilot_only or name == "world_template" else iter(())
        relations[name] = writer.relation(sid,len(table["columns"]),primary_columns(table),rows)
    # A pilot retains all schema definitions but explicitly empty synthetic relations are not complete data.
    relation_root = writer.put(canonical(relations))
    dbcs = {}
    dbc_bytes = 0
    if dbc_directory is not None:
        directory = Path(dbc_directory)
        for path in sorted(directory.glob("*.dbc")):
            check(path.is_file() and not path.is_symlink(),"catalog.path")
            size = path.stat().st_size
            check(size <= LIMITS["dbc_file"],"catalog.raw_limit")
            dbc_bytes = plus(dbc_bytes,size)
            check(dbc_bytes <= LIMITS["dbc_total"],"catalog.raw_limit")
            # DBC blobs are retained verbatim; streaming handles also cover the large Spell.dbc.
            h = hashlib.sha256()
            with path.open("rb") as handle, tempfile.NamedTemporaryFile(dir=writer.root/"objects",delete=False) as out:
                before = os.fstat(handle.fileno())
                while data := handle.read(FRAME):
                    out.write(data)
                    h.update(data)
                out.flush()
                os.fsync(out.fileno())
                check(Catalog._identity(before) == Catalog._identity(os.fstat(handle.fileno())),"catalog.changed")
                temporary = Path(out.name)
            target = writer.root/"objects"/h.hexdigest()
            if target.exists():
                temporary.unlink()
            else:
                os.link(temporary,target)
                temporary.unlink()
            dbcs[path.name] = {"sha256":h.hexdigest(),"bytes":size}
        check(len(dbcs) == 158,"catalog.dbc_inventory")
    dbc_root = writer.put(canonical(dbcs))
    manifest = {"format":FORMAT,"core_revision":schema_contract.CORE_REVISION,
        "database_revision":schema_contract.DATABASE_REVISION,"source_snapshot":"phase0-sequential-diagnostic-not-F1",
        "schema_root":schema_root,"loader_coverage_root":coverage_root,"relation_root":relation_root,
        "index_root":digest({n:{k:r[k] for k in ("ordinal","key")} for n,r in relations.items()}),
        "dbc_root":dbc_root,"registration_root":None,"occurrence_root":None,
        "scope":{"primary_maps":[0,1],"global":True,"other_maps":True},"limits_policy":LIMITS,"completeness":False}
    manifest["catalog_sha256"] = digest(manifest)
    with (writer.root/"candidate.json").open("xb") as handle:
        handle.write(canonical(manifest))
        handle.flush()
        os.fsync(handle.fileno())
    receipt = {"raw_bytes":writer.raw_bytes,"index_bytes":writer.index_bytes,"rows":writer.rows,
               "max_frame":writer.max_frame,"max_chunk":writer.max_chunk,"decoded_row_bytes":writer.max_decoded,
               "rss_kib":rss_kib(),"seconds":time.monotonic()-started,
               "pilot_only":pilot_only,"queries":source.queries}
    (writer.root/"acquisition.json").write_bytes(canonical(receipt))
    return receipt
