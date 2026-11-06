package com.dicefoxcreations.anotherworldbuilder.server;

import com.dicefoxcreations.anotherworldbuilder.R;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.util.Log;

public final class ServerReceiver extends BroadcastReceiver {
    @Override public void onReceive(Context context, Intent intent) {
        boolean boot = Intent.ACTION_BOOT_COMPLETED.equals(intent.getAction());
        boolean update = Intent.ACTION_MY_PACKAGE_REPLACED.equals(intent.getAction());
        if (!(boot || update)) return;
        if (!ServerConfig.prefs(context).getBoolean("wanted", false)) return;
        if (boot && !ServerConfig.prefs(context).getBoolean("boot", false)) return;
        try {
            context.startForegroundService(new Intent(context, ServerService.class));
        } catch (RuntimeException error) {
            ServerConfig.prefs(context).edit().putString("error", context.getString(R.string.server_resume_error, error.getMessage())).apply();
            Log.w("AWServer", "Cannot resume hosting", error);
        }
    }
}
