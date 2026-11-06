package com.dicefoxcreations.anotherworldbuilder.server;

import android.content.Context;
import android.content.SharedPreferences;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;
import java.io.File;
import java.util.Locale;

final class ServerConfig {
    static final String DEFAULT_NAME = "My table";
    private ServerConfig() {}
    static SharedPreferences prefs(Context context) {
        return context.getSharedPreferences("server", Context.MODE_PRIVATE);
    }
    static File data(Context context) { return new File(context.getFilesDir(), "server"); }

    /** The ID of the app's profile on this phone — the admin, mostly; "" before the app is set up. */
    static String ownId(Context context) {
        try {
            byte[] raw = java.nio.file.Files.readAllBytes(new File(context.getDataDir(), "profile/identity.json").toPath());
            return new JSONObject(new String(raw, java.nio.charset.StandardCharsets.UTF_8)).optString("id", "");
        } catch (Exception missing) {
            return "";
        }
    }

    static String normalizeAdmin(String text) {
        String admin = text.trim().toLowerCase(Locale.ROOT);
        return admin.startsWith("id:") ? admin : "id:" + admin;
    }

    static void validate(String admin, String name, int port) {
        if (!normalizeAdmin(admin).matches("id:[0-9a-f]{16}"))
            throw new IllegalArgumentException("Enter your admin ID: id: and the 16 characters from your profile in the app.");
        if (name.trim().isEmpty() || name.codePointCount(0, name.length()) > 120)
            throw new IllegalArgumentException("Enter a server name of 1 to 120 characters.");
        if (port < 1024 || port > 65535)
            throw new IllegalArgumentException("Enter a port between 1024 and 65535.");
    }

    static String json(Context context) throws JSONException {
        SharedPreferences p = prefs(context);
        String admin = normalizeAdmin(p.getString("admin", ""));
        String name = p.getString("name", DEFAULT_NAME).trim();
        int port = p.getInt("port", 7447);
        validate(admin, name, port);
        JSONArray addresses = new JSONArray();
        for (String address : p.getString("addresses", "").split(",")) {
            if (!address.trim().isEmpty()) addresses.put(address.trim());
        }
        return new JSONObject().put("admin", admin).put("data", data(context).getAbsolutePath())
            .put("port", port).put("name", name).put("addresses", addresses)
            .put("lan", true).put("internet", true).toString();
    }
}
