#!/usr/bin/env python3
"""Builds aw-server as one static binary and packs it as a Docker image
archive — the format `docker save` writes and `docker load` or Synology's
Container Manager (Image → Add → Add from file) take in. No Docker needed.

  python3 tools/build/server-image.py [out-dir]

The image holds nothing but the binary and an empty /data: the binary is
static (musl), so no system underneath is needed.
"""
import gzip, hashlib, io, json, os, subprocess, sys, tarfile, time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TARGET = "x86_64-unknown-linux-musl"
VERSION = None
with open(os.path.join(ROOT, "Cargo.toml")) as f:
    for line in f:
        if line.startswith("version ="):
            VERSION = line.split('"')[1]
            break

def build():
    env = dict(os.environ)
    env.setdefault("CARGO_TARGET_DIR", os.path.join(ROOT, "build", "cargo"))
    # The binary names no folder of this machine (as tools/build/paths.mjs).
    env["RUSTFLAGS"] = " ".join(f for f in [env.get("RUSTFLAGS", ""), f"--remap-path-prefix={os.path.expanduser('~')}=/build",
                                            f"--remap-path-prefix={ROOT}=/aw"] if f)
    # The C parts (ring) for musl: clang can aim there without a musl toolchain.
    env.setdefault("CC_x86_64_unknown_linux_musl", "clang")
    env.setdefault("CFLAGS_x86_64_unknown_linux_musl", "--target=x86_64-unknown-linux-musl")
    subprocess.run(["cargo", "build", "--release", "--locked", "-p", "aw-desktop", "--no-default-features",
                    "--bin", "aw-server", "--target", TARGET], cwd=ROOT, env=env, check=True)
    return os.path.join(env["CARGO_TARGET_DIR"], TARGET, "release", "aw-server")

def layer(binary, notices):
    """The one layer: /usr/local/bin/aw-server, its licence notices, /data, /tmp."""
    buf = io.BytesIO()
    now = int(time.time())
    with tarfile.open(fileobj=buf, mode="w", format=tarfile.PAX_FORMAT) as tar:
        dirs = [("usr", 0o755), ("usr/local", 0o755), ("usr/local/bin", 0o755), ("usr/share", 0o755), ("usr/share/doc", 0o755),
                ("usr/share/doc/aw-server", 0o755), ("data", 0o777), ("tmp", 0o1777)]
        for d, mode in dirs:
            info = tarfile.TarInfo(d); info.type = tarfile.DIRTYPE; info.mode = mode; info.mtime = now
            tar.addfile(info)
        info = tar.gettarinfo(binary, "usr/local/bin/aw-server")
        info.mode = 0o755; info.uid = info.gid = 0; info.uname = info.gname = "root"; info.mtime = now
        with open(binary, "rb") as f:
            tar.addfile(info, f)
        # Every copy of the program carries the notices of what is in it.
        info = tar.gettarinfo(notices, "usr/share/doc/aw-server/THIRD-PARTY-NOTICES.txt")
        info.mode = 0o644; info.uid = info.gid = 0; info.uname = info.gname = "root"; info.mtime = now
        with open(notices, "rb") as f:
            tar.addfile(info, f)
    return buf.getvalue()

def image(binary, notices, out):
    data = layer(binary, notices)
    diff = "sha256:" + hashlib.sha256(data).hexdigest()
    created = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    config = {
        "architecture": "amd64",
        "os": "linux",
        "created": created,
        "config": {
            "Entrypoint": ["/usr/local/bin/aw-server"],
            # Shown in a NAS's container dialog: what is left to fill in.
            "Env": ["AW_ADMIN=", "AW_NAME=", "AW_PORT=7447"],
            "ExposedPorts": {"7447/udp": {}, "7447/tcp": {}},
            "Volumes": {"/data": {}},
            "WorkingDir": "/data",
            "Labels": {"org.opencontainers.image.title": "aw-server", "org.opencontainers.image.version": VERSION,
                       "org.opencontainers.image.description": "AnotherWorldbuilder server — in development, not ready for real use yet."},
        },
        "rootfs": {"type": "layers", "diff_ids": [diff]},
        "history": [{"created": created, "created_by": "tools/build/server-image.py"}],
    }
    config_bytes = json.dumps(config, separators=(",", ":")).encode()
    config_name = hashlib.sha256(config_bytes).hexdigest() + ".json"
    layer_dir = hashlib.sha256(data).hexdigest()
    tag = f"aw-server:{VERSION}"
    manifest = [{"Config": config_name, "RepoTags": [tag, "aw-server:latest"], "Layers": [f"{layer_dir}/layer.tar"]}]
    repositories = {"aw-server": {VERSION: layer_dir, "latest": layer_dir}}
    with tarfile.open(out, mode="w", format=tarfile.PAX_FORMAT) as tar:
        def put(name, content):
            info = tarfile.TarInfo(name); info.size = len(content); info.mtime = int(time.time()); info.mode = 0o644
            tar.addfile(info, io.BytesIO(content))
        dinfo = tarfile.TarInfo(layer_dir); dinfo.type = tarfile.DIRTYPE; dinfo.mode = 0o755; dinfo.mtime = int(time.time())
        tar.addfile(dinfo)
        put(f"{layer_dir}/VERSION", b"1.0")
        put(f"{layer_dir}/json", json.dumps({"id": layer_dir}).encode())
        put(f"{layer_dir}/layer.tar", data)
        put(config_name, config_bytes)
        put("manifest.json", json.dumps(manifest).encode())
        put("repositories", json.dumps(repositories).encode())
    return tag

if __name__ == "__main__":
    out_dir = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "..", "programs", "server-linux-x86_64")
    os.makedirs(out_dir, exist_ok=True)
    binary = build()
    # Without its symbols: a third smaller, the same program.
    # Written beside it, then put in place: a server running from it keeps
    # running, the next start takes the new one.
    small = os.path.join(out_dir, "aw-server")
    fresh = small + ".new"
    subprocess.run(["cp", binary, fresh], check=True)
    subprocess.run(["strip", fresh], check=False)
    os.replace(fresh, small)
    binary = small
    archive = os.path.join(out_dir, f"aw-server-{VERSION}-docker.tar")
    # The notices the desktop build collected — the server's packages are a part of them.
    notices = os.path.join(ROOT, "..", "programs", "linux-x86_64", "THIRD-PARTY-NOTICES.txt")
    if not os.path.exists(notices):
        sys.exit(f"{notices} is missing: build the Linux release first (tools/build/collect-release.mjs).")
    subprocess.run(["cp", notices, os.path.join(out_dir, "THIRD-PARTY-NOTICES.txt")], check=True)
    # How to run it, beside it.
    for name in ["compose.yaml", "Dockerfile", "README.md"]:
        subprocess.run(["cp", os.path.join(ROOT, "apps", "server", name), os.path.join(out_dir, name)], check=True)
    tag = image(binary, notices, archive)
    print(f"{archive} ({os.path.getsize(archive) // 1024 // 1024} MB) — docker load -i …  →  {tag}")
