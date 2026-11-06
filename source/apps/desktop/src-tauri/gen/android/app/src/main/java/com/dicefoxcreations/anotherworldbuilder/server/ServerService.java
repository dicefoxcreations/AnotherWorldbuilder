package com.dicefoxcreations.anotherworldbuilder.server;

import com.dicefoxcreations.anotherworldbuilder.R;

import android.app.*;
import android.content.*;
import android.content.pm.ServiceInfo;
import android.net.*;
import android.net.wifi.WifiManager;
import android.os.*;
import org.json.JSONObject;
import java.util.Objects;

public final class ServerService extends Service {
    static final String STOP = "com.dicefoxcreations.anotherworldbuilder.server.STOP";
    static final String RESTART = "com.dicefoxcreations.anotherworldbuilder.server.RESTART";
    private static final String CHANNEL = "hosting";
    private static final int NOTIFICATION = 7447;
    static volatile String display = "Stopped";
    static volatile String phase = "stopped";
    static volatile boolean active = false;
    static volatile boolean exporting = false;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private ConnectivityManager connectivity;
    private ConnectivityManager.NetworkCallback networkCallback;
    private PowerManager.WakeLock wakeLock;
    private WifiManager.WifiLock wifiLock;
    private WifiManager.MulticastLock multicastLock;
    private boolean wanted, restart, finishing, startPending;
    private String networkKey = "", lastNotification = "";
    private long wakeRenewed;

    @Override public void onCreate() {
        super.onCreate();
        active = true;
        NotificationChannel channel = new NotificationChannel(CHANNEL, getString(R.string.server_channel_name), NotificationManager.IMPORTANCE_LOW);
        channel.setDescription(getString(R.string.server_channel_description));
        getSystemService(NotificationManager.class).createNotificationChannel(channel);
        connectivity = getSystemService(ConnectivityManager.class);
        networkCallback = new ConnectivityManager.NetworkCallback() {
            @Override public void onAvailable(Network n) { changed(); }
            @Override public void onLost(Network n) { changed(); }
            @Override public void onLinkPropertiesChanged(Network n, LinkProperties p) { changed(); }
            @Override public void onCapabilitiesChanged(Network n, NetworkCapabilities p) { changed(); }
            private void changed() {
                handler.removeCallbacks(checkNetwork);
                handler.postDelayed(checkNetwork, 1200);
            }
        };
        connectivity.registerDefaultNetworkCallback(networkCallback);
    }

    @Override public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null && STOP.equals(intent.getAction())) {
            wanted = false;
            ServerConfig.prefs(this).edit().putBoolean("wanted", false).apply();
            NativeServer.stop();
            show("stopping", getString(R.string.server_state_stopping));
            handler.removeCallbacks(poll);
            handler.post(poll);
            return START_NOT_STICKY;
        }
        wanted = ServerConfig.prefs(this).getBoolean("wanted", false);
        if (!wanted || exporting) { stopSelf(); return START_NOT_STICKY; }
        startPending = true;
        if (intent != null && RESTART.equals(intent.getAction())) {
            restart = true;
            NativeServer.stop();
        }
        phase = restart ? "restarting" : "starting";
        display = getString(restart ? R.string.server_state_restarting : R.string.server_state_starting);
        lastNotification = "";
        Notification notification = notification(display);
        if (Build.VERSION.SDK_INT >= 34)
            startForeground(NOTIFICATION, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE);
        else startForeground(NOTIFICATION, notification);
        networkKey = currentNetworkKey();
        handler.removeCallbacks(poll);
        handler.post(poll);
        return START_STICKY;
    }

    private String currentNetworkKey() {
        Network network = connectivity.getActiveNetwork();
        if (network == null) return "";
        NetworkCapabilities cap = connectivity.getNetworkCapabilities(network);
        if (cap == null) return "";
        if (ServerConfig.prefs(this).getBoolean("wifi_only", true) && !cap.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)) return "";
        LinkProperties links = connectivity.getLinkProperties(network);
        return links == null ? "" : network + ":" + links.getInterfaceName() + ":" + links.getLinkAddresses();
    }

    private final Runnable checkNetwork = () -> {
        if (!wanted || finishing) return;
        String key = currentNetworkKey();
        if (!Objects.equals(networkKey, key)) {
            networkKey = key;
            restart = true;
            NativeServer.stop();
        }
    };

    private final Runnable poll = new Runnable() {
        @Override public void run() {
            if (finishing) return;
            try {
                JSONObject status = new JSONObject(NativeServer.status());
                String state = status.optString("state", "stopped");
                boolean done = state.equals("stopped") || state.equals("failed");
                if (done) {
                    releaseLocks();
                    if (!wanted) { finishService(); return; }
                    if (state.equals("failed") && !restart && !startPending) {
                        ServerConfig.prefs(ServerService.this).edit().putString("error", status.optString("error")).putBoolean("wanted", false).apply();
                        wanted = false;
                        finishService();
                        return;
                    }
                    if (networkKey.isEmpty()) show("waiting", getString(ServerConfig.prefs(ServerService.this).getBoolean("wifi_only", true) ? R.string.server_state_waiting_wifi : R.string.server_state_waiting_network));
                    else {
                        // A finished Rust worker may publish its result just before
                        // its thread exits. Retrying here avoids a start/stop race.
                        acquireLocks();
                        String error = NativeServer.start(ServerConfig.json(ServerService.this));
                        if (!error.isEmpty() && !error.contains("still stopping")) throw new IllegalStateException(error);
                        restart = !error.isEmpty();
                        startPending = !error.isEmpty();
                        show("starting", getString(R.string.server_state_starting));
                    }
                } else if (!wanted || restart) {
                    NativeServer.stop();
                    show(wanted ? "restarting" : "stopping", getString(wanted ? R.string.server_state_restarting : R.string.server_state_stopping));
                } else if (state.equals("running")) {
                    startPending = false;
                    show("running", getString(R.string.server_state_running));
                } else show("starting", getString(R.string.server_state_starting));
            } catch (Exception | LinkageError error) {
                ServerConfig.prefs(ServerService.this).edit().putString("error", String.valueOf(error.getMessage())).putBoolean("wanted", false).apply();
                wanted = false;
                NativeServer.stop();
                finishService();
                return;
            }
            renewWakeLock();
            handler.postDelayed(this, 750);
        }
    };

    @SuppressWarnings("deprecation")
    private void acquireLocks() {
        if (wakeLock != null) return;
        wakeLock = getSystemService(PowerManager.class).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "AWServer:hosting");
        wakeLock.setReferenceCounted(false);
        renewWakeLock();
        WifiManager wifi = (WifiManager) getApplicationContext().getSystemService(WIFI_SERVICE);
        if (wifi != null) {
            wifiLock = wifi.createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "AWServer:wifi");
            wifiLock.setReferenceCounted(false);
            wifiLock.acquire();
            // A server is always found on the local network.
            multicastLock = wifi.createMulticastLock("AWServer:discovery");
            multicastLock.setReferenceCounted(false);
            multicastLock.acquire();
        }
    }

    private void renewWakeLock() {
        if (wakeLock != null && (!wakeLock.isHeld() || SystemClock.elapsedRealtime() - wakeRenewed > 5 * 60 * 1000)) {
            wakeLock.acquire(10 * 60 * 1000L);
            wakeRenewed = SystemClock.elapsedRealtime();
        }
    }

    private void releaseLocks() {
        if (multicastLock != null && multicastLock.isHeld()) multicastLock.release();
        if (wifiLock != null && wifiLock.isHeld()) wifiLock.release();
        if (wakeLock != null && wakeLock.isHeld()) wakeLock.release();
        multicastLock = null; wifiLock = null; wakeLock = null;
    }

    private Notification notification(String text) {
        PendingIntent open = PendingIntent.getActivity(this, 0, new Intent(this, ServerActivity.class), PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
        PendingIntent stop = PendingIntent.getService(this, 1, new Intent(this, ServerService.class).setAction(STOP), PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
        return new Notification.Builder(this, CHANNEL).setSmallIcon(R.drawable.server_ic_server)
            .setContentTitle(ServerConfig.prefs(this).getString("name", "AW Server"))
            .setContentText(text).setContentIntent(open).setOngoing(true).setOnlyAlertOnce(true)
            .setCategory(Notification.CATEGORY_SERVICE).setVisibility(Notification.VISIBILITY_PRIVATE)
            .addAction(new Notification.Action.Builder(null, getString(R.string.server_stop), stop).build()).build();
    }

    private void show(String state, String text) {
        phase = state;
        display = text;
        if (state.equals("running")) text = getString(R.string.server_notification_running, ServerConfig.prefs(this).getInt("port", 7447));
        if (!text.equals(lastNotification)) {
            getSystemService(NotificationManager.class).notify(NOTIFICATION, notification(text));
            lastNotification = text;
        }
    }

    private void finishService() {
        finishing = true;
        phase = "stopped";
        display = getString(R.string.server_state_stopped);
        releaseLocks();
        stopForeground(STOP_FOREGROUND_REMOVE);
        stopSelf();
    }

    @Override public void onDestroy() {
        handler.removeCallbacksAndMessages(null);
        if (connectivity != null && networkCallback != null) connectivity.unregisterNetworkCallback(networkCallback);
        NativeServer.stop();
        releaseLocks();
        active = false;
        phase = "stopped";
        display = getString(R.string.server_state_stopped);
        super.onDestroy();
    }
    @Override public IBinder onBind(Intent intent) { return null; }
}
