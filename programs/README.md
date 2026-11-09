# Programs

The program files themselves are under **[Releases](../../../releases/latest)**: every version for download.
This folder holds only the accompanying files of the current version: checksums (`SHA256SUMS.txt`),
licenses (`THIRD-PARTY-NOTICES.txt`) and, for the server, `compose.yaml` and `Dockerfile`.

Every version comes for Linux, Windows and Android. Each carries **AW Server**, the server
for a group, beside the app: a second icon on Android, a second menu entry on Linux and
Windows (or *Options → Network → Open AW Server*). The Docker image is the same server
without a window, for a NAS.

| File in the release | For |
| --- | --- |
| `AnotherWorldbuilder_<version>_amd64.AppImage` | Linux, no installation; updates itself |
| `AnotherWorldbuilder_<version>_amd64.deb` | Linux (Debian, Ubuntu), installed |
| `AnotherWorldbuilder_<version>_x64-setup.exe` | Windows, installed for the current user; updates itself |
| `AnotherWorldbuilder_<version>_android.apk` | Android 8 or later — [about it](../source/apps/desktop/ANDROID.md) |
| `DnD-5e-template.zip` | The template from [`../templates/`](../templates/), to unzip and import |
| `aw-server` | The server without a window (Linux, static) |
| `aw-server-<version>-docker.tar` | The server as a Docker image to load; simpler: `dicefoxcreations/anotherworldbuilder-server` from Docker Hub |
