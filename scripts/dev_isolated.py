"""
Runs a debug build of Kusema with no access to the real user's data. This is the
only way to launch a build for testing: see "Testing rule" in the build plan.

  python scripts/dev_isolated.py start [--set NAME=JSON ...] [--env NAME=VALUE ...]
  python scripts/dev_isolated.py stop [--keep]

`start` sets everything up first, then launches `bun run tauri dev`, then checks
the app is really using the isolated folder. If it can't confirm that, it stops
the app and exits with an error. `stop` ends the run, removes the test data
(unless --keep) and reports whether anything real changed.

What isolation means, and what enforces it:
  - Settings, History, the ledger, models, recordings, logs and WebView data live
    in src-tauri/target/debug/Data (portable mode, from a marker next to the exe).
  - KUSEMA_DEBUG_ISOLATED makes the app refuse to start unless portable mode is
    on, keep API keys in memory instead of Credential Manager, leave the Windows
    startup entry alone, and refuse to touch the clipboard.
  - HF_HOME points inside the test data, so the shared model cache isn't seen.

Windows only. Never reads the contents of the real keychain or settings values.
"""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import winreg
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEBUG = REPO / "src-tauri" / "target" / "debug"
DATA = DEBUG / "Data"
MARKER = DEBUG / "portable"
STATE = DEBUG / "isolated-run.json"
RUN_LOG = DEBUG / "isolated-run.log"
APP_LOG = DATA / "logs" / "handy.log"
READY_TEXT = "Shortcuts initialized successfully"
START_TIMEOUT_SECS = 580

REAL = Path(os.environ["APPDATA"]) / "com.neulinc.kusema"
REAL_HF_HUB = Path.home() / ".cache" / "huggingface" / "hub"
RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"

# Nothing here may reach the clipboard or the network without being asked to.
DEFAULT_SETTINGS = {
    "onboarding_completed": True,
    "paste_method": "direct",
    "clipboard_handling": "dont_modify",
    "update_checks_enabled": False,
    "autostart_enabled": False,
    "log_level": "debug",
    "start_hidden": False,
}


def say(message):
    print(message, flush=True)


def digest(path):
    if not path.exists():
        return None
    # Modification time and a hash: enough to see a change without showing anything.
    return [path.stat().st_mtime_ns, hashlib.sha256(path.read_bytes()).hexdigest()]


def startup_entries():
    entries = {}
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY) as key:
            for index in range(winreg.QueryInfoKey(key)[1]):
                name, value, _ = winreg.EnumValue(key, index)
                if "kusema" in name.lower() or "handy" in name.lower() or "kusema" in str(value).lower():
                    entries[name] = str(value)
    except OSError:
        pass
    return entries


def real_snapshot():
    """Everything real an isolated run must leave alone."""
    models = REAL / "models"
    return {
        "settings": digest(REAL / "settings_store.json"),
        "ledger": digest(REAL / "privacy_ledger.db"),
        "history": digest(REAL / "history.db"),
        "models": sorted(p.name for p in models.iterdir()) if models.exists() else [],
        "recordings": sorted(p.name for p in (REAL / "recordings").iterdir()) if (REAL / "recordings").exists() else [],
        "hf_cache": sorted(p.name for p in REAL_HF_HUB.iterdir()) if REAL_HF_HUB.exists() else [],
        "startup": startup_entries(),
    }


def debug_processes():
    script = (
        "Get-CimInstance Win32_Process -Filter \"Name='handy.exe'\" | "
        "Where-Object { $_.ExecutablePath -like '*target\\debug\\handy.exe' } | "
        "ForEach-Object { $_.ProcessId }"
    )
    out = subprocess.run(["powershell", "-NoProfile", "-Command", script], capture_output=True, text=True).stdout
    return [int(line) for line in out.split()]


def kill_tree(pid):
    subprocess.run(["taskkill", "/PID", str(pid), "/T", "/F"], capture_output=True)


def stop_run():
    """Ends this script's run, and any debug build or dev server left over."""
    if STATE.exists():
        kill_tree(json.loads(STATE.read_text(encoding="utf-8"))["pid"])
    for pid in debug_processes():
        kill_tree(pid)
    script = (
        "Get-CimInstance Win32_Process | Where-Object { $_.Name -match '^(bun|node|esbuild)\\.exe$' -and "
        "$_.CommandLine -match 'kusema\\\\node_modules|tauri dev|bun  ?run dev' } | ForEach-Object { $_.ProcessId }"
    )
    out = subprocess.run(["powershell", "-NoProfile", "-Command", script], capture_output=True, text=True).stdout
    for pid in out.split():
        kill_tree(int(pid))
    time.sleep(2)


def compare(before, label):
    after = real_snapshot()
    changed = [name for name in before if before[name] != after[name]]
    if changed:
        say(f"WARNING: real data changed {label}: {', '.join(changed)}")
        say("(Expected only if you used an installed Kusema meanwhile. Otherwise: investigate and restore.)")
    else:
        say(f"Real data unchanged {label}.")
    return not changed


def start(args):
    if debug_processes():
        raise SystemExit("A debug build is already running. Stop it first.")

    # Setup comes first, and finishes before anything is launched.
    before = real_snapshot()
    DATA.mkdir(parents=True, exist_ok=True)
    (DATA / "huggingface").mkdir(exist_ok=True)
    settings_path = DATA / "settings_store.json"
    if settings_path.exists():
        store = json.loads(settings_path.read_text(encoding="utf-8"))
    else:
        store = {"settings": {}}
    store["settings"].update(DEFAULT_SETTINGS)
    for item in args.set:
        name, _, value = item.partition("=")
        store["settings"][name] = json.loads(value)
    settings_path.write_text(json.dumps(store, indent=2), encoding="utf-8")
    MARKER.write_text("Handy Portable Mode")

    if not MARKER.exists() or not settings_path.exists():
        raise SystemExit("Setup failed; nothing was launched.")
    say(f"Isolated data folder ready: {DATA}")

    env = dict(os.environ)
    env["KUSEMA_DEBUG_ISOLATED"] = "1"
    env["HF_HOME"] = str(DATA / "huggingface")
    for item in args.env:
        name, _, value = item.partition("=")
        env[name] = value

    offset = APP_LOG.stat().st_size if APP_LOG.exists() else 0
    with open(RUN_LOG, "wb") as log:
        process = subprocess.Popen(
            "bun run tauri dev",
            cwd=REPO,
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT,
            shell=True,
            creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
        )
    STATE.write_text(json.dumps({"pid": process.pid, "real": before}), encoding="utf-8")

    # The app has to prove it's isolated before this says so.
    deadline = time.time() + START_TIMEOUT_SECS
    while time.time() < deadline:
        time.sleep(2)
        if process.poll() is not None:
            break
        if APP_LOG.exists():
            with open(APP_LOG, encoding="utf-8", errors="replace") as handle:
                handle.seek(offset)
                if READY_TEXT in handle.read():
                    break
    else:
        stop_run()
        raise SystemExit("ABORTED: the app didn't start in time. Nothing further was run.")

    run_log = RUN_LOG.read_text(encoding="utf-8", errors="replace")
    in_use = (
        process.poll() is None
        and f"[portable] data dir: {DATA}" in run_log
        and (DATA / "privacy_ledger.db").exists()
        and compare(before, "after launching")
    )
    if not in_use:
        stop_run()
        raise SystemExit(f"ABORTED: couldn't confirm the isolated folder is in use. See {RUN_LOG}")
    say("Verified: the app is using the isolated folder.")


def stop(args):
    stop_run()
    before = json.loads(STATE.read_text(encoding="utf-8"))["real"] if STATE.exists() else None
    if MARKER.exists():
        MARKER.unlink()
    STATE.unlink(missing_ok=True)
    if not args.keep:
        shutil.rmtree(DATA, ignore_errors=True)
    say("Run stopped." if args.keep else "Run stopped and test data removed.")
    if before is not None and not compare(before, "by this run"):
        sys.exit(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    start_parser = commands.add_parser("start")
    start_parser.add_argument("--set", action="append", default=[], metavar="NAME=JSON")
    start_parser.add_argument("--env", action="append", default=[], metavar="NAME=VALUE")
    stop_parser = commands.add_parser("stop")
    stop_parser.add_argument("--keep", action="store_true", help="keep the test data for another run")
    args = parser.parse_args()
    {"start": start, "stop": stop}[args.command](args)


if __name__ == "__main__":
    main()
