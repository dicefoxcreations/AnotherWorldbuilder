# Local and mounted cloud folders

The former "Shared folders" transport of its own was removed. A cloud folder provided by the operating system is chosen like any other local storage location; Worldbuilder works directly on its world folders.

1. Open **Settings → Storage → Add world locations**.
2. Choose one or more folders that are to hold `world-<id>/` with `world.json` (format 3), `control/` and `articles/`. Only finished versions go through folders – a change appears for the others as soon as the author leaves edit mode.
3. Under **New world → Local** choose the target folder. Existing current worlds appear in the shared library.
4. **Remove location** only removes the assignment. It deletes neither worlds nor keys.

If a world lies in an additional storage location and *Network → Over the internet* is on, the devices leave their signed notes there under `peers/` and connect directly — then changes arrive in seconds, not only with the file sync (see [Directly over the internet](internet-direct.md)).

**rclone and Google Drive (measured 2026-10-20):** Google takes small files only one at a time, through rclone about 0.4 files per second. A large world (20,000 files, 430 MB) would need about 14 hours for the first upload, and all other changes in the same drive wait behind it for that long. With `--vfs-cache-mode=writes` every read goes over the network: the app then stalls on every opening. With `--vfs-cache-mode=full` it reads from disk after the first time (a small world: start 13 s, first read 53 s, after that 60 ms). For large worlds a mounted drive is therefore only usable with a full cache and a one-time upload overnight; rclone's shared client ID is switched off in 2026, an own one is needed.

**Directory cache (measured 2026-10-20, two mounts of the same Drive folder):** with `--dir-cache-time=30s` rclone asks Google again once it expires; with rclone's shared client ID Google then often answers `rateLimitExceeded`, and a listing takes 19 s. The direct sync waits that long too; changes arrived after 52–76 s instead of 2 s. With `--dir-cache-time=1000h --poll-interval=15s` (rclone's recommendation for Drive: the cache stays, Google reports changes) 5 of 5 changes arrived in 2.1–3.0 s, also after a pause of 30–60 s. Recommended mount:

```
rclone mount gdrive:<folder> <target> --vfs-cache-mode=full --vfs-write-back=2s --dir-cache-time=1000h --poll-interval=15s
```

Worlds in an additional storage location are shown on the start page with a cloud symbol and the folder's name: they lie in a shared folder. The cloud app takes care of transport, offline availability and file sync; which provider does not matter. Participants need their own Worldbuilder profile and the matching world permissions; access to a folder alone grants no decryption rights. The GM admits players through their player card. Through the folder the devices find each other for the [direct connection](internet-direct.md): then changes, cursors and presence travel in seconds.

No conversion of earlier development formats. Old "Shared folders" exchange folders are not world folders in the current format.

Technical details and backup notes: [Storage locations](../architecture/storage.md).
