#!/usr/bin/env bash
# Usage: bash scripts/ci-apt-install.sh PACKAGE...
# Retry the complete transaction three times per mirror, with five apt fetch retries.
# CI_APT_DIR redirects source edits to fixtures in test-ci-apt-install.py.
set -euo pipefail

if (( $# == 0 )); then
    echo 'Usage: ci-apt-install.sh PACKAGE...' >&2
    exit 2
fi
apt_dir=${CI_APT_DIR:-/etc/apt}
apt_options=(-o Acquire::Retries=5 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30)

install_packages() {
    local attempt
    for attempt in 1 2 3; do
        echo "apt update/install attempt $attempt/3"
        if sudo apt-get "${apt_options[@]}" update -qq &&
            sudo apt-get "${apt_options[@]}" install -y -qq "$@"; then
            return 0
        fi
        echo "apt update/install attempt $attempt/3 failed" >&2
        if (( attempt < 3 )); then
            sleep 10
        fi
    done
    return 1
}

if [[ -f "$apt_dir/apt-mirrors.txt" ]]; then
    echo 'Runner apt mirror list:'
    cat "$apt_dir/apt-mirrors.txt"
fi
if install_packages "$@"; then
    exit 0
fi

if ! getent ahosts security.ubuntu.com >/dev/null; then
    # The failed runner logs name only security.ubuntu.com, not a proven-good
    # alternate. Check DNS here; package installation must still succeed.
    case "$(dpkg --print-architecture)" in
        amd64|amd64v3) mirrors=(archive.ubuntu.com/ubuntu azure.archive.ubuntu.com/ubuntu) ;;
        arm64) mirrors=(ports.ubuntu.com/ubuntu-ports) ;;
        *) mirrors=() ;;
    esac
    for mirror in "${mirrors[@]}"; do
        host=${mirror%%/*}
        if ! getent ahosts "$host" >/dev/null; then
            echo "Cannot resolve apt fallback $host" >&2
            continue
        fi
        echo "Cannot resolve security.ubuntu.com; trying https://$mirror/"
        # Keep every suite, including -security, and its existing signature policy.
        # Only Ubuntu URLs change; third-party repositories remain untouched.
        sudo python3 - "$apt_dir" "https://$mirror/" <<'PY'
from pathlib import Path
import re
import sys

root = Path(sys.argv[1])
paths = [root / "sources.list", *root.glob("apt-mirrors*.txt"),
         *(root / "sources.list.d").glob("*.list"),
         *(root / "sources.list.d").glob("*.sources")]
url = re.compile(r"https?://(?:security|archive|azure\.archive|ports)\.ubuntu\.com/ubuntu(?:-ports)?(?:/)?(?=\s|$)")
changed = 0
for path in paths:
    if not path.is_file():
        continue
    original = path.read_text()
    updated = url.sub(sys.argv[2], original)
    if updated != original:
        path.write_text(updated)
        print(f"Updated Ubuntu mirror in {path}")
        changed += 1
if not changed:
    sys.exit("No Ubuntu apt source URLs found to switch to the fallback")
PY
        if install_packages "$@"; then
            exit 0
        fi
    done
fi

echo "apt installation failed after bounded retries and available DNS fallbacks: $*" >&2
exit 1
