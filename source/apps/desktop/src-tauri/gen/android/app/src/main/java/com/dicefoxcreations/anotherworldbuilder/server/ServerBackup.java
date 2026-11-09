package com.dicefoxcreations.anotherworldbuilder.server;

import android.content.Context;
import android.net.Uri;
import org.json.JSONObject;
import java.io.*;
import java.nio.file.*;
import java.util.Iterator;
import java.util.zip.*;

final class ServerBackup {
    private ServerBackup() {}

    /** Caller reserves ServerService.exporting before handing off to a worker. */
    static void write(Context context, Uri destination) throws Exception {
        JSONObject state = new JSONObject(NativeServer.status());
        if (!state.optString("state").matches("stopped|failed") || ServerService.active)
            throw new IOException("Stop the server before exporting a backup.");
        Path root = ServerConfig.data(context).toPath();
        try (OutputStream output = context.getContentResolver().openOutputStream(destination, "wt")) {
            if (output == null) throw new IOException("Could not open the backup destination.");
            try (ZipOutputStream zip = new ZipOutputStream(output)) {
                if (Files.exists(root)) try (java.util.stream.Stream<Path> paths = Files.walk(root)) {
                    Iterator<Path> files = paths.iterator();
                    while (files.hasNext()) {
                        Path file = files.next();
                        Path relative = root.relativize(file);
                        if (!Files.isRegularFile(file, LinkOption.NOFOLLOW_LINKS) || file.getFileName().toString().equals(".lock") || relative.startsWith("locks")) continue;
                        zip.putNextEntry(new ZipEntry("server/" + relative.toString().replace(File.separatorChar, '/')));
                        Files.copy(file, zip);
                        zip.closeEntry();
                    }
                }
                zip.putNextEntry(new ZipEntry("server-settings.json"));
                zip.write(ServerConfig.json(context).getBytes(java.nio.charset.StandardCharsets.UTF_8));
                zip.closeEntry();
            }
        }
    }
}
