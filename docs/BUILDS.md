# Build and run a player artifact

This fork's `Build player` workflow builds the player and `benilla-native-terrain`
helper for Linux amd64, Linux arm64, macOS arm64, and Windows amd64. It runs on
pushes to `master`, temporarily on pushes to `feat/psk-p2p-foundation`, and through `workflow_dispatch`. Pull requests, other
branch pushes, tags, and schedules do not start it. Remove the explicit feature-branch
entry after this development cycle. It does not publish releases.

A separate **windows-amd64 terrain helper** job builds only the small headless
helper, so its download can become available while the full player jobs continue.
It uses the same commit, Rust pin, lockfile, source preparation, licenses, and
source-retention checks. It runs focused MPQ and terrain tests, including the
`--inspect` tests, from the prepared source. These checks do not establish
full-workspace, player, or real-install acceptance.

Each full player job also runs the local profile-storage and terrain-helper
regression suites before packaging. This exercises save/reopen, corruption handling,
file replacement, and authored terrain inputs on the actual target OS. Packaging
copies the helper without executing it or reading a player installation. Windows
does not claim Unix permissions or portable directory-fsync guarantees.

## Start a build

The workflow must be on the repository's default branch before GitHub enables
manual dispatch. Review and merge the build pull request separately before these
steps. Creating the pull request does not produce downloadable binaries.

1. Open the repository's **Actions** tab and select **Build player**.
2. Select **Run workflow** and choose the branch to build.
3. Confirm **Run workflow**. This starts four player jobs and the independent
   Windows helper job and uses GitHub Actions
   runner time and artifact storage under the repository's billing settings.
4. Open the run and check each job's result. One failed job does not cancel the
   others. Download the artifact for your operating system and CPU from the run's
   **Artifacts** section. You must sign in to GitHub to download it.

Artifacts expire after 14 days, subject to repository retention settings. Each
artifact contains a player archive and its SHA-256 checksum. The player archive
includes `SOURCE-DOWNLOAD.txt`, with an exact link and SHA-256 for its separate
`benilla-source-<platform>-<commit>` artifact in the same run. That source download
contains the exact committed source, locked vendored dependencies, build scripts,
and license/provenance notices. Source uploads happen before binary uploads and
request 90-day retention, longer than the player's 14 days. CI verifies the actual
source expiration is more than 15 days away before packaging the player; stricter
repository limits fail closed. Never manually remove a source artifact while its
binary is available.
Names include the platform and full source commit. Extract the GitHub artifact ZIP first, then the archive
inside it. Both executables are at the archive root, with `.exe` suffixes on
Windows. Unix archives give both executables executable permission.

To verify the archive on Linux, run `sha256sum -c <archive>.sha256`. On macOS, run
`shasum -a 256 -c <archive>.sha256`. On Windows, run
`Get-FileHash .\<archive> -Algorithm SHA256` in PowerShell and compare the
result with the adjacent `.sha256` file. Replace `<archive>` with the downloaded
archive's name. These checks detect corruption, not publisher identity.

For just the Windows helper, download
`benilla-native-terrain-windows-amd64-<commit>` once its job succeeds; you do not
need to wait for the player builds. Its ZIP contains `benilla-native-terrain.exe`,
the license/provenance documents, and `SOURCE-DOWNLOAD.txt`. The matching source
artifact is `benilla-native-terrain-source-windows-amd64-<commit>`, with the full
workspace and locked dependencies, not a reduced source subset. The source uses
the same `benilla-windows-amd64-<commit>-source.tar.gz` filename inside its
distinct artifact. Source uploads and verified retention precede the helper
upload, with the same 90-day source and 14-day binary retention requests.

## Run the player

You need your own legally obtained English WoW 1.12.1 installation, build 5875,
and, for realm mode, a compatible 1.12.1 server with Warden disabled.
For experimental peer mode, follow the included `docs/P2P-CONFIG.md`; only the
safe `config.example.toml` is shipped, never a populated peer key/configuration. No game assets, MPQs,
FrameXML, credentials, or user settings are included in the archive. The binary
embeds benilla's own UI files and shaders. You do not need the source checkout.

Extract the archive to a writable folder. Open a terminal in the extracted
`benilla-<platform>-<commit>` folder that contains the executable. On Linux or
macOS, run:

```sh
WOW_DATA="/path/to/WoW/Data" WOW_HOST="your-server.example:3724" ./benilla
```

On Windows, run in PowerShell:

```powershell
$env:WOW_DATA = "C:\path\to\WoW\Data"
$env:WOW_HOST = "your-server.example:3724"
.\benilla.exe
```

Replace the example paths and host with your installation and server. Enter your
account details at the login screen. The Realmlist button also sets the server.
If `WOW_DATA` is unset, the player looks for `Data/` or `WoW/Data/` beside the
binary. An empty `WOW_DATA` explicitly disables installation discovery.

The player writes settings, screenshots, logs, and addons to `benilla-config/`
beside the binary. Put compatible addons in `benilla-config/AddOns/`. It reads the
WoW installation without changing it.

## Inspect selected terrain without publishing

The packaged `benilla-native-terrain` command is a separate, headless helper. From
the extracted archive folder, inspect a selected tile on Linux or macOS with:

```sh
./benilla-native-terrain --data "/path/to/WoW/Data" --map 0 --tile 48,32 --inspect
```

On Windows, run in PowerShell:

```powershell
.\benilla-native-terrain.exe --data 'C:\Games\World of Warcraft\Data' --map 0 --tile 48,32 --inspect
```

Replace the example Data path with your own installation. `--inspect` runs the same
bounded preparation and terrain validation as generation, then reports the
result without publishing products, a manifest, or an output directory. It does
not require `--output`. The tile coordinates and map are explicit; repeat `--tile`
for at most nine selected tiles. The example is a request to inspect, not a claim
that this real tile is supported or has passed acceptance.

The helper reads the installation without modifying it. Unsupported inputs fail
closed, including history-dependent liquid edges and incompatible patch-chain winners;
there is no fallback conversion or automatic neighbor selection. Successful
inspection does not provide VMaps/MMaps, gameplay admission, movement authority,
or a live installation lease. Keep any generated terrain and source manifests
private. See the included [terrain helper guide](../crates/benilla-native-terrain/README.md)
for the separate explicit-output generation command and its publication limits.

## Platform requirements

- Linux artifacts use glibc and system audio/windowing libraries. They build on
  Ubuntu 26.04 and target Ubuntu 26.04 or a compatible newer distribution, not
  musl-based distributions. Install the runtime ALSA and udev libraries, a
  working graphical session, and a Vulkan-capable graphics driver. Use `ldd
  ./benilla` to check for missing libraries on your machine.
- The macOS artifact is for Apple silicon. It builds on macOS 26 and has no
  signing or notarization step. It is a command-line executable, not an app
  bundle. Follow your organization's policy if macOS blocks it.
- The Windows artifact is for x64 Windows and uses MSVC on the
  `windows-2025` runner. It has no signing or
  installer step. It may require Microsoft's Visual C++ x64 runtime. Keep the
  executable in a writable folder rather than `Program Files`.

A successful build does not test graphics, audio, login, gameplay, or compatibility
with older operating systems. Test those on the intended machine before sharing
an artifact as a release.

## Build details

Each job uses a native runner, the Rust version in `rust-toolchain.toml`, and the
checked-in `Cargo.lock`. Mr Boxington wraps Cargo. Full player jobs use both build commands:

```sh
mbx build --locked --release -p benilla --bin benilla --no-default-features --target <target>
mbx build --locked --release -p benilla-native-terrain --bin benilla-native-terrain --no-default-features --target <target>
```

The targets are `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
`aarch64-apple-darwin`, and `x86_64-pc-windows-msvc`. Disabling default features
selects the player build without developer tools. The workflow uses `release`
rather than the more expensive fat-LTO `ship` profile. Full player jobs limit Cargo
to two or three parallel build jobs per platform and each runner to 180 minutes.
The independent Windows helper job uses only the second build command, a separate
helper-only cache generation, three parallel Cargo jobs, and a 60-minute timeout.
It neither waits for nor replaces the full player jobs or the separate CI workflow.

Linux installs a C compiler, pkg-config, ALSA headers, and udev headers. macOS
uses the runner's Xcode tools and libclang; Windows uses its MSVC tools. The
workflow does not enable Bevy dynamic linking or require a game installation.

## GPL-covered distribution and corresponding source

The player links GPL-3.0-or-later `benilla-session` / `benilla-sim`. The combined
player is GPL-covered; original client files retain MIT OR Apache-2.0 grants.
The terrain helper is also GPL-3.0-or-later. Read `DISTRIBUTION-NOTICE.md`,
`LICENSE-GPL-3.0`, the simulation provenance, and the included
`crates/benilla-native-terrain/CMANGOS-NOTICE.txt`. Both executables use the same
exact corresponding-source archive. All existing upstream copyright/license
notices remain intact.

Each job first snapshots the selected full Git commit, runs `cargo vendor --locked
--versioned-dirs`, and checks the locked graph resolves offline using the generated
`.cargo/vendor-config.toml`. It then builds from that snapshot, with those vendored
sources, using `--offline --locked`, for its selected executable(s). `SOURCE-MANIFEST.json`
records the source commit and SHA-256 of every source/dependency file. `BUILD-INFO.txt` records the
Rust, Cargo, and mbx versions. Packaging checks that the snapshot has not changed
and archives its entire contents in the matching separate source download. CI
uploads source first and packages the player only after upload succeeds, using
the returned exact artifact URL and source archive hash. Both are available
through the same run with the same GitHub sign-in requirement. Source retention
is longer than binary retention; keep it available while offering the binary.
Packaging refuses missing source, licenses, dependencies, changed source, symlinks,
and known game-data/local-state paths. Only Git-tracked project files and Cargo's
dependency sources enter the snapshot, never arbitrary checkout contents. This
is not a general secret scanner: review tracked additions before building.

To reproduce the build, download the source using `SOURCE-DOWNLOAD.txt`, verify
its SHA-256, extract the artifact ZIP and its `*-source.tar.gz`, and change into its
`benilla-source-<commit>` folder, install the repository Rust toolchain and the
platform prerequisites above, then run:

```sh
cargo build --config .cargo/vendor-config.toml --offline --locked --release -p benilla --bin benilla --no-default-features --target <target>
cargo build --config .cargo/vendor-config.toml --offline --locked --release -p benilla-native-terrain --bin benilla-native-terrain --no-default-features --target <target>
```

Cargo is the underlying builder; mbx is an optional cache wrapper. The archive
contains the preferred source and build instructions, not prebuilt system toolchains
or OS libraries. Obtain these normally. The embedded build stamp may differ without
Git metadata, and host toolchains/caches can differ; bit-identical reproduction is
not promised. No signing keys, proprietary assets, or external game installation
are needed to compile. A game installation is needed for the normal game UI.

For a local distribution, commit all intended changes first (tracked dirty files
are refused), then run from the clean checkout:

```sh
commit=$(git rev-parse HEAD)
python3 scripts/package-player.py --prepare-source --commit "$commit"
export CARGO_TARGET_DIR="$PWD/target"
(cd .player-source/current && mbx build --config .cargo/vendor-config.toml --offline --locked --release -p benilla --bin benilla --no-default-features --target <target>)
(cd .player-source/current && mbx build --config .cargo/vendor-config.toml --offline --locked --release -p benilla-native-terrain --bin benilla-native-terrain --no-default-features --target <target>)
python3 scripts/package-player.py --platform <platform> --commit "$commit"
```

Local packaging without `--source-url` still embeds `corresponding-source.tar.gz`
for a self-contained distribution. CI uses `--source-only` to create and checksum
the source first, uploads it, then supplies the upload's exact artifact URL through
`--source-url`. Do not supply an unverified URL or one pointing at different source.

For a helper-only local distribution, use the same clean-source preparation, run
only the `benilla-native-terrain` build command, then add `--helper-only` to the
final packaging command. No player executable is required. The helper-only
archive keeps the same license/provenance documents and full source envelope;
its name starts with `benilla-native-terrain-`. To rebuild that download, use only
the helper's offline Cargo command above with `x86_64-pc-windows-msvc` on Windows.

Preparation requires Git, Cargo, Rust and mbx and downloads the locked dependencies.
It refuses to reuse an existing staging directory. After a failed preparation,
review/remove only `.player-source/current` before retrying. Packaging tests
use fake local fixtures: `python3 scripts/test-package-player.py` needs no game data,
dependency download or paid runner; one offline vendor-resolution test needs Cargo.

GPLv3 section 1 describes Corresponding Source including necessary build scripts
and non-system dependencies; section 6 covers object-code distribution and source
access. See the [GNU GPLv3 text](https://www.gnu.org/licenses/gpl-3.0.html) and
[GNU licensing FAQ](https://www.gnu.org/licenses/gpl-faq.html). This workflow uses
equivalent access to matching source under section 6(d), not a written-offer
mechanism or a repository URL alone. Keep source and license notices available
when sharing the player. A GitHub artifact URL expires: for a mirror or longer-lived
download, host the matching source alongside the binary and update the directions. If mirroring binary downloads,
provide the matching source with equivalent access for as long as the binary is
offered. If distributing the executable itself by peer-to-peer transfer, review
section 6(e)'s public source/object-code availability and notice requirements;
the game's peer networking does not itself distribute the executable. Separately
review dependency-specific notices and any applicable installation-information
requirements before distributing a User Product. This is an engineering delivery
mechanism, not legal advice or clearance of third-party game rights.

The explicit runner labels are `ubuntu-26.04`, `ubuntu-26.04-arm`, `macos-26`,
and `windows-2025`. These are standard hosted runners listed in
[GitHub's runner images](https://github.com/actions/runner-images). No larger-runner label is used. Standard runner compute is free
for this public repository; artifact and cache storage follow GitHub's limits.
The existing CI workflow also uses Ubuntu 26.04.

## Build cache and action updates

The workflow installs the latest Mr Boxington release and pins its setup action
to a commit. The setup
step runs after Rust installation so the cache key uses the repository's compiler.
It uses GitHub Actions cache in `target` mode, which keeps the build outputs at
`target/<target>/release/` for the packaging script. Successful default-branch
pushes and manual runs save cache entries for later builds. Cache keys separate operating systems,
architectures, and Rust compiler versions. The cache generation is specific to
this workflow and its Ubuntu 26.04 baseline. The binary version can change between
runs; check the setup step's output for the version used.

The fresh distribution snapshot uses the stable `.player-source/current` path so Cargo's
cached dependency paths do not change with every commit. It stays outside
`target/` because the cache action prunes that tree; the snapshot is never restored
from the build cache. Preparation still refuses an existing snapshot, and the
source manifest must match the requested commit and every file before packaging.
The first build after a staging-path change needs to warm the new paths. Compare
later builds' compile duration and Cargo reuse, not just mbx object-cache hits:
`target` mode transports Cargo artifacts rather than the mbx object store.

Run this workflow only on branches you trust. Cache storage counts against the
repository's GitHub Actions limits. These cached artifacts are for testing.
Before distributing a production release, use an uncached build or change the
Mr Boxington action to `backend: local` and set `MBX_TARGET_VIEWS=0` so the
packaging path stays unchanged. Mr Boxington's [release guidance](https://mr-boxington.jdx.dev/github-action#production-releases)
explains the cache-poisoning risk.

Dependabot checks GitHub Actions dependencies weekly and groups all version
updates into one pull request. Security updates follow GitHub's separate security
update behavior. This configuration starts after it reaches the default branch.
Dependabot updates action references. The mbx binary follows `version: latest`
and does not need a Dependabot update.

For workflow linting, run `actionlint -config-file .github/actionlint.yaml`.
The configuration lists the new hosted labels that actionlint 1.7.12 does not yet
recognize. GitHub's runner list, rather than this linter exception, establishes
runner availability.

## Windows terrain diagnostic report

The player and standalone terrain-helper archives include
`scripts/Collect-BenillaTerrainDiagnostics.ps1`. From the extracted archive, run
in Windows PowerShell 5.1 or PowerShell 7 on Windows:

```powershell
.\scripts\Collect-BenillaTerrainDiagnostics.ps1 -Data '.\Data' -Helper '.\benilla-native-terrain.exe' -Output '.\Benilla-Terrain-Diagnostics.json'
```

Use the actual local Data path if it is elsewhere. The output must be a new JSON
file outside Data, with an existing parent directory. Review that JSON before
sharing it. The collector reads bounded archive headers and invokes only helper
help/diagnosis (or inspection on older helpers); it does not upload data, extract
assets, or modify Data. It excludes local paths and arbitrary helper strings.
Default selection is map 0, tile 48,32, with a 120-second helper timeout. A report
can contain failed or unavailable stages; it does not establish gameplay readiness.
If script execution is blocked, use your normal approved script-running process;
the collector does not change execution policy.

The fast Windows helper job runs authored collector smoke tests under both
Windows PowerShell and PowerShell 7 from the exact distributed source before
building and delivering the helper. The tests use an authored fake helper compiled
by the runner's installed .NET compiler, including timeout, output bounds, redaction,
relative paths, and unchanged-input checks. They require no installation assets.
