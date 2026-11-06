#!/usr/bin/env python3
"""Builds the Android app — AnotherWorldbuilder and AW Server, one APK — and signs it.

Run from `source/`, after `npm ci` in `apps/desktop`, with Java 17, the
Android SDK 36 / NDK 28c (`ANDROID_HOME`, else `build/android-sdk`) and the
Rust targets of `TARGETS`. The release APK goes to `../programs/android/`;
`--debug` writes a debuggable one and the server's device test to
`build/android/` instead.

The signing key lies outside the project, beside the updater key:
`~/.tauri/android/anotherworldbuilder.jks` and `.password` (or
`AW_ANDROID_KEYSTORE` / `AW_ANDROID_PASSWORD_FILE`). Without it no update can
be installed over an installed app any more — keep a copy in the password
manager. Debug builds are signed with it too, so that a release installs over
a test installation and keeps its data.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib
import xml.etree.ElementTree as ET
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[2]
APP = ROOT / "apps/desktop"
ANDROID = APP / "src-tauri/gen/android"
BUILD = ROOT / "build/android"
NDK = "28.2.13676358"
TARGETS = {"aarch64": "aarch64-linux-android", "armv7": "armv7-linux-androideabi", "x86_64": "x86_64-linux-android"}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--targets", default=",".join(TARGETS), help="of " + ", ".join(TARGETS))
    parser.add_argument("--debug", action="store_true", help="debuggable APK and the server's device test, into build/android/")
    args = parser.parse_args()
    targets = args.targets.split(",")
    if any(t not in TARGETS for t in targets):
        parser.error("targets: " + ", ".join(TARGETS))

    signing = Path.home() / ".tauri/android"
    key = Path(os.environ.get("AW_ANDROID_KEYSTORE", signing / "anotherworldbuilder.jks"))
    password = Path(os.environ.get("AW_ANDROID_PASSWORD_FILE", signing / "anotherworldbuilder.password"))
    if not key.exists() or not password.exists():
        parser.error(f"signing key missing: {key} and {password}")

    sdk = Path(os.environ.get("ANDROID_HOME", ROOT / "build/android-sdk")).resolve()
    env = dict(os.environ, ANDROID_HOME=str(sdk), NDK_HOME=str(Path(os.environ.get("NDK_HOME", sdk / "ndk" / NDK)).resolve()),
               ANDROID_USER_HOME=str(ROOT / "build/android-user"), GRADLE_USER_HOME=str(ROOT / "build/android-gradle"),
               CARGO_TARGET_DIR=str(ROOT / "build/cargo"))
    # Tauri sets the targets' own flags itself, overriding any given: these
    # go in RUSTFLAGS instead, with the libraries Tauri links. 16 KiB pages
    # (Android 15+), no symbols (a third of the size), and no paths of this
    # machine in the library.
    env["RUSTFLAGS"] = " ".join(["-Clink-arg=-landroid", "-Clink-arg=-llog", "-Clink-arg=-lOpenSLES",
                                 "-Clink-arg=-Wl,-z,max-page-size=16384", "-Cstrip=symbols",
                                 f"--remap-path-prefix={ROOT}=/aw", f"--remap-path-prefix={Path.home()}=/build"])

    def run(command, cwd=ROOT, **kw):
        return subprocess.run([str(p) for p in command], cwd=cwd, env=env, check=True, **kw)

    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    variant = "debug" if args.debug else "release"
    tauri = ["node", APP / "node_modules/@tauri-apps/cli/tauri.js", "android", "build", "--apk", "--ci", "--target", *targets]
    if args.debug:
        tauri.append("--debug")

    # The notices: Rust and JavaScript, then the Android libraries Gradle
    # packs — those only once Gradle has resolved them, so a second build
    # takes the complete text in (Rust is done by then and not built again).
    run(["node", ROOT / "tools/licenses/notices.mjs", "--target", "aarch64-linux-android"])
    run(["node", APP / "node_modules/typescript/bin/tsc", "--noEmit", "-p", "tsconfig.json"], APP)
    run(["node", APP / "node_modules/vite/bin/vite.js", "build"], APP)
    assets = BUILD / "assets"
    assets.mkdir(parents=True, exist_ok=True)
    # Tauri links each target's library in here; one left from an earlier
    # build for another target would go into the APK as it is.
    shutil.rmtree(ANDROID / "app/src/main/jniLibs", ignore_errors=True)
    run(tauri, APP)
    run([ANDROID / "gradlew", "--project-dir", ANDROID, ":app:awRuntimeCoordinates", "--console=plain"], ANDROID)
    coordinates = (BUILD / "maven-runtime.txt").read_text().splitlines()
    for coordinate in coordinates:
        check_license(coordinate, Path(env["GRADLE_USER_HOME"]))
    apache = BUILD / "APACHE-2.0.txt"
    if not apache.exists():
        with urlopen("https://www.apache.org/licenses/LICENSE-2.0.txt", timeout=30) as response:
            apache.write_bytes(response.read())
    notices = ROOT / "build/public/licenses.txt"
    with notices.open("a") as out:
        out.write("\nAndroid libraries (Apache-2.0)\n==============================\n" + "\n".join(coordinates) + "\n\n" + apache.read_text())
    # The server's own screen shows them from the APK's assets.
    shutil.copy2(ROOT.parent / "LICENSE", assets / "APP-LICENSE.txt")
    shutil.copy2(notices, assets / "THIRD-PARTY-NOTICES.txt")
    run(["node", APP / "node_modules/vite/bin/vite.js", "build"], APP)
    run(tauri, APP)
    built = list((ANDROID / "app/build/outputs/apk/universal" / variant).glob("*.apk"))
    if len(built) != 1:
        parser.error(f"expected one universal {variant} APK: {built}")

    out = BUILD if args.debug else ROOT.parent / "programs/android"
    if not args.debug and out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True, exist_ok=True)
    apk = out / (f"AnotherWorldbuilder_{version}_android{'-debug' if args.debug else ''}.apk")
    signer = sdk / "build-tools/36.0.0/apksigner"
    run([signer, "sign", "--v4-signing-enabled", "false", "--ks", key, "--ks-pass", "file:" + str(password), "--out", apk, built[0]])
    run([signer, "verify", apk])
    if args.debug:
        # The server's device test (`tools/test/android-server.py`).
        # Only the test's own classes: the Rust library is in the app already.
        skip = [arg for abi in ("Arm64", "Arm", "X86", "X86_64", "Universal") for arg in ("-x", f":app:rustBuild{abi}Debug")]
        run([ANDROID / "gradlew", "--project-dir", ANDROID, ":app:assembleUniversalDebugAndroidTest", *skip, "--console=plain"], ANDROID)
        tests = list((ANDROID / "app/build/outputs/apk/androidTest/universal/debug").glob("*.apk"))
        run([signer, "sign", "--v4-signing-enabled", "false", "--ks", key, "--ks-pass", "file:" + str(password),
             "--out", out / "AnotherWorldbuilder-test.apk", tests[0]])
        print(apk)
        return
    checksum = hashlib.sha256(apk.read_bytes()).hexdigest()
    (out / "SHA256SUMS.txt").write_text(f"{checksum}  {apk.name}\n")
    (out / "build-info.json").write_text(json.dumps({
        "version": version, "minAndroid": "8.0", "targetSdk": 36, "targets": [TARGETS[t] for t in targets],
        "commit": subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True).stdout.strip(),
        "apk": apk.name, "bytes": apk.stat().st_size, "sha256": checksum}, indent=2) + "\n")
    shutil.copy2(notices, out / "THIRD-PARTY-NOTICES.txt")
    shutil.copy2(APP / "ANDROID.md", out / "README.md")
    print(apk)


def check_license(coordinate: str, gradle: Path):
    """An Android library goes in only under Apache 2.0 — anything else is looked at first."""
    group, name, version = coordinate.split(":")
    cache = gradle / "caches/modules-2/files-2.1"
    poms = list((cache / group / name / version).glob("*/*.pom"))
    if not poms:
        raise RuntimeError("no license metadata: " + coordinate)
    pom = ET.fromstring(poms[0].read_bytes())
    licenses = pom.findall("{*}licenses/{*}license")
    # Many inherit theirs from a parent.
    for _ in range(8):
        parent = pom.find("{*}parent")
        if licenses or parent is None:
            break
        g, a, v = [parent.findtext("{*}" + k) for k in ("groupId", "artifactId", "version")]
        parents = list((cache / g / a / v).glob("*/*.pom"))
        if not parents:
            raise RuntimeError("no license metadata of the parent: " + a)
        pom = ET.fromstring(parents[0].read_bytes())
        licenses = pom.findall("{*}licenses/{*}license")
    named = [" ".join(item.itertext()).lower() for item in licenses]
    if not any("apache" in n and ("2.0" in n or "2,0" in n) for n in named):
        raise RuntimeError("look at this license before shipping: " + coordinate + " " + str(named))


if __name__ == "__main__":
    main()
