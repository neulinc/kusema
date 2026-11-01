# /// script
# requires-python = ">=3.10"
# dependencies = ["huggingface_hub[hf_xet]"]
# ///
"""
Publishes Kusema's model mirror to the `kusema` organization on Hugging Face.

Uploads each file byte for byte, so every size and SHA-256 Kusema pins still
holds, with a model card and the license files each license asks for. Then it
checks the Hub's record of each file, and downloads each one back from its
public URL and hashes the actual bytes.

Which files are hosted is decided here, and must match:
  - `KUSEMA_MIRRORS` in src-tauri/src/catalog/mod.rs (catalog models)
  - the `ModelSource::Url` entries in src-tauri/src/managers/model.rs (legacy)

Run after `hf auth login` with write access to the organization. This uses the
stored login and never reads or prints the token. Safe to re-run: anything
already published and correct is left alone.

  python scripts/kusema_mirror.py [--tmpdir DIR] [--verify-only]

Keep --tmpdir off a nearly full drive: the legacy files pass through it (the
Hub's own cache is pointed there too).
"""
import argparse
import hashlib
import html
import json
import os
import re
import shutil
import sys
import tempfile
import urllib.request
from pathlib import Path

ORG = "kusema"
CHUNK = 8 * 1024 * 1024
ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "src-tauri" / "src" / "catalog" / "catalog.json"

URLS = {
    "apache": "https://www.apache.org/licenses/LICENSE-2.0.txt",
    "cc-by": "https://creativecommons.org/licenses/by/4.0/legalcode.txt",
    "whisper": "https://raw.githubusercontent.com/openai/whisper/main/LICENSE",
    "whisper.cpp": "https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/LICENSE",
    "nvidia": "https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-license/",
    "openmdw": "https://openmdw.ai/license/1-1/",
}

# Files once served from Handy's own server. Sizes and hashes are what
# src-tauri/src/managers/model.rs pins (or, for size, what that server reported).
HANDY_BLOB = "https://blob.handy.computer"
WHISPER_GGML = {
    "ggml-small.bin": (487601967, "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b"),
    "whisper-medium-q4_1.bin": (491852915, "79283fc1f9fe12ca3248543fbd54b73292164d8df5a16e095e2bceeaaabddf57"),
    "ggml-large-v3-turbo.bin": (1624555275, "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69"),
    "ggml-large-v3-q5_0.bin": (1081140203, "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1"),
}
BREEZE_GGML = {
    "breeze-asr-q5_k.bin": (1080732108, "8efbf0ce8a3f50fe332b7617da787fb81354b358c288b008d3bdef8359df64c6"),
}

# Catalog models hosted here: their default quant only, the file Kusema downloads.
CATALOG_MODELS = {
    "parakeet-unified-en-0.6b": {
        "authors": "NVIDIA",
        "license": "nvidia",
    },
    "parakeet-tdt-0.6b-v3": {
        "authors": "NVIDIA",
        "license": "cc-by",
    },
    "nemotron-3.5-asr-streaming-0.6b": {
        "authors": "NVIDIA",
        "license": "openmdw",
    },
    "canary-180m-flash": {
        "authors": "NVIDIA",
        "license": "cc-by",
    },
}

LICENSE_META = {
    "nvidia": ("license: other\nlicense_name: nvidia-open-model-license\n"
               f"license_link: {URLS['nvidia']}", "NVIDIA Open Model License"),
    "cc-by": ("license: cc-by-4.0", "Creative Commons Attribution 4.0 International (CC-BY-4.0)"),
    "openmdw": (f"license: other\nlicense_name: openmdw-1.1\nlicense_link: {URLS['openmdw']}", "OpenMDW License 1.1"),
    "mit": ("license: mit", "MIT"),
    "apache": ("license: apache-2.0", "Apache License 2.0"),
}
NOTICE_NVIDIA = "Licensed by NVIDIA Corporation under the NVIDIA Open Model License\n"
MIRROR_NOTE = (
    "**This is a mirror for [Kusema](https://github.com/neulinc/kusema), a Windows dictation app.** "
    "The file is an unmodified copy, so its size and SHA-256 match what Kusema checks after every "
    "download. Kusema's authors did not create or train this model and are not affiliated with or "
    "endorsed by its authors. Use the original source below unless you need this copy."
)


def say(message):
    print(message, flush=True)


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0 kusema-mirror"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read().decode("utf-8", "replace")


def page_text(url):
    """The readable text of a web page holding a license."""
    text = re.sub(r"<(script|style).*?</\1>", " ", fetch(url), flags=re.S)
    text = re.sub(r"<!--.*?-->", " ", text, flags=re.S)
    text = html.unescape(re.sub(r"<[^>]+>", " ", text))
    lines = (re.sub(r"[ \t\r\f]+", " ", line).strip() for line in text.splitlines())
    return "\n".join(line for line in lines if line)


def between(text, start, end):
    """The part of `text` from `start` through the line holding `end`."""
    first = text.index(start)
    last = text.index(end, first)
    last = text.find("\n", last)
    return text[first:last].strip() + "\n"


def license_files(kind):
    """The files a repo needs under this license."""
    if kind == "apache":
        return {"LICENSE": fetch(URLS["apache"])}
    if kind == "cc-by":
        return {"LICENSE": fetch(URLS["cc-by"])}
    if kind == "mit":
        return {"LICENSE": f"OpenAI Whisper:\n\n{fetch(URLS['whisper'])}\n\nwhisper.cpp (GGML format):\n\n{fetch(URLS['whisper.cpp'])}"}
    if kind == "nvidia":
        agreement = between(page_text(URLS["nvidia"]), "NVIDIA Open Model License Agreement", "Version Release Date")
        return {"LICENSE": f"Source: {URLS['nvidia']}\n\n{agreement}", "NOTICE": NOTICE_NVIDIA}
    if kind == "openmdw":
        agreement = between(page_text(URLS["openmdw"]), "OpenMDW License Agreement, version 1.1", "OUT OF OR IN CONNECTION")
        return {"LICENSE": f"Source: {URLS['openmdw']}\n\n{agreement}"}
    raise ValueError(kind)


def file_table(files):
    rows = "\n".join(f"| `{name}` | {size:,} | `{sha}` |" for name, (size, sha) in files.items())
    return f"| File | Bytes | SHA-256 |\n| --- | --- | --- |\n{rows}\n"


def card_for_catalog(model, kind, authors, name, entry):
    base = entry["base"]
    meta, license_name = LICENSE_META[kind]
    extra = ""
    if kind == "nvidia":
        extra = (
            "\nThis repo includes a copy of the agreement (`LICENSE`) and the required attribution "
            "(`NOTICE`): Licensed by NVIDIA Corporation under the NVIDIA Open Model License.\n"
        )
    return f"""---
{meta}
base_model: {base}
base_model_relation: quantized
library_name: transcribe.cpp
pipeline_tag: automatic-speech-recognition
tags:
  - gguf
  - transcribe.cpp
  - mirror
---

# {name}

{MIRROR_NOTE}

## Source and attribution

- **Original model:** [{base}](https://huggingface.co/{base}), by {authors}.
- **License:** {license_name}.{extra}
- **Changes from the original:** converted to GGUF and quantized by the transcribe.cpp project, as
  [{entry['repo']}](https://huggingface.co/{entry['repo']}) at revision `{entry['revision']}`. This
  repo changes nothing further, and hosts only the one file Kusema downloads.
- **Runtime:** [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp).

See the original model's page for its intended uses, limitations and benchmarks.

## File

{file_table(entry['files'])}"""


def card_for_whisper(files):
    meta, license_name = LICENSE_META["mit"]
    return f"""---
{meta}
library_name: whisper.cpp
pipeline_tag: automatic-speech-recognition
tags:
  - ggml
  - whisper
  - mirror
---

# Whisper (GGML)

{MIRROR_NOTE}

## Source and attribution

- **Original model:** [Whisper](https://github.com/openai/whisper), by OpenAI (Radford et al.,
  "Robust Speech Recognition via Large-Scale Weak Supervision").
- **License:** {license_name}. Copyright OpenAI; the GGML format and tools are copyright the ggml
  authors. Both notices are in `LICENSE`.
- **Changes from the original:** converted to the GGML format of
  [whisper.cpp](https://github.com/ggml-org/whisper.cpp), and some quantized (`q4_1`, `q5_0`). Copied
  from the files Handy distributed, which Kusema is based on.

## Files

{file_table(files)}"""


def card_for_breeze(files):
    meta, license_name = LICENSE_META["apache"]
    return f"""---
{meta}
base_model: MediaTek-Research/Breeze-ASR-25
base_model_relation: quantized
library_name: whisper.cpp
pipeline_tag: automatic-speech-recognition
tags:
  - ggml
  - mirror
---

# Breeze ASR 25 (GGML)

{MIRROR_NOTE}

## Source and attribution

- **Original model:** [MediaTek-Research/Breeze-ASR-25](https://huggingface.co/MediaTek-Research/Breeze-ASR-25),
  by MediaTek Research.
- **License:** {license_name}. A copy is in `LICENSE`.
- **Changes from the original:** converted to the GGML format of
  [whisper.cpp](https://github.com/ggml-org/whisper.cpp) and quantized (`q5_k`). Copied from the file
  Handy distributed, which Kusema is based on.

## Files

{file_table(files)}"""


def sha256_of(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(CHUNK), b""):
            digest.update(chunk)
    return digest.hexdigest()


def download_to(url, path):
    request = urllib.request.Request(url, headers={"User-Agent": "kusema-mirror"})
    with urllib.request.urlopen(request, timeout=120) as response, open(path, "wb") as out:
        shutil.copyfileobj(response, out, CHUNK)


def stream_hash(url):
    """Hash and length of what a public URL actually serves, without saving it."""
    digest = hashlib.sha256()
    length = 0
    request = urllib.request.Request(url, headers={"User-Agent": "kusema-mirror"})
    with urllib.request.urlopen(request, timeout=120) as response:
        for chunk in iter(lambda: response.read(CHUNK), b""):
            digest.update(chunk)
            length += len(chunk)
    return digest.hexdigest(), length


def on_hub(api, repo):
    """Files already published, by name, as (size, sha256)."""
    found = {}
    for item in api.list_repo_tree(repo, recursive=True):
        lfs = getattr(item, "lfs", None)
        if lfs is not None:
            found[item.path] = (item.size, lfs.sha256)
    return found


def publish_text(api, repo, files):
    from huggingface_hub import CommitOperationAdd

    operations = [
        CommitOperationAdd(path_in_repo=name, path_or_fileobj=text.encode("utf-8"))
        for name, text in files.items()
    ]
    api.create_commit(repo, operations, commit_message="Model card and license files")


def publish(api, plan, tmp, verify_only):
    repo = plan["repo"]
    say(f"\n=== {repo}")
    if not verify_only:
        api.create_repo(repo, repo_type="model", private=False, exist_ok=True)
        present = on_hub(api, repo)
        for name, (size, sha) in plan["files"].items():
            if present.get(name) == (size, sha):
                say(f"  {name}: already there")
                continue
            source = plan["source"](name)
            if source["kind"] == "copy":
                try:
                    api.copy_files(source["hf"], f"hf://{repo}/{name}")
                    say(f"  {name}: copied on the server")
                    continue
                except Exception as error:  # noqa: BLE001 - falls back to moving the bytes
                    say(f"  {name}: server copy unavailable ({type(error).__name__}); moving the bytes")
            path = Path(tmp) / name
            say(f"  {name}: downloading {source['url']}")
            download_to(source["url"], path)
            actual = sha256_of(path)
            if actual != sha or path.stat().st_size != size:
                raise SystemExit(f"{name}: source doesn't match the pinned hash ({actual})")
            say(f"  {name}: source verified; uploading")
            api.upload_file(path_or_fileobj=str(path), path_in_repo=name, repo_id=repo, commit_message=f"Add {name}")
            path.unlink()
        publish_text(api, repo, {"README.md": plan["card"], **plan["licenses"]})

    present = on_hub(api, repo)
    for name, (size, sha) in plan["files"].items():
        if present.get(name) != (size, sha):
            raise SystemExit(f"{name}: the Hub reports {present.get(name)}, expected {(size, sha)}")
        actual, length = stream_hash(f"https://huggingface.co/{repo}/resolve/main/{name}")
        if (length, actual) != (size, sha):
            raise SystemExit(f"{name}: downloaded {length} bytes hashing {actual}, expected {size} {sha}")
        say(f"  {name}: OK, {size:,} bytes, sha256 {sha}")


def plans():
    catalog = json.loads(CATALOG.read_text(encoding="utf-8"))
    by_id = {m["id"]: m for m in catalog["models"]}
    for model_id, info in CATALOG_MODELS.items():
        model = by_id[model_id]
        default = model["files"][model["quant"]]
        repo_name = model["repository"].split("/")[1]
        base = re.search(r"^base_model: (\S+)", fetch(f"https://huggingface.co/{model['repository']}/raw/{model['revision']}/README.md"), re.M).group(1)
        files = {default["filename"]: (default["size"], default["sha256"])}
        entry = {"base": base, "repo": model["repository"], "revision": model["revision"], "files": files}
        yield {
            "repo": f"{ORG}/{repo_name}",
            "files": files,
            "card": card_for_catalog(model, info["license"], info["authors"], model["name"], entry),
            "licenses": license_files(info["license"]),
            "source": lambda name, m=model: {
                "kind": "copy",
                "hf": f"hf://{m['repository']}@{m['revision']}/{name}",
                "url": f"https://huggingface.co/{m['repository']}/resolve/{m['revision']}/{name}",
            },
        }
    legacy_source = lambda name: {"kind": "url", "url": f"{HANDY_BLOB}/{name}"}
    yield {
        "repo": f"{ORG}/whisper-ggml",
        "files": WHISPER_GGML,
        "card": card_for_whisper(WHISPER_GGML),
        "licenses": license_files("mit"),
        "source": legacy_source,
    }
    yield {
        "repo": f"{ORG}/breeze-asr-ggml",
        "files": BREEZE_GGML,
        "card": card_for_breeze(BREEZE_GGML),
        "licenses": license_files("apache"),
        "source": legacy_source,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tmpdir")
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()

    tmp = args.tmpdir or tempfile.mkdtemp(prefix="kusema-mirror-")
    os.makedirs(tmp, exist_ok=True)
    os.environ["HF_HUB_CACHE"] = os.path.join(tmp, "hub")
    os.environ["HF_XET_CACHE"] = os.path.join(tmp, "xet")
    from huggingface_hub import HfApi

    api = HfApi()
    say(f"Publishing as {api.whoami().get('name')}")
    all_plans = list(plans())
    for plan in all_plans:
        publish(api, plan, tmp, args.verify_only)
    say(f"\nAll {sum(len(p['files']) for p in all_plans)} files verified.")


if __name__ == "__main__":
    sys.exit(main())
