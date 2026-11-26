"""
Checks every file Kusema can download, before a release: each URL must answer
200, with the size and SHA-256 the app pins. Fails (exit 1) if any doesn't.

  KUSEMA_DOWNLOAD_MANIFEST=manifest.json cargo test --lib \
    download_manifest::writes_the_manifest -- --ignored   (in src-tauri)
  python scripts/check_downloads.py manifest.json

Hugging Face files stored with LFS are checked from the Hub's own record of
them (its X-Linked-Size and X-Linked-ETag headers, the file's SHA-256), and
the download link is followed to make sure it answers. Everything else is
downloaded and hashed. No third-party packages; standard library only.
"""
import concurrent.futures
import hashlib
import json
import sys
import time
import urllib.error
import urllib.request

CHUNK = 1024 * 1024
AGENT = {"User-Agent": "kusema-release-check"}


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def first_response(url):
    """The first answer, without following a redirect: (status, headers)."""
    opener = urllib.request.build_opener(NoRedirect)
    request = urllib.request.Request(url, method="HEAD", headers=AGENT)
    try:
        with opener.open(request, timeout=60) as response:
            return response.status, response.headers
    except urllib.error.HTTPError as error:
        return error.code, error.headers


def final_status(url):
    """The status after following redirects, without downloading the body."""
    request = urllib.request.Request(url, headers={**AGENT, "Range": "bytes=0-0"})
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            return 200 if response.status in (200, 206) else response.status
    except urllib.error.HTTPError as error:
        return error.code


def download_hash(url):
    """(status, size, sha256) of the whole file."""
    request = urllib.request.Request(url, headers=AGENT)
    digest = hashlib.sha256()
    size = 0
    try:
        with urllib.request.urlopen(request, timeout=120) as response:
            for chunk in iter(lambda: response.read(CHUNK), b""):
                digest.update(chunk)
                size += len(chunk)
            return response.status, size, digest.hexdigest()
    except urllib.error.HTTPError as error:
        return error.code, None, None


def check(entry):
    url, want_size, want_sha = entry["url"], entry["size"], entry["sha256"]
    if url.startswith("https://huggingface.co/"):
        status, headers = first_response(url)
        linked_sha = (headers.get("X-Linked-ETag") or "").strip('"')
        linked_size = headers.get("X-Linked-Size")
        if status in (301, 302, 303, 307, 308) and len(linked_sha) == 64:
            problems = []
            if linked_sha != want_sha:
                problems.append(f"sha256 {linked_sha}, expected {want_sha}")
            if want_size is not None and int(linked_size or -1) != want_size:
                problems.append(f"size {linked_size}, expected {want_size}")
            followed = final_status(url)
            if followed != 200:
                problems.append(f"download answered {followed}")
            return problems
        if status not in (200, 301, 302, 303, 307, 308):
            return [f"answered {status}"]
    status, size, sha = download_hash(url)
    if status != 200:
        return [f"answered {status}"]
    problems = []
    if sha != want_sha:
        problems.append(f"sha256 {sha}, expected {want_sha}")
    if want_size is not None and size != want_size:
        problems.append(f"size {size}, expected {want_size}")
    return problems


def transient(problems):
    """A server hiccup (429 or 5xx), worth asking again; not a wrong file."""
    return bool(problems) and all(
        any(f"answered {code}" in p for code in ("429", "500", "502", "503", "504")) for p in problems
    )


def check_with_retries(entry):
    """Network errors and temporary server errors are retried, then reported."""
    for attempt in range(4):
        try:
            problems = check(entry)
            if not transient(problems):
                return problems
            last = problems
        except Exception as error:  # noqa: BLE001
            last = [f"{type(error).__name__}: {error}"]
        time.sleep(5 * (attempt + 1))
    return last


def main():
    entries = json.load(open(sys.argv[1], encoding="utf-8"))
    failures = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        for entry, problems in zip(entries, pool.map(check_with_retries, entries)):
            mark = "FAIL" if problems else "ok  "
            print(f"{mark} {entry['what']}: {entry['url']}" + (f"\n       {'; '.join(problems)}" if problems else ""), flush=True)
            if problems:
                failures.append(entry["url"])
    print(f"\n{len(entries) - len(failures)} of {len(entries)} downloads answer 200 with the pinned size and SHA-256.")
    if failures:
        print(f"{len(failures)} broken: the release must not go out until they're fixed.")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
