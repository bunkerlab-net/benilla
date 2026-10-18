#!/usr/bin/env python3
"""Package the player and terrain helper with their exact vendored build source."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile
import zipfile
from urllib.parse import urlparse

TARGETS = {
    "linux-amd64": "x86_64-unknown-linux-gnu",
    "linux-arm64": "aarch64-unknown-linux-gnu",
    "macos-arm64": "aarch64-apple-darwin",
    "windows-amd64": "x86_64-pc-windows-msvc",
}
EXECUTABLES = ("benilla", "benilla-native-terrain")
DOCUMENTS = (
    "LICENSE-MIT", "LICENSE-APACHE", "LICENSE-GPL-3.0", "DISTRIBUTION-NOTICE.md",
    "docs/BUILDS.md", "docs/P2P-CONFIG.md", "docs/P2P-ENCOUNTER.md", "docs/P2P-SETTINGS.md",
    "docs/LOGIN-OPTIONS.md", "docs/P2P-CHARACTERS.md", "crates/benilla-profile/README.md", "config.example.toml",
    "docs/CLASSIC-EXPLORATION-STARTS.md",
    "crates/benilla-app/src/net/p2p/creation_start/licenses/AUTHORS",
    "crates/benilla-app/src/net/p2p/creation_start/licenses/COPYRIGHT.md",
    "crates/benilla-app/src/net/p2p/creation_start/licenses/LICENSE.md",
    "docs/CMANGOS-PARITY.md", "docs/CLASSIC-WORLD-IMPORT.md",
    "docs/CLASSIC-EXPORTER-CONTRACT.md", "docs/CLASSIC-STARTER-CAPTURE.md",
    "crates/benilla-sim/CLASSIC.md", "crates/benilla-sim/CLASSIC-TARGETS.md",
    "crates/benilla-sim/MAGE-RESOURCES.md", "crates/benilla-sim/STARTER-INIT.md",
    "crates/benilla-sim/NPC-MELEE.md", "crates/benilla-sim/CREATURE-INIT.md",
    "crates/benilla-sim/PLAYER-DEATH.md", "crates/benilla-sim/QUEST-LIFECYCLE.md",
    "crates/benilla-world-runtime/src/native/DEATH.md",
    "crates/benilla-app/src/net/p2p/world/native_geometry/README.md",
    "crates/benilla-native-dbc/README.md",
    "crates/benilla-native-terrain/README.md", "crates/benilla-native-terrain/CMANGOS-NOTICE.txt",
    "scripts/native_starter/README.md", "scripts/native_starter/PROVENANCE.md",
    "scripts/native_starter/CONSUMER-CONTRACT.md",
    "scripts/classic_export/NATIVE-SPAWN-CONTRACT.md",
    "scripts/classic_export/SPELL-REGISTRY-CONTRACT.md",
    "scripts/classic_export/INITIAL-HOOKS.md", "crates/benilla-sim/PLAYER-PROGRESSION.md",
    "crates/benilla-sim/KILL-XP.md",
    "crates/benilla-sim/CREATURE-AI.md", "crates/benilla-sim/SPELLS.md", "crates/benilla-app/src/net/p2p/world/README.md",
    "crates/benilla-sim/QUESTS.md", "crates/benilla-world-data/PROVENANCE.md",
    "crates/benilla-world-runtime/README.md", "crates/benilla-world-store/README.md",
    "crates/benilla-world-client/APP-CONTRACT.md", "crates/benilla-world-client/README.md",
    "crates/benilla-world-client/INTERACTIONS.md", "crates/benilla-world-client/NATIVE-STARTER.md",
    "crates/benilla-world-data/README.md",
    "third_party/kira/LICENSE-MIT", "third_party/kira/LICENSE-APACHE",
    "third_party/lua-src/LICENSE", "crates/benilla-sim/PROVENANCE.md",
    "crates/benilla-sim/licenses/LICENSE", "crates/benilla-sim/licenses/GPL-3.0.txt",
    "crates/benilla-sim/licenses/COPYRIGHT.md", "crates/benilla-sim/licenses/AUTHORS.md",
)
REQUIRED_SOURCE = (*DOCUMENTS, "Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
                   "scripts/package-player.py", "scripts/test-package-player.py",
                   ".github/workflows/build.yml", "crates/benilla/src/main.rs",
                   "crates/benilla-session/src/lib.rs", "crates/benilla-sim/src/lib.rs",
                   "crates/benilla-encounter/src/lib.rs", "crates/benilla-profile/src/lib.rs",
                   "crates/benilla-world-data/src/lib.rs", "crates/benilla-world-runtime/src/lib.rs",
                   "crates/benilla-world-client/src/lib.rs", "crates/benilla-world-store/src/lib.rs",
                   "scripts/classic-export.py", "scripts/native-starter.py",
                   "scripts/check-native-outfit-oracle.py", "scripts/test-initial-hooks.py",
                   "scripts/native_starter/outfit_cases.json",
                   "crates/benilla-native-dbc/src/main.rs", "crates/benilla-app/src/net/p2p/world.rs",
                   "crates/benilla-native-terrain/Cargo.toml",
                   "crates/benilla-native-terrain/src/main.rs", "crates/benilla-native-terrain/src/lib.rs",
                   "crates/benilla-native-terrain/src/source.rs", "crates/benilla-native-terrain/src/writer.rs",
                   "crates/benilla-world-data/tests/fixtures/original-camp.json",
                   "crates/benilla-world-store/tests/fixtures/runtime-v1.checkpoint",
                   ".cargo/vendor-config.toml", "BUILD-INFO.txt")
MANIFEST = "SOURCE-MANIFEST.json"


def run(root, *args):
    return subprocess.check_output(args, cwd=root)


def check_commit(commit):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("commit must be a full lowercase Git commit SHA")


def safe_path(name):
    path = PurePosixPath(name)
    if not name or path.is_absolute() or ".." in path.parts or "\\" in name:
        raise ValueError(f"unsafe source path: {name}")
    parts = {part.lower() for part in path.parts}
    vendored = name.startswith("vendor/")
    # This subtree is freshly produced by cargo vendor --locked, never copied from
    # local user settings. Public crate sources include .env build examples and
    # Windows Credentials API modules; preserve their complete source.
    forbidden = {"wow", "extracted", ".git", ".ssh", ".aws",
                 ".probe-identity", "id_rsa", "id_ed25519"}
    if not vendored:
        forbidden.add("credentials")
    if parts & forbidden or ("benilla-config" in parts and not (name == "crates/benilla-config" or name.startswith("crates/benilla-config/"))):
        raise ValueError(f"forbidden source input: {name}")
    if not vendored and ("p2p-profile" in parts or path.name.lower() in {"profile.json", "profile.previous.json", "profile.lock"}):
        raise ValueError(f"private profile input forbidden: {name}")
    if not vendored and ("p2p-worlds" in parts or path.name.lower() in {
            "world.checkpoint", "world.previous", "world.lock", ".world-checkpoint.pending"}):
        raise ValueError(f"private world save input forbidden: {name}")
    if not name.startswith("vendor/") and path.suffix.lower() in {".pem", ".key"}:
        raise ValueError(f"credential-like source input requires review: {name}")
    if (name == "config.toml" or (not vendored and path.name.startswith(".env"))
            or path.suffix.lower() in {".mpq", ".blp", ".m2", ".adt", ".wdt", ".wmo", ".dbc"}):
        raise ValueError(f"forbidden source input: {name}")


def regular(path, root):
    relative = path.relative_to(root)
    if ".." in relative.parts:
        raise ValueError(f"file outside package root: {path}")
    # Resolve trusted ancestors (for example macOS /var), never package inputs.
    current = root.resolve()
    for part in relative.parts:
        current = current / part
        if current.is_symlink():
            raise ValueError(f"required regular file missing or symlinked: {path}")
    if not current.is_file():
        raise ValueError(f"required regular file missing or symlinked: {path}")


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as content:
        for chunk in iter(lambda: content.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def inventory(source, root=None):
    root = source.parent if root is None else root
    if source.is_symlink():
        raise ValueError(f"symlink in source: {source}")
    result = {}
    for path in sorted(source.rglob("*")):
        name = path.relative_to(source).as_posix()
        if path.is_symlink():
            raise ValueError(f"symlink in source: {name}")
        if path.is_file() and name != MANIFEST:
            safe_path(name)
            regular(path, root)
            result[name] = digest(path)
    return result


def source_dir(root, commit):
    check_commit(commit)
    # Stable vendor paths preserve Cargo cache identity between commits. Keep
    # source outside target/, whose cache post-step recursively prunes files.
    # The manifest, rather than the directory name, binds source to its commit.
    return root / ".player-source" / "current"


def prepare_source(root, commit):
    check_commit(commit)
    if run(root, "git", "rev-parse", "HEAD").decode().strip() != commit:
        raise ValueError("source commit must equal HEAD")
    if run(root, "git", "status", "--porcelain", "--untracked-files=no"):
        raise ValueError("commit tracked source changes before preparing a distribution")
    source = source_dir(root, commit)
    if source.exists():
        raise ValueError(f"source staging already exists; review and remove it first: {source}")
    with tarfile.open(fileobj=io.BytesIO(run(root, "git", "archive", "--format=tar", commit))) as snapshot:
        members = snapshot.getmembers()
        for member in members:
            safe_path(member.name)
            if not (member.isdir() or member.isfile()):
                raise ValueError(f"unsupported source member: {member.name}")
        source.mkdir(parents=True)
        for member in members:
            path = source / member.name
            if member.isdir():
                path.mkdir(parents=True, exist_ok=True)
            else:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(snapshot.extractfile(member).read())
                path.chmod(member.mode & 0o777)
    lock_hash = digest(source / "Cargo.lock")
    config = run(source, "cargo", "vendor", "--locked", "--versioned-dirs", "vendor")
    (source / ".cargo").mkdir(exist_ok=True)
    (source / ".cargo/vendor-config.toml").write_bytes(config)
    run(source, "cargo", "--config", ".cargo/vendor-config.toml", "metadata", "--offline",
        "--locked", "--format-version", "1")
    if digest(source / "Cargo.lock") != lock_hash:
        raise ValueError("dependency preparation changed Cargo.lock")
    info = f"Source commit: {commit}\n"
    for command in (("rustc", "-Vv"), ("cargo", "--version"), ("mbx", "--version")):
        info += run(root, *command).decode() + "\n"
    (source / "BUILD-INFO.txt").write_text(info, encoding="utf-8")
    for name in REQUIRED_SOURCE:
        regular(source / name, root)
    files = inventory(source, root)
    if not any(p.startswith("vendor/") and p.endswith("/.cargo-checksum.json") for p in files):
        raise ValueError("vendored dependency source missing")
    (source / MANIFEST).write_text(json.dumps({"commit": commit, "files": files}, indent=2) + "\n")
    return source


def validate_source(source, commit, root=None):
    root = source.parent if root is None else root
    regular(source / MANIFEST, root)
    manifest = json.loads((source / MANIFEST).read_text())
    if manifest.get("commit") != commit:
        raise ValueError("source manifest commit does not match binary label")
    for name in REQUIRED_SOURCE:
        regular(source / name, root)
    files = inventory(source, root)
    if manifest.get("files") != files:
        raise ValueError("source changed after preparation, or source files are missing")
    if not any(p.startswith("vendor/") and p.endswith("/.cargo-checksum.json") for p in files):
        raise ValueError("vendored dependency source missing")
    return files


def write_tar(archive, files, prefix, executables=()):
    with tarfile.open(archive, "w:gz") as output:
        for source, destination in files:
            info = output.gettarinfo(str(source), f"{prefix}/{destination}")
            info.mode = 0o755 if destination in executables or source.stat().st_mode & 0o111 else 0o644
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            with source.open("rb") as content:
                output.addfile(info, content)


def source_archive_path(root, platform, commit):
    return root / "dist" / f"benilla-{platform}-{commit}-source.tar.gz"


def checksum(archive):
    archive.with_name(archive.name + ".sha256").write_text(
        f"{digest(archive)}  {archive.name}\n", encoding="utf-8")


def package_source(root, platform, commit):
    check_commit(commit)
    source = source_dir(root, commit)
    source_files = validate_source(source, commit, root)
    archive = source_archive_path(root, platform, commit)
    archive.parent.mkdir(exist_ok=True)
    write_tar(archive, [(source / p, p) for p in sorted([*source_files, MANIFEST])],
              f"benilla-source-{commit}")
    checksum(archive)
    return archive


def package(root, platform, commit, source_url=None, *, helper_only=False):
    check_commit(commit)
    source = source_dir(root, commit)
    validate_source(source, commit, root)
    suffix = ".exe" if platform == "windows-amd64" else ""
    selected = ("benilla-native-terrain",) if helper_only else EXECUTABLES
    executables = tuple(name + suffix for name in selected)
    files = [(root / "target" / TARGETS[platform] / "release" / name, name)
             for name in executables]
    files.extend((source / path, path) for path in DOCUMENTS)
    for path, _ in files:
        regular(path, root)
    product = "benilla-native-terrain" if helper_only else "benilla"
    name = f"{product}-{platform}-{commit}"
    dist = root / "dist"
    dist.mkdir(exist_ok=True)
    if source_url is None:
        source_archive = package_source(root, platform, commit)
        files.append((source_archive, "corresponding-source.tar.gz"))
    else:
        # CI passes upload-artifact's verified URL only after source upload succeeds.
        # Reject arbitrary hosts, credentials, queries and malformed artifact links.
        url = urlparse(source_url)
        repository = os.environ.get("GITHUB_REPOSITORY", "bunkerlab-net/benilla")
        if (url.scheme != "https" or url.netloc != "github.com" or url.query or url.fragment
                or not re.fullmatch(r"/" + re.escape(repository) + r"/actions/runs/[0-9]+/artifacts/[0-9]+", url.path)):
            raise ValueError("source URL must be an exact HTTPS GitHub Actions artifact URL")
        source_archive = source_archive_path(root, platform, commit)
        regular(source_archive, root)
        source_checksum = source_archive.with_name(source_archive.name + ".sha256")
        regular(source_checksum, root)
        expected = f"{digest(source_archive)}  {source_archive.name}\n"
        if source_checksum.read_text(encoding="utf-8") != expected:
            raise ValueError("prepared source archive checksum mismatch")
        notice = dist / f"{name}-SOURCE-DOWNLOAD.txt"
        subject = "benilla-native-terrain" if helper_only else "Benilla and benilla-native-terrain"
        notice.write_text(
            f"Corresponding Source for {subject}\nSource commit: {commit}\n"
            f"Platform: {platform}\n\nDownload: {source_url}\n"
            f"Archive: {source_archive.name}\nSHA-256: {digest(source_archive)}\n\n"
            "Download access is through the same GitHub Actions run as the binary; "
            "GitHub sign-in is required for both. Extract the downloaded ZIP, then "
            "the source tar.gz. See docs/BUILDS.md for offline build instructions.\n"
            "The complete source includes locked dependencies and license notices. "
            "Keep this source available with equivalent access for as long as the "
            "binary is offered. When mirroring or redistributing, include the source "
            "archive or host it alongside the binary and update these directions; "
            "do not rely on an expired GitHub artifact.\n", encoding="utf-8")
        files.append((notice, "SOURCE-DOWNLOAD.txt"))
    if platform == "windows-amd64":
        archive = dist / f"{name}.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
            for path, destination in files:
                output.write(path, f"{name}/{destination}")
    else:
        archive = dist / f"{name}.tar.gz"
        write_tar(archive, files, name, executables=executables)
    checksum(archive)
    return archive


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=TARGETS)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--prepare-source", action="store_true")
    parser.add_argument("--source-only", action="store_true")
    parser.add_argument("--helper-only", action="store_true",
                        help="package only benilla-native-terrain with the full matching source and notices")
    parser.add_argument("--source-url", help="already uploaded exact source artifact URL; omit to embed source")
    args = parser.parse_args()
    if args.prepare_source and (args.source_only or args.source_url or args.helper_only):
        parser.error("source preparation cannot be combined with packaging options")
    if args.source_only and args.source_url:
        parser.error("--source-only cannot be combined with --source-url")
    if args.source_only and args.helper_only:
        parser.error("--source-only cannot be combined with --helper-only")
    root = Path(__file__).resolve().parent.parent
    if args.prepare_source:
        print(prepare_source(root, args.commit))
    elif args.platform:
        if args.source_only:
            print(package_source(root, args.platform, args.commit))
        else:
            print(package(root, args.platform, args.commit, args.source_url,
                          helper_only=args.helper_only))
    else:
        parser.error("--platform is required unless --prepare-source is used")
