#!/usr/bin/env python3
"""Every update the same way: Linux, Windows and Android — and the server's
Docker image when the server changed. Run from `source/`, in this order:

  python3 tools/build/release.py build [--server]
      tests, then Linux (AppImage + deb), Android (one APK: app and AW
      Server) and with --server the Docker image, into ../programs/
  python3 tools/build/release.py windows
      the Windows installer of the commit pushed (HEAD), built by the
      workflow, signed here, into ../programs/windows-x86_64/
  python3 tools/build/release.py publish "What is new" [--server]
      latest.json, the GitHub release v<version> with every package, and
      with --server the image as <version> and latest on Docker Hub

`build` needs nothing pushed; `windows` and `publish` need HEAD on GitHub.
The keys stay outside the project (docs/development/builds.md).
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
PROGRAMS = ROOT.parent / "programs"
APP = ROOT / "apps/desktop"
REPO = "dicefoxcreations/AnotherWorldbuilder"
IMAGE = "index.docker.io/dicefoxcreations/anotherworldbuilder-server"


def version() -> str:
    return json.loads((APP / "src-tauri/tauri.conf.json").read_text())["version"]


def run(*command, cwd=ROOT, env=None, capture=False):
    print("→", " ".join(str(c) for c in command), flush=True)
    done = subprocess.run([str(c) for c in command], cwd=cwd, env=env, check=True, text=True, capture_output=capture)
    return done.stdout if capture else None


def node_env():
    # Since kernel 7.2 / node 26 a file read via io_uring can hang for good.
    return dict(os.environ, UV_USE_IO_URING="0")


def head() -> str:
    return run("git", "rev-parse", "HEAD", capture=True).strip()


def build(server: bool):
    if run("git", "status", "--porcelain", "--", ".", capture=True).strip():
        sys.exit("Commit the changes in source/ first: the packages name the commit they are built from.")
    run("cargo", "test", "--workspace", "--locked")
    run("npm", "test", cwd=APP, env=node_env())
    env = node_env()
    run("node", "tools/build/desktop.mjs", "--target", "x86_64-unknown-linux-gnu", env=env)
    run("node", "tools/build/collect-release.mjs", "x86_64-unknown-linux-gnu", env=env)
    run(sys.executable, "tools/build/android.py", env=dict(env, JAVA_HOME=os.environ.get("JAVA_HOME", "/usr/lib/jvm/java-17-openjdk")))
    if server:
        run(sys.executable, "tools/build/server-image.py")
        out = PROGRAMS / "server-linux-x86_64"
        for old in out.glob("aw-server-*-docker.tar"):
            if old.name != f"aw-server-{version()}-docker.tar":
                old.unlink()
        sums = "".join(f"{hashlib.sha256((out / n).read_bytes()).hexdigest()}  {n}\n" for n in ("aw-server", f"aw-server-{version()}-docker.tar"))
        (out / "SHA256SUMS.txt").write_text(sums)
    print(f"\nBuilt {version()}. Next: commit programs/, push, then `release.py windows`.")


def windows():
    commit = head()
    if run("git", "branch", "-r", "--contains", commit, capture=True).strip() == "":
        sys.exit("Push HEAD first — the workflow builds it from GitHub.")
    run("gh", "workflow", "run", "windows-installer.yml", "-R", REPO, "-f", f"ref={commit}")
    # The run just started: the newest one for this commit.
    for _ in range(30):
        time.sleep(4)
        runs = json.loads(run("gh", "run", "list", "-R", REPO, "--workflow", "windows-installer.yml", "-L", "10",
                              "--json", "databaseId,headSha,createdAt", capture=True))
        mine = [r for r in runs if r["headSha"] == commit]
        if mine:
            break
    else:
        sys.exit("The workflow run did not show up.")
    started = max(mine, key=lambda r: r["createdAt"])
    run("gh", "run", "watch", str(started["databaseId"]), "-R", REPO, "--exit-status")
    nsis = ROOT / "build/cargo/x86_64-pc-windows-msvc/release/bundle/nsis"
    shutil.rmtree(nsis, ignore_errors=True)
    nsis.mkdir(parents=True)
    run("gh", "run", "download", str(started["databaseId"]), "-R", REPO, "-n", "windows-installer", "-D", nsis)
    setup = nsis / f"AnotherWorldbuilder_{version()}_x64-setup.exe"
    env = node_env()
    run("node", "tools/build/sign-update.mjs", setup, env=env)
    run("node", "tools/build/collect-release.mjs", "x86_64-pc-windows-msvc", env=env)
    print(f"\nWindows {version()} collected. Commit programs/, push, then `release.py publish`.")


def publish(notes: str, server: bool):
    v = version()
    env = node_env()
    run("node", "tools/build/update-manifest.mjs", notes, env=env)
    files = [
        PROGRAMS / f"linux-x86_64/AnotherWorldbuilder_{v}_amd64.AppImage",
        PROGRAMS / f"linux-x86_64/AnotherWorldbuilder_{v}_amd64.deb",
        PROGRAMS / f"windows-x86_64/AnotherWorldbuilder_{v}_x64-setup.exe",
        PROGRAMS / f"android/AnotherWorldbuilder_{v}_android.apk",
        PROGRAMS / "latest.json",
    ]
    if server:
        files += [PROGRAMS / "server-linux-x86_64/aw-server", PROGRAMS / f"server-linux-x86_64/aw-server-{v}-docker.tar"]
    missing = [str(f) for f in files if not f.exists()]
    if missing:
        sys.exit("Missing: " + ", ".join(missing))
    with tempfile.TemporaryDirectory() as extra:
        # Unchanged between versions: taken over from the latest release.
        run("gh", "release", "download", "-R", REPO, "-p", "DnD-5e-template.zip", "-p", "Example-world.zip", "-D", extra)
        if not server:
            run("gh", "release", "download", "-R", REPO, "-p", "aw-server", "-p", "aw-server-*-docker.tar", "-D", extra)
        files += sorted(Path(extra).iterdir())
        image = next(f.name for f in files if f.name.endswith("-docker.tar"))
        rows = [("AnotherWorldbuilder_%s_amd64.AppImage" % v, "Linux, no installation; updates itself"),
                ("AnotherWorldbuilder_%s_amd64.deb" % v, "Linux (Debian, Ubuntu), installed"),
                ("AnotherWorldbuilder_%s_x64-setup.exe" % v, "Windows installer, for the current user; updates itself"),
                ("AnotherWorldbuilder_%s_android.apk" % v, "Android 8 or later: the app and AW Server, two icons"),
                ("Example-world.zip", "A small made-up example world: unzip, create a world, then *Options → Transfer → Import*"),
                ("DnD-5e-template.zip", "Template for D&D 5e (types and outline only, no content): unzip, then *Options → Transfer → Import*"),
                ("aw-server", "The server without a window (Linux, static) — for a NAS or a headless machine"),
                (image, "The server as a Docker image; simpler: `dicefoxcreations/anotherworldbuilder-server` from Docker Hub"),
                ("latest.json", "What installed apps ask for when looking for updates")]
        table = "| File | For |\n| --- | --- |\n" + "\n".join(f"| `{n}` | {t} |" for n, t in rows)
        sums = "\n".join(f"{hashlib.sha256(f.read_bytes()).hexdigest()}  {f.name}" for f in files)
        body = (f"**In development** – not meant for real use yet."
                + (f" **Update the server too** (Docker: `dicefoxcreations/anotherworldbuilder-server:{v}`)." if server else "")
                + f"\n\n{notes}\n\n{table}\n\n### Checksums (SHA-256)\n\n```\n{sums}\n```\n")
        notes_file = Path(extra) / "notes.md"
        notes_file.write_text(body)
        run("gh", "release", "create", f"v{v}", "-R", REPO, "--title", v, "--notes-file", notes_file, "--latest", "--target", head(), *files)
    if server:
        tar = PROGRAMS / f"server-linux-x86_64/aw-server-{v}-docker.tar"
        crane = ROOT / "build/tools/crane"
        for tag in (v, "latest"):
            run(crane, "push", tar, f"{IMAGE}:{tag}")
    print(f"\nReleased {v}.")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    steps = parser.add_subparsers(dest="step", required=True)
    b = steps.add_parser("build")
    b.add_argument("--server", action="store_true", help="the Docker image too")
    steps.add_parser("windows")
    p = steps.add_parser("publish")
    p.add_argument("notes")
    p.add_argument("--server", action="store_true", help="the Docker image too")
    args = parser.parse_args()
    if args.step == "build":
        build(args.server)
    elif args.step == "windows":
        windows()
    else:
        publish(args.notes, args.server)


if __name__ == "__main__":
    main()
