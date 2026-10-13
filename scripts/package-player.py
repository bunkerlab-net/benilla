#!/usr/bin/env python3
"""Prepare exact vendored build source and package it with the GPL-covered player."""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile
import zipfile

TARGETS = {
    "linux-amd64": "x86_64-unknown-linux-gnu",
    "linux-arm64": "aarch64-unknown-linux-gnu",
    "macos-arm64": "aarch64-apple-darwin",
    "windows-amd64": "x86_64-pc-windows-msvc",
}
DOCUMENTS = (
    "LICENSE-MIT", "LICENSE-APACHE", "LICENSE-GPL-3.0", "DISTRIBUTION-NOTICE.md",
    "docs/BUILDS.md", "docs/P2P-CONFIG.md", "docs/P2P-ENCOUNTER.md", "docs/P2P-SETTINGS.md", "config.example.toml",
    "third_party/kira/LICENSE-MIT", "third_party/kira/LICENSE-APACHE",
    "third_party/lua-src/LICENSE", "crates/benilla-sim/PROVENANCE.md",
    "crates/benilla-sim/licenses/LICENSE", "crates/benilla-sim/licenses/GPL-3.0.txt",
    "crates/benilla-sim/licenses/COPYRIGHT.md", "crates/benilla-sim/licenses/AUTHORS.md",
)
REQUIRED_SOURCE = (*DOCUMENTS, "Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
                   "scripts/package-player.py", "scripts/test-package-player.py",
                   ".github/workflows/build.yml", "crates/benilla/src/main.rs",
                   "crates/benilla-session/src/lib.rs", "crates/benilla-sim/src/lib.rs",
                   "crates/benilla-encounter/src/lib.rs",
                   "vendor-config.toml", "BUILD-INFO.txt")
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
    forbidden = {"wow", "extracted", ".git", ".ssh", ".aws",
                 ".probe-identity", "credentials", "id_rsa", "id_ed25519"}
    if parts & forbidden or ("benilla-config" in parts and not (name == "crates/benilla-config" or name.startswith("crates/benilla-config/"))):
        raise ValueError(f"forbidden source input: {name}")
    if not name.startswith("vendor/") and path.suffix.lower() in {".pem", ".key"}:
        raise ValueError(f"credential-like source input requires review: {name}")
    if (name == "config.toml" or path.name.startswith(".env")
            or path.suffix.lower() in {".mpq", ".blp", ".m2", ".adt", ".wdt", ".wmo", ".dbc"}):
        raise ValueError(f"forbidden source input: {name}")


def regular(path):
    if path.is_symlink() or not path.is_file() or any(p.is_symlink() for p in path.parents):
        raise ValueError(f"required regular file missing or symlinked: {path}")


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as content:
        for chunk in iter(lambda: content.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def inventory(source):
    result = {}
    for path in sorted(source.rglob("*")):
        name = path.relative_to(source).as_posix()
        if path.is_symlink():
            raise ValueError(f"symlink in source: {name}")
        if path.is_file() and name != MANIFEST:
            safe_path(name)
            regular(path)
            result[name] = digest(path)
    return result


def source_dir(root, commit):
    return root / "target" / "player-source" / commit


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
    (source / "vendor-config.toml").write_bytes(config)
    run(source, "cargo", "--config", "vendor-config.toml", "metadata", "--offline",
        "--locked", "--format-version", "1")
    if digest(source / "Cargo.lock") != lock_hash:
        raise ValueError("dependency preparation changed Cargo.lock")
    info = f"Source commit: {commit}\n"
    for command in (("rustc", "-Vv"), ("cargo", "--version"), ("mbx", "--version")):
        info += run(root, *command).decode() + "\n"
    (source / "BUILD-INFO.txt").write_text(info, encoding="utf-8")
    for name in REQUIRED_SOURCE:
        regular(source / name)
    files = inventory(source)
    if not any(p.startswith("vendor/") and p.endswith("/.cargo-checksum.json") for p in files):
        raise ValueError("vendored dependency source missing")
    (source / MANIFEST).write_text(json.dumps({"commit": commit, "files": files}, indent=2) + "\n")
    return source


def validate_source(source, commit):
    regular(source / MANIFEST)
    manifest = json.loads((source / MANIFEST).read_text())
    if manifest.get("commit") != commit:
        raise ValueError("source manifest commit does not match binary label")
    for name in REQUIRED_SOURCE:
        regular(source / name)
    files = inventory(source)
    if manifest.get("files") != files:
        raise ValueError("source changed after preparation, or source files are missing")
    if not any(p.startswith("vendor/") and p.endswith("/.cargo-checksum.json") for p in files):
        raise ValueError("vendored dependency source missing")
    return files


def write_tar(archive, files, prefix):
    with tarfile.open(archive, "w:gz") as output:
        for source, destination in files:
            info = output.gettarinfo(str(source), f"{prefix}/{destination}")
            info.mode = 0o755 if source.stat().st_mode & 0o111 else 0o644
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            with source.open("rb") as content:
                output.addfile(info, content)


def package(root, platform, commit):
    check_commit(commit)
    source = source_dir(root, commit)
    source_files = validate_source(source, commit)
    executable = "benilla.exe" if platform == "windows-amd64" else "benilla"
    files = [(root / "target" / TARGETS[platform] / "release" / executable, executable)]
    files.extend((source / path, path) for path in DOCUMENTS)
    for path, _ in files:
        regular(path)
    name = f"benilla-{platform}-{commit}"
    dist = root / "dist"
    dist.mkdir(exist_ok=True)
    source_archive = dist / f"{name}-source.tar.gz"
    write_tar(source_archive, [(source / p, p) for p in sorted([*source_files, MANIFEST])],
              f"benilla-source-{commit}")
    files.append((source_archive, "corresponding-source.tar.gz"))
    if platform == "windows-amd64":
        archive = dist / f"{name}.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
            for path, destination in files:
                output.write(path, f"{name}/{destination}")
    else:
        archive = dist / f"{name}.tar.gz"
        write_tar(archive, files, name)
    archive.with_name(archive.name + ".sha256").write_text(
        f"{digest(archive)}  {archive.name}\n", encoding="utf-8")
    return archive


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=TARGETS)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--prepare-source", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    if args.prepare_source:
        print(prepare_source(root, args.commit))
    elif args.platform:
        print(package(root, args.platform, args.commit))
    else:
        parser.error("--platform is required unless --prepare-source is used")
