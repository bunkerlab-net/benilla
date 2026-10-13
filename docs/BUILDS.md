# Build and run a player artifact

This fork's `Build player` workflow builds Linux amd64, Linux arm64, macOS arm64,
and Windows amd64 binaries. It runs only through `workflow_dispatch`. Pushes,
pull requests, tags, and schedules do not start it. It does not publish releases.

## Start a build

The workflow must be on the repository's default branch before GitHub enables
manual dispatch. Review and merge the build pull request separately before these
steps. Creating the pull request does not produce downloadable binaries.

1. Open the repository's **Actions** tab and select **Build player**.
2. Select **Run workflow** and choose the branch to build.
3. Confirm **Run workflow**. This starts all four jobs and uses GitHub Actions
   runner time and artifact storage under the repository's billing settings.
4. Open the run and check each job's result. One failed job does not cancel the
   others. Download the artifact for your operating system and CPU from the run's
   **Artifacts** section. You must sign in to GitHub to download it.

Artifacts expire after 14 days, subject to repository retention settings. Each
artifact contains an archive and its SHA-256 checksum. Names include the platform
and full source commit. Extract the GitHub artifact ZIP first, then the archive
inside it. Unix archives preserve the executable permission.

To verify the archive on Linux, run `sha256sum -c <archive>.sha256`. On macOS, run
`shasum -a 256 -c <archive>.sha256`. On Windows, run
`Get-FileHash .\<archive> -Algorithm SHA256` in PowerShell and compare the
result with the adjacent `.sha256` file. Replace `<archive>` with the downloaded
archive's name. These checks detect corruption, not publisher identity.

## Run the player

You need your own legally obtained English WoW 1.12.1 installation, build 5875,
and a compatible 1.12.1 server with Warden disabled. No game assets, MPQs,
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
checked-in `Cargo.lock`. Mr Boxington wraps Cargo for the build command:

```sh
mbx build --locked --release -p benilla --bin benilla --no-default-features --target <target>
```

The targets are `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
`aarch64-apple-darwin`, and `x86_64-pc-windows-msvc`. Disabling default features
selects the player build without developer tools. The workflow uses `release`
rather than the more expensive fat-LTO `ship` profile. It limits Cargo to two
parallel build jobs and each runner to 180 minutes.

Linux installs a C compiler, pkg-config, ALSA headers, and udev headers. macOS
uses the runner's Xcode tools and libclang; Windows uses its MSVC tools. The
workflow does not enable Bevy dynamic linking or require a game installation.

`scripts/package-player.py` includes only the executable, this guide, the project
licenses, and the vendored kira and Lua licenses. It does not copy a source tree,
asset directory, game installation, or local settings. Review dependency license
obligations before wider redistribution. Packaging tests run with
`python3 scripts/test-package-player.py` and require no game data or Rust compiler.

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
`target/<target>/release/` for the packaging script. Successful manual runs save
cache entries for later builds. Cache keys separate operating systems,
architectures, and Rust compiler versions. The cache generation is specific to
this workflow and its Ubuntu 26.04 baseline. The binary version can change between
runs; check the setup step's output for the version used.

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
