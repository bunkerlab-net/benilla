#!/usr/bin/env python3
"""Package the player from an explicit allowlist, never a WoW install or checkout."""

import argparse
import hashlib
from pathlib import Path
import re
import tarfile
import zipfile

TARGETS = {
    "linux-amd64": "x86_64-unknown-linux-gnu",
    "linux-arm64": "aarch64-unknown-linux-gnu",
    "macos-arm64": "aarch64-apple-darwin",
    "windows-amd64": "x86_64-pc-windows-msvc",
}
DOCUMENTS = (
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "docs/BUILDS.md",
    "third_party/kira/LICENSE-MIT",
    "third_party/kira/LICENSE-APACHE",
    "third_party/lua-src/LICENSE",
)


def package(root, platform, commit):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("commit must be a full lowercase Git commit SHA")
    target = TARGETS[platform]
    executable = "benilla.exe" if platform == "windows-amd64" else "benilla"
    files = [(root / "target" / target / "release" / executable, executable)]
    files.extend((root / path, path) for path in DOCUMENTS)
    for source, _ in files:
        if source.is_symlink() or not source.is_file():
            raise ValueError(f"required regular file missing: {source}")
    name = f"benilla-{platform}-{commit}"
    dist = root / "dist"
    dist.mkdir(exist_ok=True)
    if platform == "windows-amd64":
        archive = dist / f"{name}.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
            for source, destination in files:
                output.write(source, f"{name}/{destination}")
    else:
        archive = dist / f"{name}.tar.gz"
        with tarfile.open(archive, "w:gz") as output:
            for source, destination in files:
                info = output.gettarinfo(str(source), f"{name}/{destination}")
                info.mode = 0o755 if destination == executable else 0o644
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                with source.open("rb") as content:
                    output.addfile(info, content)
    digest = hashlib.sha256()
    with archive.open("rb") as content:
        for chunk in iter(lambda: content.read(1024 * 1024), b""):
            digest.update(chunk)
    archive.with_name(archive.name + ".sha256").write_text(
        f"{digest.hexdigest()}  {archive.name}\n", encoding="utf-8"
    )
    return archive


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", required=True, choices=TARGETS)
    parser.add_argument("--commit", required=True)
    args = parser.parse_args()
    print(package(Path(__file__).resolve().parent.parent, args.platform, args.commit))
