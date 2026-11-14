#!/usr/bin/env python3
"""Fail closed unless the uploaded source artifact outlives the player download.

Newly finalized artifacts may briefly return HTTP 404. Retry only bounded,
read-only visibility/service failures; never assume a requested retention period.
"""
from datetime import datetime, timedelta, timezone
import json
import os
import re
import subprocess
import time

RETRY_DELAYS = (1, 2, 4, 8, 16)
TRANSIENT = {404, 429, 500, 502, 503, 504}


def verify(repository, artifact_id, *, run=subprocess.run, sleep=time.sleep, now=None):
    if (not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository)
            or any(part in {".", ".."} for part in repository.split("/"))):
        raise ValueError("Invalid source repository")
    if not re.fullmatch(r"[1-9][0-9]{0,19}", artifact_id):
        raise ValueError("Invalid source artifact ID")
    command = ["gh", "api", f"repos/{repository}/actions/artifacts/{artifact_id}"]
    for attempt in range(len(RETRY_DELAYS) + 1):
        try:
            result = run(command, capture_output=True, text=True, timeout=30, check=False)
        except subprocess.TimeoutExpired:
            retryable = True
        else:
            if result.returncode == 0:
                if len(result.stdout) > 1024 * 1024:
                    raise ValueError("Oversized source artifact response")
                artifact = json.loads(result.stdout)
                if (type(artifact.get("id")) is not int
                        or artifact["id"] != int(artifact_id)
                        or artifact.get("expired") is not False):
                    raise ValueError("Source artifact identity or availability mismatch")
                expiry = datetime.fromisoformat(artifact["expires_at"].replace("Z", "+00:00"))
                if expiry.tzinfo is None:
                    raise ValueError("Source expiry lacks timezone")
                current = now if now is not None else datetime.now(timezone.utc)
                if expiry <= current + timedelta(days=15):
                    raise ValueError("Source retention must exceed 15 days before publishing a 14-day player")
                return
            status = re.search(r"\(HTTP ([0-9]{3})\)", result.stderr)
            retryable = status is not None and int(status.group(1)) in TRANSIENT
        if not retryable or attempt == len(RETRY_DELAYS):
            raise RuntimeError("Could not verify source artifact retention; player publication is blocked")
        sleep(RETRY_DELAYS[attempt])


if __name__ == "__main__":
    try:
        verify(os.environ.get("GITHUB_REPOSITORY", ""), os.environ.get("SOURCE_ID", ""))
    except (OSError, ValueError, KeyError, TypeError, AttributeError, RuntimeError) as error:
        raise SystemExit(f"Source retention verification failed: {error}") from None
    print("Verified exact source artifact retention exceeds the player download lifetime")
