package com.dicefoxcreations.anotherworldbuilder.server;

import com.dicefoxcreations.anotherworldbuilder.R;

import android.app.*;
import android.content.*;
import android.os.Bundle;
import android.os.Build;
import android.net.*;
import android.view.*;
import android.widget.*;
import org.json.JSONObject;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.util.zip.ZipFile;

/** Dependency-free device smoke test; run only on a dedicated clean emulator. */
public final class ServerInstrumentation extends Instrumentation {
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); start(); }
    private static void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }
    private JSONObject waitFor(String state) throws Exception {
        long deadline = System.currentTimeMillis() + 60000;
        JSONObject last;
        do {
            last = new JSONObject(NativeServer.status());
            if (state.equals(last.optString("state"))) return last;
            require(!"failed".equals(last.optString("state")), last.toString());
            Thread.sleep(100);
        } while (System.currentTimeMillis() < deadline);
        throw new AssertionError("Expected " + state + ": " + last);
    }
    private void waitServiceStopped() throws Exception {
        waitFor("stopped");
        long deadline = System.currentTimeMillis() + 15000;
        while (ServerService.active && System.currentTimeMillis() < deadline) Thread.sleep(100);
        require(!ServerService.active, "foreground service did not stop");
    }
    private void shell(String command) throws Exception {
        try (java.io.InputStream output = new android.os.ParcelFileDescriptor.AutoCloseInputStream(getUiAutomation().executeShellCommand(command))) {
            byte[] bytes = new byte[4096];
            while (output.read(bytes) != -1) { /* wait for shell completion */ }
        }
    }
    private void click(Activity activity, int id) {
        runOnMainSync(() -> {
            View view = activity.findViewById(id);
            require(view != null && view.isEnabled(), "Unavailable control: " + id);
            view.performClick();
        });
    }
    private void navigate(Activity activity, int id) {
        click(activity, R.id.server_navigation_menu); click(activity, id);
    }
    private void awaitEnabled(Activity activity, int id) throws Exception {
        long deadline = System.currentTimeMillis() + 5000;
        boolean[] enabled = {false};
        do {
            runOnMainSync(() -> { View v = activity.findViewById(id); enabled[0] = v != null && v.isEnabled(); });
            if (enabled[0]) return;
            Thread.sleep(100);
        } while (System.currentTimeMillis() < deadline);
        throw new AssertionError("Control did not become enabled: " + id);
    }
    private String discovery(int port) throws Exception {
        try (Socket socket = new Socket()) {
            socket.connect(new InetSocketAddress("127.0.0.1", port), 1000);
            socket.setSoTimeout(3000);
            socket.getOutputStream().write("GET / HTTP/1.0\r\n\r\n".getBytes(StandardCharsets.US_ASCII));
            ByteArrayOutputStream bytes = new ByteArrayOutputStream();
            byte[] piece = new byte[4096]; int read;
            while ((read = socket.getInputStream().read(piece)) != -1) bytes.write(piece, 0, read);
            return new String(bytes.toByteArray(), StandardCharsets.UTF_8);
        }
    }
    private void awaitPort(int port, String endpoint) throws Exception {
        long deadline = System.currentTimeMillis() + 60000;
        do {
            try { if (discovery(port).contains(endpoint)) return; } catch (java.io.IOException ignored) { }
            Thread.sleep(100);
        } while (System.currentTimeMillis() < deadline);
        throw new AssertionError("Updated server not listening on " + port);
    }
    @Override public void onStart() {
        Bundle report = new Bundle();
        Activity activity = null;
        try {
            Context context = getTargetContext();
            require(!ServerConfig.prefs(context).contains("admin") || ServerConfig.prefs(context).getBoolean("instrumentation", false), "Use a fresh test emulator, not real server data.");
            require(NativeServer.start("{}").length() > 0, "invalid native configuration accepted");
            ServerConfig.prefs(context).edit().putBoolean("instrumentation", true).putString("admin", "id:0000000000000000")
                .putString("name", "Android service test").putInt("port", 17447).putBoolean("lan", true)
                .putBoolean("wifi_only", false).putBoolean("wanted", true).putBoolean("boot", false).putString("addresses", "").commit();
            activity = startActivitySync(new Intent(context, ServerActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            Activity fresh = activity;
            runOnMainSync(() -> require(fresh.findViewById(R.id.server_start).getVisibility() == View.VISIBLE, "A remembered hosting preference hides Start while the service is stopped"));
            runOnMainSync(() -> context.startForegroundService(new Intent(context, ServerService.class)));
            String firstCode = waitFor("running").getString("code");
            require(!firstCode.isEmpty(), "no connection code");
            require(ServerService.active, "no foreground service");
            ConnectivityManager connectivity = context.getSystemService(ConnectivityManager.class);
            NetworkCapabilities caps = connectivity.getNetworkCapabilities(connectivity.getActiveNetwork());
            if (Build.VERSION.SDK_INT >= 29 && caps != null && caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)) {
                ServerConfig.prefs(context).edit().putBoolean("wifi_only", true).commit();
                shell("svc wifi disable");
                waitFor("stopped");
                Thread.sleep(1500);
                require(ServerService.active && ServerService.display.startsWith("Waiting"), "not waiting after Wi-Fi loss");
                shell("svc wifi enable");
                String reconnected = waitFor("running").getString("code");
                require(firstCode.split("@")[0].equals(reconnected.split("@")[0]), "network change replaced identity");
                firstCode = reconnected;
            }
            Activity ui = activity;
            runOnMainSync(() -> {
                require(ui.findViewById(R.id.server_admin_id) == null && ui.findViewById(R.id.server_third_party_licenses) == null, "Home contains settings or legal content");
                require(ui.findViewById(R.id.server_status) != null && ui.findViewById(R.id.server_restart) != null, "Missing home controls");
            });
            navigate(ui, R.id.server_nav_options);
            click(ui, R.id.server_advanced_options);
            File codeBeforeReset = new File(ServerConfig.data(context), "host-code.txt");
            long startBeforeReset = codeBeforeReset.lastModified();
            boolean savedWifi = ServerConfig.prefs(context).getBoolean("wifi_only", false);
            runOnMainSync(() -> {
                require(ui.findViewById(R.id.server_save_options) instanceof ImageButton && ui.findViewById(R.id.server_reset_options) instanceof ImageButton, "Options toolbar actions are not icons");
                ((EditText)ui.findViewById(R.id.server_admin_id)).setText("id:1111111111111111");
                ((EditText)ui.findViewById(R.id.server_name)).setText("Unsaved draft");
                ((EditText)ui.findViewById(R.id.server_port)).setText("19000");
                ((EditText)ui.findViewById(R.id.server_addresses)).setText("127.0.0.1:19000");
                ((CheckBox)ui.findViewById(R.id.server_wifi_only)).setChecked(!savedWifi);
                ((CheckBox)ui.findViewById(R.id.server_resume_on_boot)).setChecked(true);
            });
            click(ui, R.id.server_reset_options);
            runOnMainSync(() -> {
                require(((EditText)ui.findViewById(R.id.server_admin_id)).getText().toString().equals("id:0000000000000000"), "Reset did not restore admin");
                require(((EditText)ui.findViewById(R.id.server_name)).getText().toString().equals("Android service test"), "Reset did not restore server name");
                require(((EditText)ui.findViewById(R.id.server_port)).getText().toString().equals("17447"), "Reset did not restore port");
                require(((EditText)ui.findViewById(R.id.server_addresses)).getText().length() == 0, "Reset did not restore addresses");
                require(((CheckBox)ui.findViewById(R.id.server_wifi_only)).isChecked() == savedWifi, "Reset did not restore Wi-Fi option");
                require(!((CheckBox)ui.findViewById(R.id.server_resume_on_boot)).isChecked(), "Reset did not restore boot option");
                require(!ui.findViewById(R.id.server_save_options).isEnabled() && !ui.findViewById(R.id.server_reset_options).isEnabled(), "Clean options still have active actions");
            });
            require(discovery(17447).contains(firstCode), "Reset interrupted the running host");
            require(codeBeforeReset.lastModified() == startBeforeReset, "Reset restarted the server");
            runOnMainSync(() -> {
                ((EditText)ui.findViewById(R.id.server_admin_id)).setText("id:0000000000000001");
                ((EditText)ui.findViewById(R.id.server_name)).setText("Renamed Android server");
                ((EditText)ui.findViewById(R.id.server_port)).setText("17448");
            });
            awaitEnabled(ui, R.id.server_save_options); click(ui, R.id.server_save_options);
            awaitPort(17448, firstCode.split("@")[0]);
            require(ServerConfig.prefs(context).getString("admin", "").equals("id:0000000000000001"), "Admin ID not saved");
            try (Socket old = new Socket()) {
                old.connect(new InetSocketAddress("127.0.0.1", 17447), 500);
                throw new AssertionError("Previous port still listening after settings restart");
            } catch (java.io.IOException expected) { }
            firstCode = waitFor("running").getString("code");
            require(ServerConfig.prefs(context).getString("name", "").equals("Renamed Android server"), "Name not saved");
            runOnMainSync(() -> ((EditText)ui.findViewById(R.id.server_name)).setText("Discard after save"));
            click(ui, R.id.server_reset_options);
            runOnMainSync(() -> require(((EditText)ui.findViewById(R.id.server_name)).getText().toString().equals("Renamed Android server"), "Reset restored defaults instead of the latest saved settings"));
            navigate(ui, R.id.server_nav_worlds);
            require(new JSONObject(NativeServer.worlds(ServerConfig.data(context).getAbsolutePath())).getJSONArray("worlds") != null, "World list missing");
            runOnMainSync(() -> require(ui.findViewById(R.id.server_hosted_worlds) != null && ui.findViewById(R.id.server_admin_id) == null, "Worlds page not isolated"));
            navigate(ui, R.id.server_nav_about);
            runOnMainSync(() -> require(ui.findViewById(R.id.server_third_party_licenses) != null && ui.findViewById(R.id.server_admin_id) == null, "About page missing licenses"));
            navigate(ui, R.id.server_nav_server);
            Thread.sleep(1000);
            File hostCode = new File(ServerConfig.data(context), "host-code.txt");
            long previousStart = hostCode.lastModified();
            awaitEnabled(ui, R.id.server_restart); click(ui, R.id.server_restart);
            long restartDeadline = System.currentTimeMillis() + 60000;
            while (hostCode.lastModified() <= previousStart && System.currentTimeMillis() < restartDeadline) Thread.sleep(100);
            require(hostCode.lastModified() > previousStart, "Home Restart did not start a new host");
            require(firstCode.split("@")[0].equals(waitFor("running").getString("code").split("@")[0]), "Home Restart changed endpoint identity");
            Thread.sleep(1000);
            android.service.notification.StatusBarNotification[] notifications = context.getSystemService(NotificationManager.class).getActiveNotifications();
            require(notifications.length == 1 && notifications[0].getNotification().actions.length == 1, "missing notification Stop action");
            Activity runningActivity = activity;
            runOnMainSync(runningActivity::finish);
            activity = null;
            Thread.sleep(1500);
            require("running".equals(new JSONObject(NativeServer.status()).getString("state")), "closing the activity stopped hosting");
            shell("input keyevent 223");
            Thread.sleep(2000);
            require("running".equals(new JSONObject(NativeServer.status()).getString("state")), "screen-off stopped hosting");
            require(discovery(17448).contains(firstCode), "TCP discovery returned another code");
            notifications[0].getNotification().actions[0].actionIntent.send();
            waitServiceStopped();
            shell("input keyevent 224");
            shell("wm dismiss-keyguard");
            require(!ServerConfig.prefs(context).getBoolean("wanted", true), "Stop did not disable automatic restart");
            File backup = new File(context.getCacheDir(), "server-backup-test.zip");
            ServerService.exporting = true;
            try { ServerBackup.write(context, Uri.fromFile(backup)); }
            finally { ServerService.exporting = false; }
            try (ZipFile zip = new ZipFile(backup)) {
                require(zip.getEntry("server/profile/identity.json") != null, "backup omitted identity");
                require(zip.getEntry("server-settings.json") != null, "backup omitted settings");
                require(zip.getEntry("server/profile/.lock") == null, "backup contains live lock");
            }
            // Reopen the activity to satisfy Android's user-visible start rules.
            activity = startActivitySync(new Intent(context, ServerActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            ServerConfig.prefs(context).edit().putBoolean("wanted", true).commit();
            runOnMainSync(() -> context.startForegroundService(new Intent(context, ServerService.class)));
            require(firstCode.split("@")[0].equals(waitFor("running").getString("code").split("@")[0]), "identity changed on restart");
            runOnMainSync(() -> context.startService(new Intent(context, ServerService.class).setAction(ServerService.STOP)));
            waitServiceStopped();
            report.putString("stream", "PASS: English home/navigation, toolbar Save, Reset restores all latest saved settings without restarting, Options save and restart with changed port/name/admin, Worlds, About licenses, Home Restart, JNI validation, foreground service, QUIC, TCP discovery, notification Stop, Activity close, screen off, backup, stable identity; Wi-Fi reconnect where supported.\n");
            finish(Activity.RESULT_OK, report);
        } catch (Throwable error) {
            NativeServer.stop();
            report.putString("stream", android.util.Log.getStackTraceString(error));
            finish(Activity.RESULT_CANCELED, report);
        } finally {
            if (activity != null) { Activity last = activity; runOnMainSync(last::finish); }
        }
    }
}
