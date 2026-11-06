#!/usr/bin/env python3
"""Installs the debug app on a dedicated emulator and runs the server's device test (AW Server)."""
import argparse
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = "com.dicefoxcreations.anotherworldbuilder"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("serial", help="a dedicated emulator, e.g. emulator-5580")
    args = parser.parse_args()
    if not args.serial.startswith("emulator-"):
        parser.error("Use a dedicated emulator, not a phone with a real server on it.")
    adb = ["adb", "-s", args.serial]

    def run(*arguments, **kwargs):
        return subprocess.run(adb + list(arguments), check=True, **kwargs)

    api = int(run("shell", "getprop", "ro.build.version.sdk", capture_output=True, text=True).stdout.strip())
    built = ROOT / "build/android"
    app = sorted(built.glob("AnotherWorldbuilder_*_android-debug.apk"))[-1]
    run("install", "-r", str(app))
    run("install", "-r", str(built / "AnotherWorldbuilder-test.apk"))
    if api >= 33:
        run("shell", "pm", "grant", PACKAGE, "android.permission.POST_NOTIFICATIONS")
    result = run("shell", "am", "instrument", "-w", f"{PACKAGE}.test/{PACKAGE}.server.ServerInstrumentation",
                 capture_output=True, text=True, timeout=180)
    report = built / f"device-api{api}-test.log"
    report.write_text(result.stdout + result.stderr)
    print(result.stdout, end="")
    if "PASS:" not in result.stdout:
        raise SystemExit(f"Device test failed: {report}")
    run("shell", "am", "start", "-n", f"{PACKAGE}/.server.ServerActivity", capture_output=True)
    with (built / f"device-api{api}.png").open("wb") as screenshot:
        run("exec-out", "screencap", "-p", stdout=screenshot)
    print(report)


if __name__ == "__main__":
    main()
