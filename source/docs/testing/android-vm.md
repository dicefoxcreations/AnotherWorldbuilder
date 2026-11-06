# Joining a world hosted by the Android VM

AW Server in the signed APK has been tested from the Linux host over real
QUIC/UDP, followed by opening the same world in the normal desktop app with
separate GM and player profiles. This extends the on-device service smoke
test: it exercises clients outside Android and the emulator's NAT.

## Current demo

- AVD: `aw-server-join`, Android 15, console/ADB serial `emulator-5584`.
- APK: `programs/android/AnotherWorldbuilder_<version>_android.apk` in the repository root.
- Server name: `Android-VM-Server`; server port: 7447, UDP and TCP.
- Server's explicit address in this VM: `10.0.2.15:7447`.
- Test data and generated identities: `source/build/android-join-test/`.
- Desktop profiles: its `gm/` and `player/` subfolders, independent of normal
  app profiles. The world is stored on Android; their local world folders
  contain no copy of it.

The live demo uses these loopback-only emulator redirections:

| Host port | Guest port | Use |
| --- | --- | --- |
| 17447 TCP/UDP | 7447 | GM test client / TCP code lookup |
| 17448 UDP | 7447 | Player test client, using a full connection code |
| 17449 TCP/UDP | 7447 | Additional manual desktop client |

For a manual join, use **Add server → `127.0.0.1:17449`**, then request access
to **Android-VM-Testwelt**. Approve it in the generated GM desktop client.
The VM's displayed guest address is not a directly reachable host address;
the forwarded loopback port is the entry point from the Linux desktop.

## Two emulator networking details

This AVD has both `eth0` (10.0.2.15) and `wlan0` (10.0.2.16). The server
initially chose Wi-Fi, while emulator console redirection delivered packets
to Ethernet. Setting the server's explicit address to 10.0.2.15:7447 aligns
the listener with that forwarding. No certificate checks are bypassed.

Two simultaneous QUIC client endpoints did not connect reliably through a
single UDP redirection in this test. Giving each endpoint its own local
forwarded UDP port made both work concurrently against the same Android
server port. The helper restricts its address candidates to the explicit
forwardings, avoiding WAN candidates advertised by the guest.

## Reproduce

Run the following from `source/`. The SDK, AVD and signed APK must already
exist; building is in `apps/desktop/ANDROID.md`.

```sh
export ANDROID_HOME="$PWD/build/android-sdk"
export ANDROID_AVD_HOME="$PWD/build/android-avd"
export ANDROID_USER_HOME="$PWD/build/android-user"

"$ANDROID_HOME/emulator/emulator" -avd aw-server-join -port 5584 \
  -no-snapshot -gpu swiftshader -memory 2048 -cores 2 -scale 0.55
```

In another terminal, install the APK if needed and prepare fresh test data:

```sh
adb -s emulator-5584 install -r ../programs/android/AnotherWorldbuilder_*_android.apk
cargo run -p aw-desktop --no-default-features --features server \
  --example android_join_probe -- prepare build/android-join-test
```

Open **AW Server**, then **Options** from its navigation menu, and use the generated GM ID as the
Android admin. Under **Advanced options**, set **Advertised addresses** to
`10.0.2.15:7447`, then save. Return to **Server**, allow the status notification
and start hosting. **Worlds** lists the hosted test world; **About** contains
the version and licenses. Saving options while running restarts the host. For ADB
keyboard automation, this VM's handwriting interception can be disabled with
`adb -s emulator-5584 shell settings put secure stylus_handwriting_enabled 0`.

Add the port redirections once per running emulator:

```sh
adb -s emulator-5584 emu redir add tcp:17447:7447
adb -s emulator-5584 emu redir add udp:17447:7447
adb -s emulator-5584 emu redir add udp:17448:7447
adb -s emulator-5584 emu redir add tcp:17449:7447
adb -s emulator-5584 emu redir add udp:17449:7447

cargo run -p aw-desktop --no-default-features --features server \
  --example android_join_probe -- join build/android-join-test \
  127.0.0.1:17447 127.0.0.1:17448
```

The helper uploads a real SQLite world, requests a player's admission,
approves it as GM and checks:

- GM and player can open the world on Android.
- The player receives public text and does not receive the GM-only section.
- A permitted player change is stored on Android and read back by the GM.
- A player cannot change a GM-only setting; a nonmember cannot read the world.

It writes `connection.json` and the two desktop profiles' `network.json` files.
Repeat runs preserve their generated identities and check existing membership.
Start each profile in its own desktop process, using the matching profile
root (do not launch another process while that profile is already open):

```sh
AW_PROFILE_ROOT="$PWD/build/android-join-test/gm" build/cargo/release/aw-desktop
AW_PROFILE_ROOT="$PWD/build/android-join-test/player" build/cargo/release/aw-desktop
```

The native UI check also confirmed both clients displayed the public article
and only the GM client displayed its secret section. Test outputs are in
`build/android-join-result.log` and `build/android-join-test/native-ui-result.json`.

## The app on the same device

The app and AW Server are one APK, in processes of their own with identities
of their own. The generated player profile
`Android Client Test` joins the existing test world after GM approval. Enable
the client's local-network hosting to exercise its fallback from occupied
port 7447 to a free UDP/TCP pair. `tools/test/android-client.mjs` checks the
visible player article, server-side permissions and both lookup ports.

The emulator's graphics backend crashed with the layered SVG loader during
initial testing. The Android activity uses CPU WebView layers only on the
standard emulator hardware (`ranchu`/`goldfish`); physical devices keep
hardware acceleration. The demo is started with Vulkan disabled.
