# AnotherWorldbuilder on Android

One APK, two icons: **AnotherWorldbuilder**, the app with the same interface
and Rust core as on the desktop, and **AW Server**, the dedicated server — the
host of `aw-server` in Docker, with its own native screen. Android 8.0 or later
with an updated Android System WebView; the APK carries ARM64, ARMv7 and
x86-64.

## The app

Enter your name, add a server by its address or connection code, then ask to
join a world; its GM lets you in. Profile and local worlds lie in the app's
private storage — no folder to choose, no browser extension, no desktop
updater: a newer APK is installed over this one and keeps the data. A world on
a server stays on that server. Back closes what is open, then leaves the world.

What outlasts the app goes to `Documents/AnotherWorldbuilder/` (Android 11 or
later, no permission asked): a world's backups, one `.tar` file each, and the
profile backup (*profile → Back up your profile*). After installing anew:
*Restore your profile from a backup* on the first screen, then *⋮ → Restore a
backup → From a file …* for each world — Android shows a new install none of
the old one's files unless they are picked. A phone's `.tar` restores on a
computer too. Uninstalling removes everything else on the phone, AW Server's
worlds too — export them first.

## AW Server

Its own process and task, its own identity and folder (`files/server/`), as
on a NAS. Set it up under **Options**: your admin ID (from the app's profile),
a name, the port (7447). **Start server** keeps it running as a foreground
service with a notification holding **Stop** — also with the screen off and
the app closed. The connection code is under **Options → Connection**; the app
on a computer finds it on the local network too. **Worlds** lists what it
keeps; members and access are managed from the app, as for any server.

- Wi-Fi only is on at first: without a network it waits and comes back up with
  the same identity (an existing client may need the new address).
- Resuming after a restart of the phone is off until switched on, and only
  resumes a server left running; Stop switches it off.
- **Options → Export server backup** saves a ZIP (with the server's private
  identity — keep it private) while the server is stopped; its `server/` folder
  works with `aw-server` on Linux, Windows or Docker too.
- Reaching it from the internet needs what any server needs: open UDP/TCP
  ports, a usable IPv6 path or a VPN. Mobile carrier NAT and guest Wi-Fi block it.

The app's own server and AW Server run side by side: AW Server keeps its port;
the app keeps off it — or any other port taken — and says so under *Options →
Network*.

**Why `specialUse`:** the service waits for clients for as long as the user
wants, also with nobody connected. `dataSync` is for finite transfers (six
hours a day from Android 15), `shortService` ends after minutes,
`connectedDevice` needs a paired device, `systemExempted` a system role. A
Play Store submission would have to justify `specialUse`; this APK is
installed directly.

## Build

From `source/`, after `npm ci` in `apps/desktop`, with Java 17, the Android SDK
36 / NDK 28c and the Rust targets `aarch64-linux-android`,
`armv7-linux-androideabi`, `x86_64-linux-android`:

```sh
JAVA_HOME=/usr/lib/jvm/java-17-openjdk python3 tools/build/android.py
```

The Android project is `apps/desktop/src-tauri/gen/android` (committed); AW
Server's screen is `app/src/main/java/…/server/`, its Rust side
`src-tauri/src/dedicated.rs` and `android.rs`. The APK goes to
`../programs/android/`, signed with `~/.tauri/android/anotherworldbuilder.jks`
(see the script — without that key no update installs over an installed app).
`--targets x86_64 --debug` builds for an emulator.
