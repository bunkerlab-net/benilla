# Private native DBC extraction

Read your normal WoW 1.12.1 Data folder through the same
`benilla_formats::Chain` used by the application, without another MPQ/locale
resolver:

```sh
cargo run -p benilla-native-dbc -- \
  --data "/path/to/your/WoW/Data" \
  --output benilla-config/local-dbc
```

The parent directory must exist; the output directory must be new and outside
Data. Each fixed archive member is capped at 64 MiB before read/decompression,
192 MiB in aggregate, then checked against its exact WDBC envelope. The opt-in
bounded chain also caps encoded indices cumulatively at 64 MiB, encoded/decoded
sectors at 64 KiB, header scanning at 1 MiB/archive, mounted archives at 64 and
directory entries at 4096. Index/sector bounds precede allocation and reads.
Only stored sectors and the existing exact zlib codec are supported; unknown
codecs, oversized sectors and excess inflation fail closed. Legacy callers retain
their prior behavior. Bounded archive opens require regular, non-symlink files;
Unix NONBLOCK/NOFOLLOW and opened-handle checks protect member reopens too. No install
file is modified. Highest-priority patches and tombstones keep their normal app
semantics. Missing/corrupt winners never fall back to an earlier copy.

All required members are read/checked before creating output. Files use
create-new semantics and the manifest is written last. A process crash can leave
an incomplete private directory without `extraction.json`; retry to a new path.
The preparation CLI requires that manifest and verifies the exact allowlist,
file hashes, lengths and layouts. It binds the manifest's exact bytes as well.
This is no-overwrite extraction, not a crash-recoverable world-store transaction.
On Unix the directory/files are 0700/0600 with file/directory sync. Other systems
inherit parent permissions and only file sync is asserted; use a private parent.

Whole extracted-file SHA-256 hashes and winning archive basenames are recorded.
WDBC headers have no build stamp, so the 5875 claim is explicitly unverified.
Manifest verification is integrity checking, not authenticated provenance.
No database rows, game assets, world rules or runtime activation are provided by
this crate. Keep outputs private and uncommitted.

The next command and the native compiler contract are documented in
[the starter compiler](../../scripts/native_starter/README.md).

Verification uses original generated MPQ/DBC bytes only. Actual CLI tests cover
patch precedence, tombstones, corrupt winners, pre-read size caps, unchanged
input archives, exact allowlists and non-overwrite private publication. No real
installation or graphical gameplay has been tested here.


The manifest contract is `benilla-native-dbc-extraction-v3`: exactly 20 tables.
CreatureDisplayInfo is mandatory (12 fields, 48-byte records), alongside the v2
Talent table. The Python verifier checks CreatureDisplayInfo string offsets only
at words 6–8; word 9 is signed SizeClass, not a string. It binds every selected
raw row and whole-file hash, preserving missing/invalid input without defaults.

To migrate a v1/v2 extraction, run the current helper into a new private output
directory and prepare again. Do not edit an old extraction manifest or copy in a
new table under an old completion marker. Missing/corrupt CreatureDisplayInfo
fails before publication, just like every other required member.
