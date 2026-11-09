package com.dicefoxcreations.anotherworldbuilder.server;

import com.dicefoxcreations.anotherworldbuilder.R;
import com.dicefoxcreations.anotherworldbuilder.BuildConfig;

import android.Manifest;
import android.app.*;
import android.content.*;
import android.content.pm.PackageManager;
import android.graphics.Typeface;
import android.net.Uri;
import android.os.*;
import android.provider.Settings;
import android.text.InputType;
import android.text.TextUtils;
import android.text.Editable;
import android.text.TextWatcher;
import android.view.*;
import android.view.inputmethod.InputMethodManager;
import android.widget.*;
import org.json.*;
import java.io.*;
import java.util.Arrays;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import static com.dicefoxcreations.anotherworldbuilder.server.ServerUi.*;

public final class ServerActivity extends Activity {
    private static final int NOTIFICATIONS = 1, EXPORT = 2;
    enum Page { SERVER, OPTIONS, WORLDS, ABOUT }
    private Page page = Page.SERVER;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final ExecutorService reader = Executors.newSingleThreadExecutor();
    private FrameLayout root, content, drawer;
    private LinearLayout chrome, worldsList, optionActions;
    private ImageButton menu, save, reset, stop, restart, refreshWorlds;
    private TextView title, status, portValue, problem, code, worldsMessage;
    private Button start, export, copy;
    private EditText admin, name, port, addresses;
    private CheckBox wifiOnly, boot;
    private Bundle draft;
    private boolean advancedOpen, worldsLoading;
    private long nextWorldsRead;
    private String serverCode = "", lastWorlds = "", permissionAction;
    private String nativeState = "stopped";

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        if (state != null) {
            try { page = Page.valueOf(state.getString("page", "SERVER")); } catch (IllegalArgumentException ignored) { page = Page.SERVER; }
            draft = state.getBundle("draft"); advancedOpen = state.getBoolean("advanced");
        }
        root = new FrameLayout(this); root.setBackgroundColor(BASE);
        root.setFocusableInTouchMode(true);
        chrome = column(this); root.addView(chrome, new FrameLayout.LayoutParams(-1, -1));
        root.setOnApplyWindowInsetsListener((v, insets) -> {
            if (Build.VERSION.SDK_INT >= 30) {
                android.graphics.Insets bars = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
                android.graphics.Insets ime = insets.getInsets(WindowInsets.Type.ime());
                v.setPadding(bars.left, bars.top, bars.right, Math.max(bars.bottom, ime.bottom));
            } else v.setPadding(insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(), insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom());
            return insets;
        });
        LinearLayout bar = new LinearLayout(this); bar.setGravity(Gravity.CENTER_VERTICAL);
        bar.setPadding(dp(this, 8), 0, dp(this, 12), 0);
        chrome.addView(bar, new LinearLayout.LayoutParams(-1, dp(this, 64)));
        menu = new ImageButton(this); menu.setId(R.id.server_navigation_menu); menu.setImageResource(R.drawable.server_ic_menu);
        menu.setContentDescription(getString(R.string.server_menu_open)); menu.setBackgroundColor(android.graphics.Color.TRANSPARENT);
        menu.setPadding(dp(this, 12), dp(this, 12), dp(this, 12), dp(this, 12));
        menu.setOnClickListener(v -> { if (drawer == null) openDrawer(); else closeDrawer(); });
        bar.addView(menu, new LinearLayout.LayoutParams(dp(this, 48), dp(this, 48)));
        title = new TextView(this); title.setTextSize(20); title.setTextColor(TEXT); title.setTypeface(null, Typeface.BOLD);
        title.setSingleLine(true); title.setEllipsize(TextUtils.TruncateAt.END);
        LinearLayout.LayoutParams titleParams = new LinearLayout.LayoutParams(0, -2, 1); titleParams.leftMargin = dp(this, 8); bar.addView(title, titleParams);
        optionActions = new LinearLayout(this); bar.addView(optionActions);
        stop = toolbarAction(R.drawable.server_ic_stop, R.string.server_stop, R.id.server_stop, false, this::stopHosting);
        restart = toolbarAction(R.drawable.server_ic_restart, R.string.server_restart, R.id.server_restart, false, () -> beginHosting(ServerService.RESTART));
        reset = toolbarAction(R.drawable.server_ic_reset, R.string.server_reset, R.id.server_reset_options, false, this::resetOptions);
        save = toolbarAction(R.drawable.server_ic_save, R.string.server_save, R.id.server_save_options, true, this::saveOptions);
        refreshWorlds = toolbarAction(R.drawable.server_ic_refresh, R.string.server_refresh, R.id.server_refresh_worlds, false, () -> { nextWorldsRead = 0; loadWorlds(); });
        View line = new View(this); line.setBackgroundColor(LINE); chrome.addView(line, new LinearLayout.LayoutParams(-1, dp(this, 1)));
        content = new FrameLayout(this); chrome.addView(content, new LinearLayout.LayoutParams(-1, 0, 1));
        setContentView(root);
        getWindow().setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE | WindowManager.LayoutParams.SOFT_INPUT_STATE_ALWAYS_HIDDEN);
        getWindow().getDecorView().setSystemUiVisibility(0);
        if (Build.VERSION.SDK_INT >= 33) BackNavigation.install(this, this::navigateBack);
        showPage(page);
    }

    private ImageButton toolbarAction(int drawable, int label, int id, boolean primary, Runnable action) {
        ImageButton actionButton = icon(optionActions, drawable, getString(label), primary, action); actionButton.setId(id); return actionButton;
    }

    @android.annotation.TargetApi(33)
    private static final class BackNavigation {
        static void install(Activity activity, Runnable back) {
            activity.getOnBackInvokedDispatcher().registerOnBackInvokedCallback(android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT, back::run);
        }
    }
    // API 33+ uses the native dispatcher above, including predictive back.
    @android.annotation.SuppressLint("GestureBackNavigation")
    @Override public void onBackPressed() { navigateBack(); }
    private void navigateBack() {
        if (drawer != null) closeDrawer();
        else if (page != Page.SERVER) showPage(Page.SERVER);
        else finish();
    }
    @Override protected void onSaveInstanceState(Bundle state) {
        captureDraft(); state.putString("page", page.name()); state.putBundle("draft", draft); state.putBoolean("advanced", advancedOpen);
        super.onSaveInstanceState(state);
    }

    private void openDrawer() {
        drawer = new FrameLayout(this); drawer.setId(R.id.server_navigation_drawer);
        View shade = new View(this); shade.setBackgroundColor(0x99000000); shade.setContentDescription(getString(R.string.server_menu_close)); shade.setFocusable(true); shade.setOnClickListener(v -> closeDrawer()); drawer.addView(shade, new FrameLayout.LayoutParams(-1, -1));
        LinearLayout panel = column(this); panel.setBackgroundColor(CARD); panel.setClickable(true); panel.setElevation(dp(this, 12));
        panel.setPadding(dp(this, 20), dp(this, 28), dp(this, 20), dp(this, 20));
        int width = Math.min(dp(this, 304), getResources().getDisplayMetrics().widthPixels - dp(this, 56));
        drawer.addView(panel, new FrameLayout.LayoutParams(width, -1, Gravity.START));
        ImageView logo = new ImageView(this); logo.setImageResource(R.drawable.app_icon); logo.setContentDescription(getString(R.string.server_app_name)); panel.addView(logo, new LinearLayout.LayoutParams(dp(this, 40), dp(this, 40)));
        space(panel, 12); heading(panel, getString(R.string.server_product_name)); space(panel, 24);
        nav(panel, Page.SERVER, R.string.server_server, R.id.server_nav_server);
        nav(panel, Page.OPTIONS, R.string.server_options, R.id.server_nav_options);
        nav(panel, Page.WORLDS, R.string.server_worlds, R.id.server_nav_worlds);
        nav(panel, Page.ABOUT, R.string.server_about, R.id.server_nav_about);
        panel.addView(new View(this), new LinearLayout.LayoutParams(1, 0, 1));
        text(panel, getString(R.string.server_version, BuildConfig.VERSION_NAME), 12, MUTED);
        chrome.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS);
        root.addView(drawer, new FrameLayout.LayoutParams(-1, -1));
    }
    private void nav(LinearLayout panel, Page destination, int label, int id) {
        Button item = button(panel, getString(label), false, () -> { closeDrawer(); showPage(destination); }); item.setId(id);
        item.setGravity(Gravity.START | Gravity.CENTER_VERTICAL);
        if (page == destination) { item.setBackground(shape(this, SOFT, 0, 8)); item.setTextColor(ACCENT); }
    }
    private void closeDrawer() {
        if (drawer != null) { root.removeView(drawer); drawer = null; chrome.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_AUTO); }
    }
    private void showPage(Page next) {
        captureDraft(); page = next;
        ((InputMethodManager)getSystemService(INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(root.getWindowToken(), 0);
        content.removeAllViews(); status = portValue = problem = code = worldsMessage = null;
        start = export = copy = null;
        admin = name = port = addresses = null; wifiOnly = boot = null; worldsList = null;
        title.setText(getString(next == Page.SERVER ? R.string.server_app_name : next == Page.OPTIONS ? R.string.server_options : next == Page.WORLDS ? R.string.server_worlds : R.string.server_about));
        for (View action : new View[] {save, reset}) action.setVisibility(next == Page.OPTIONS ? View.VISIBLE : View.GONE);
        for (View action : new View[] {stop, restart}) action.setVisibility(next == Page.SERVER ? View.VISIBLE : View.GONE);
        refreshWorlds.setVisibility(next == Page.WORLDS ? View.VISIBLE : View.GONE);
        LinearLayout body = column(this); body.setPadding(dp(this, 24), dp(this, 20), dp(this, 24), dp(this, 28));
        ScrollView scroll = new ScrollView(this); scroll.setFillViewport(true); scroll.setBackgroundColor(BASE); scroll.addView(body); content.addView(scroll);
        if (next == Page.SERVER) home(body);
        else if (next == Page.OPTIONS) options(body);
        else if (next == Page.WORLDS) worlds(body);
        else about(body);
        root.requestFocus(); refreshNow();
    }
    private void home(LinearLayout body) {
        space(body, 12); LinearLayout card = card(body);
        text(card, getString(R.string.server_server), 14, MUTED); space(card, 8);
        status = text(card, getString(R.string.server_state_stopped), 30, MUTED); status.setId(R.id.server_status); status.setTypeface(null, Typeface.BOLD);
        space(card, 28);
        text(card, getString(R.string.server_port), 14, MUTED);
        portValue = text(card, Integer.toString(ServerConfig.prefs(this).getInt("port", 7447)), 25, TEXT); portValue.setId(R.id.server_port);
        text(card, getString(R.string.server_port_protocols), 13, MUTED); space(card, 24);
        start = button(card, getString(R.string.server_start), true, this::prepareStart); start.setId(R.id.server_start);
        problem = text(body, "", 14, DANGER); problem.setTextIsSelectable(true);
    }
    private Bundle optionValues() {
        if (draft == null) {
            SharedPreferences p = ServerConfig.prefs(this); draft = new Bundle();
            // Not set up yet: the app's own profile here is its admin, until saved otherwise.
            String admin = p.getString("admin", "");
            draft.putString("admin", admin.isEmpty() ? ServerConfig.ownId(this) : admin); draft.putString("name", p.getString("name", ServerConfig.DEFAULT_NAME));
            draft.putString("port", Integer.toString(p.getInt("port", 7447))); draft.putString("addresses", p.getString("addresses", ""));
            draft.putBoolean("wifi_only", p.getBoolean("wifi_only", true)); draft.putBoolean("boot", p.getBoolean("boot", false));
        }
        return draft;
    }
    private void options(LinearLayout body) {
        Bundle d = optionValues(); LinearLayout settings = card(body); heading(settings, getString(R.string.server_server));
        admin = field(settings, R.string.server_admin_id, d.getString("admin", ""), R.id.server_admin_id, InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
        text(settings, getString(ServerConfig.ownId(this).isEmpty() ? R.string.server_admin_copy : R.string.server_admin_hint), 12, MUTED);
        name = field(settings, R.string.server_name, d.getString("name", ServerConfig.DEFAULT_NAME), R.id.server_name, InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_CAP_SENTENCES);
        port = field(settings, R.string.server_port, d.getString("port", "7447"), R.id.server_port, InputType.TYPE_CLASS_NUMBER);
        text(body, getString(R.string.server_restart_hint), 13, MUTED); space(body, 12);
        Button advanced = button(body, getString(R.string.server_advanced), false, () -> {
            captureDraft(); advancedOpen = !advancedOpen; showPage(Page.OPTIONS);
        }); advanced.setId(R.id.server_advanced_options);
        if (advancedOpen) {
            space(body, 12); LinearLayout network = card(body); heading(network, getString(R.string.server_network));
            addresses = field(network, R.string.server_advertised_addresses, d.getString("addresses", ""), R.id.server_addresses, InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_URI);
            text(network, getString(R.string.server_addresses_hint), 12, MUTED);
            wifiOnly = check(network, R.string.server_wifi_only, d.getBoolean("wifi_only", true), R.id.server_wifi_only);
            boot = check(network, R.string.server_boot_resume, d.getBoolean("boot", false), R.id.server_resume_on_boot);
            text(network, getString(R.string.server_boot_hint), 12, MUTED);
            LinearLayout system = card(body); heading(system, getString(R.string.server_android_settings));
            button(system, getString(R.string.server_battery_settings), false, () -> openExternal(new Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)));
            button(system, getString(R.string.server_notification_settings), false, () -> openExternal(new Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, getPackageName())));
        }
        space(body, 16); LinearLayout connection = card(body); heading(connection, getString(R.string.server_connection));
        code = text(connection, getString(R.string.server_connection_hint), 12, MUTED); code.setTextIsSelectable(true); code.setId(R.id.server_connection_code);
        copy = button(connection, getString(R.string.server_copy_code), false, () -> {
            getSystemService(ClipboardManager.class).setPrimaryClip(ClipData.newPlainText(getString(R.string.server_connection_code), serverCode));
            Toast.makeText(this, R.string.server_code_copied, Toast.LENGTH_SHORT).show();
        }); copy.setId(R.id.server_copy_code);
        LinearLayout maintenance = card(body); heading(maintenance, getString(R.string.server_maintenance));
        export = button(maintenance, getString(R.string.server_export_backup), false, this::chooseExport); export.setId(R.id.server_export);
        text(maintenance, getString(R.string.server_backup_hint), 12, MUTED);
        button(maintenance, getString(R.string.server_view_log), false, this::showLog);
        TextWatcher changes = new TextWatcher() {
            @Override public void beforeTextChanged(CharSequence s, int start, int count, int after) { }
            @Override public void onTextChanged(CharSequence s, int start, int before, int count) { }
            @Override public void afterTextChanged(Editable s) { refreshNow(); }
        };
        for (EditText field : new EditText[] {admin, name, port, addresses}) if (field != null) field.addTextChangedListener(changes);
        for (CheckBox toggle : new CheckBox[] {wifiOnly, boot}) if (toggle != null) toggle.setOnCheckedChangeListener((v, checked) -> refreshNow());
        reset.setContentDescription(getString(R.string.server_reset_hint));
    }
    private EditText field(LinearLayout parent, int label, String value, int id, int input) {
        space(parent, 12); text(parent, getString(label), 13, MUTED);
        EditText view = new EditText(this); view.setId(id); view.setInputType(input); view.setSingleLine(true); view.setText(value); view.setTextSize(16); view.setTextColor(TEXT);
        view.setPadding(0, dp(this, 8), 0, dp(this, 8)); view.setMinHeight(dp(this, 48)); parent.addView(view); return view;
    }
    private CheckBox check(LinearLayout parent, int label, boolean checked, int id) {
        CheckBox view = new CheckBox(this); view.setId(id); view.setText(label); view.setTextColor(TEXT); view.setChecked(checked); view.setMinHeight(dp(this, 48)); parent.addView(view); return view;
    }
    private void captureDraft() {
        if (admin == null) return;
        Bundle d = optionValues(); d.putString("admin", admin.getText().toString()); d.putString("name", name.getText().toString()); d.putString("port", port.getText().toString());
        if (addresses != null) { d.putString("addresses", addresses.getText().toString()); d.putBoolean("wifi_only", wifiOnly.isChecked()); d.putBoolean("boot", boot.isChecked()); }
    }
    private boolean dirty() {
        if (admin == null) return false; captureDraft(); SharedPreferences p = ServerConfig.prefs(this); Bundle d = optionValues();
        return !d.getString("admin", "").equals(p.getString("admin", "")) || !d.getString("name", "").equals(p.getString("name", ServerConfig.DEFAULT_NAME))
            || !d.getString("port", "").equals(Integer.toString(p.getInt("port", 7447))) || !d.getString("addresses", "").equals(p.getString("addresses", ""))
            || d.getBoolean("wifi_only") != p.getBoolean("wifi_only", true) || d.getBoolean("boot") != p.getBoolean("boot", false);
    }
    private void saveOptions() {
        try {
            captureDraft(); Bundle d = optionValues(); int number;
            try { number = Integer.parseInt(d.getString("port", "").trim()); } catch (NumberFormatException error) { throw new IllegalArgumentException(getString(R.string.server_invalid_port)); }
            String id = ServerConfig.normalizeAdmin(d.getString("admin", "")), serverName = d.getString("name", "").trim();
            ServerConfig.validate(id, serverName, number);
            if (!dirty()) { Toast.makeText(this, R.string.server_no_changes, Toast.LENGTH_SHORT).show(); return; }
            boolean resume = desired();
            ServerConfig.prefs(this).edit().putString("admin", id).putString("name", serverName).putInt("port", number)
                .putString("addresses", d.getString("addresses", "").trim())
                .putBoolean("wifi_only", d.getBoolean("wifi_only")).putBoolean("boot", d.getBoolean("boot")).putString("error", "").apply();
            draft = null;
            if (resume) beginHosting(ServerService.RESTART);
            // Rebuild from saved, normalized values rather than retaining old edit fields.
            admin = name = port = addresses = null; showPage(Page.OPTIONS);
            Toast.makeText(this, resume ? R.string.server_saved_restart : R.string.server_saved, Toast.LENGTH_SHORT).show();
        } catch (Exception error) { message(error.getMessage()); }
    }
    private void resetOptions() {
        draft = null;
        // Discard edit fields before rebuilding so captureDraft cannot restore them.
        admin = name = port = addresses = null;
        showPage(Page.OPTIONS);
        Toast.makeText(this, R.string.server_reset_done, Toast.LENGTH_SHORT).show();
    }
    private void worlds(LinearLayout body) {
        worldsMessage = text(body, getString(R.string.server_worlds_loading), 14, MUTED);
        worldsList = column(this); worldsList.setId(R.id.server_hosted_worlds); body.addView(worldsList);
        text(body, getString(R.string.server_worlds_hint), 13, MUTED); lastWorlds = ""; nextWorldsRead = 0;
    }
    private void loadWorlds() {
        if (page != Page.WORLDS || worldsLoading || isDestroyed()) return;
        worldsLoading = true; nextWorldsRead = SystemClock.elapsedRealtime() + 4000;
        String path = ServerConfig.data(this).getAbsolutePath();
        reader.execute(() -> {
            String value;
            try { value = NativeServer.worlds(path); } catch (LinkageError error) { value = ""; }
            String result = value;
            handler.post(() -> {
                worldsLoading = false;
                if (isDestroyed() || page != Page.WORLDS || worldsList == null || (!result.isEmpty() && result.equals(lastWorlds))) return;
                lastWorlds = result; renderWorlds(result);
            });
        });
    }
    private void renderWorlds(String result) {
        try {
            JSONObject response = new JSONObject(result); String failure = response.optString("error"); JSONArray entries = response.getJSONArray("worlds");
            worldsList.removeAllViews();
            worldsMessage.setVisibility(failure.isEmpty() ? View.GONE : View.VISIBLE);
            updateText(worldsMessage, failure.isEmpty() ? "" : getString(R.string.server_worlds_error, failure));
            if (entries.length() == 0 && failure.isEmpty()) {
                LinearLayout empty = card(worldsList); heading(empty, getString(R.string.server_worlds_empty)); text(empty, getString(R.string.server_worlds_empty_hint), 14, MUTED);
            }
            for (int i = 0; i < entries.length(); i++) {
                JSONObject world = entries.getJSONObject(i); LinearLayout row = card(worldsList);
                TextView heading = heading(row, world.optString("icon", "🌍") + "  " + world.getString("name")); heading.setMaxLines(2); heading.setEllipsize(TextUtils.TruncateAt.END);
                if (!world.optString("error").isEmpty()) text(row, getString(R.string.server_world_unavailable), 13, DANGER);
                else {
                    int articles = world.optInt("articles"), members = world.optInt("members");
                    text(row, getString(R.string.server_world_counts, getResources().getQuantityString(R.plurals.server_article_count, articles, articles), getResources().getQuantityString(R.plurals.server_member_count, members, members)), 13, MUTED);
                    if (world.optBoolean("closed")) text(row, getString(R.string.server_world_closed), 13, MUTED);
                }
            }
        } catch (JSONException error) { worldsMessage.setVisibility(View.VISIBLE); updateText(worldsMessage, getString(R.string.server_worlds_error, error.getMessage())); }
    }
    private void about(LinearLayout body) {
        LinearLayout identity = card(body); ImageView logo = new ImageView(this); logo.setImageResource(R.drawable.app_icon); logo.setContentDescription(getString(R.string.server_app_name));
        identity.addView(logo, new LinearLayout.LayoutParams(dp(this, 64), dp(this, 64))); space(identity, 16);
        heading(identity, getString(R.string.server_product_name)); text(identity, getString(R.string.server_about_description), 14, MUTED);
        space(identity, 16); text(identity, getString(R.string.server_version, BuildConfig.VERSION_NAME), 14, TEXT); text(identity, getString(R.string.server_publisher), 14, MUTED); text(identity, getString(R.string.server_platform), 13, MUTED);
        Button licenses = button(body, getString(R.string.server_licenses), false, this::showLicenses); licenses.setId(R.id.server_third_party_licenses);
        button(body, getString(R.string.server_app_license), false, () -> showAsset(R.string.server_app_license, "APP-LICENSE.txt"));
        button(body, getString(R.string.server_releases), false, () -> openExternal(new Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/dicefoxcreations/AnotherWorldbuilder/releases"))));
        space(body, 16); text(body, getString(R.string.server_copyright), 12, MUTED);
    }
    private boolean desired() { return ServerService.active || nativeState.matches("starting|running|stopping"); }
    private boolean configured() { try { ServerConfig.json(this); return true; } catch (Exception error) { return false; } }
    private void prepareStart() {
        if (!configured()) { showPage(Page.OPTIONS); return; }
        if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            permissionAction = null; requestPermissions(new String[] {Manifest.permission.POST_NOTIFICATIONS}, NOTIFICATIONS); return;
        }
        if (!getSystemService(NotificationManager.class).areNotificationsEnabled()) { message(getString(R.string.server_notification_required)); return; }
        beginHosting(null);
    }
    private void beginHosting(String action) {
        if (ServerService.exporting) { message(getString(R.string.server_backup_busy)); return; }
        try {
            ServerConfig.json(this);
            ServerConfig.prefs(this).edit().putBoolean("wanted", true).putString("error", "").apply();
            Intent intent = new Intent(this, ServerService.class); if (action != null) intent.setAction(action); startForegroundService(intent);
        } catch (Exception | LinkageError error) { ServerConfig.prefs(this).edit().putBoolean("wanted", false).apply(); message(error.getMessage()); }
    }
    private void stopHosting() { startService(new Intent(this, ServerService.class).setAction(ServerService.STOP)); }
    @Override public void onRequestPermissionsResult(int request, String[] permissions, int[] results) {
        super.onRequestPermissionsResult(request, permissions, results);
        if (request == NOTIFICATIONS && results.length > 0) {
            if (results[0] == PackageManager.PERMISSION_GRANTED) beginHosting(permissionAction);
            else message(getString(R.string.server_notification_required));
        }
    }
    private static void updateText(TextView view, CharSequence value) { if (view != null && !TextUtils.equals(view.getText(), value)) view.setText(value); }
    private void refreshNow() {
        try {
            JSONObject snapshot = new JSONObject(NativeServer.status()); nativeState = snapshot.optString("state", "stopped");
            boolean busy = desired(), transitioning = nativeState.equals("starting") || nativeState.equals("stopping") || ServerService.phase.equals("restarting");
            serverCode = nativeState.equals("running") ? snapshot.optString("code") : "";
            if (status != null) {
                String phase = ServerService.active ? ServerService.phase : nativeState;
                updateText(status, ServerService.active ? ServerService.display : getString(nativeState.equals("stopping") ? R.string.server_state_stopping : R.string.server_state_stopped));
                status.setTextColor(phase.equals("running") ? ACCENT : phase.matches("starting|stopping|restarting|waiting") ? 0xffd6b46a : MUTED);
                updateText(portValue, Integer.toString(ServerConfig.prefs(this).getInt("port", 7447)));
                start.setVisibility(busy ? View.GONE : View.VISIBLE); stop.setVisibility(busy ? View.VISIBLE : View.GONE); restart.setVisibility(busy ? View.VISIBLE : View.GONE);
                updateText(start, getString(configured() ? R.string.server_start : R.string.server_setup));
                enabled(start, !ServerService.exporting); enabled(stop, !nativeState.equals("stopping")); enabled(restart, !transitioning && !ServerService.exporting);
                String error = ServerConfig.prefs(this).getString("error", ""); updateText(problem, error); problem.setVisibility(error.isEmpty() ? View.GONE : View.VISIBLE);
            }
            if (page == Page.OPTIONS) {
                boolean changed = dirty();
                save.setContentDescription(getString(busy ? R.string.server_save_restart : R.string.server_save));
                save.setTooltipText(getString(busy ? R.string.server_save_restart : R.string.server_save));
                enabled(save, changed && !transitioning && !ServerService.exporting); enabled(reset, changed);
                updateText(code, serverCode.isEmpty() ? getString(R.string.server_connection_hint) : serverCode); enabled(copy, !serverCode.isEmpty());
                enabled(export, !busy && !ServerService.exporting && ServerConfig.data(this).isDirectory());
            }
            if (page == Page.WORLDS && SystemClock.elapsedRealtime() >= nextWorldsRead) loadWorlds();
        } catch (Exception | LinkageError error) {
            if (problem != null) { updateText(problem, getString(R.string.server_native_error, error.getMessage())); problem.setVisibility(View.VISIBLE); }
            if (start != null) enabled(start, false);
        }
    }
    private final Runnable refresh = new Runnable() { @Override public void run() { refreshNow(); handler.postDelayed(this, 750); } };
    @Override protected void onResume() { super.onResume(); handler.post(refresh); }
    @Override protected void onPause() { handler.removeCallbacks(refresh); super.onPause(); }
    @Override protected void onDestroy() { handler.removeCallbacksAndMessages(null); reader.shutdownNow(); super.onDestroy(); }

    private void chooseExport() {
        if (desired() || ServerService.exporting) return; ServerService.exporting = true;
        try { startActivityForResult(new Intent(Intent.ACTION_CREATE_DOCUMENT).setType("application/zip").addCategory(Intent.CATEGORY_OPENABLE).putExtra(Intent.EXTRA_TITLE, getString(R.string.server_backup_filename)), EXPORT); }
        catch (RuntimeException error) { ServerService.exporting = false; message(error.getMessage()); }
    }
    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request != EXPORT) return;
        if (result != RESULT_OK || data == null || data.getData() == null) { ServerService.exporting = false; return; }
        Uri destination = data.getData(); Context context = getApplicationContext();
        new Thread(() -> {
            String failure = null; try { ServerBackup.write(context, destination); } catch (Exception error) { failure = error.getMessage(); } finally { ServerService.exporting = false; }
            String text = failure == null ? context.getString(R.string.server_backup_done) : context.getString(R.string.server_backup_failed, failure);
            new Handler(Looper.getMainLooper()).post(() -> Toast.makeText(context, text, Toast.LENGTH_LONG).show());
        }, "aw-export").start();
    }
    private void showLog() {
        reader.execute(() -> {
            StringBuilder lines = new StringBuilder(); File[] logs = new File(ServerConfig.data(this), "logs").listFiles((dir, file) -> file.endsWith(".log"));
            if (logs != null && logs.length > 0) {
                Arrays.sort(logs);
                try (RandomAccessFile input = new RandomAccessFile(logs[logs.length - 1], "r")) {
                    long offset = Math.max(0, input.length() - 32000); input.seek(offset); byte[] bytes = new byte[(int)(input.length() - offset)]; input.readFully(bytes);
                    lines.append(new String(bytes, java.nio.charset.StandardCharsets.UTF_8));
                } catch (IOException error) { lines.append(error.getMessage()); }
            }
            showText(R.string.server_log_title, lines.length() == 0 ? getString(R.string.server_log_empty) : lines.toString());
        });
    }
    private void showLicenses() {
        showAsset(R.string.server_licenses, "THIRD-PARTY-NOTICES.txt");
    }
    private void showAsset(int label, String asset) {
        reader.execute(() -> {
            StringBuilder license = new StringBuilder();
            try (BufferedReader input = new BufferedReader(new InputStreamReader(getAssets().open(asset), java.nio.charset.StandardCharsets.UTF_8))) {
                String line; while ((line = input.readLine()) != null) license.append(line).append('\n');
            } catch (IOException error) { license.append(error.getMessage()); }
            showText(label, license.toString());
        });
    }
    private void showText(int label, String value) {
        handler.post(() -> {
            if (isFinishing() || isDestroyed()) return;
            TextView text = new TextView(this); text.setText(value); text.setTextSize(13); text.setTextIsSelectable(true); text.setTextColor(TEXT); text.setPadding(dp(this, 20), dp(this, 16), dp(this, 20), dp(this, 16));
            ScrollView scroll = new ScrollView(this); scroll.addView(text); new AlertDialog.Builder(this).setTitle(label).setView(scroll).setPositiveButton(R.string.server_close, null).show();
        });
    }
    private void openExternal(Intent intent) { try { startActivity(intent); } catch (ActivityNotFoundException error) { message(getString(R.string.server_settings_unavailable)); } }
    private void message(String value) {
        String error = value == null ? getString(R.string.server_unknown_error) : value;
        ServerConfig.prefs(this).edit().putString("error", error).apply(); Toast.makeText(this, error, Toast.LENGTH_LONG).show(); refreshNow();
    }
}
