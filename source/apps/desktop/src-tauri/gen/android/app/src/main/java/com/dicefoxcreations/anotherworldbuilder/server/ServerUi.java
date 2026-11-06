package com.dicefoxcreations.anotherworldbuilder.server;

import com.dicefoxcreations.anotherworldbuilder.R;

import android.content.Context;
import android.content.res.ColorStateList;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.graphics.drawable.RippleDrawable;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.widget.*;

/** The app's Ember palette, DiceFox's warm browns, with native Android controls. */
final class ServerUi {
    static final int BASE = 0xff17120f, CARD = 0xff221c19, FIELD = 0xff2a2521;
    static final int TEXT = 0xffeee3d6, MUTED = 0xffa79e94, LINE = 0xff423c37;
    static final int ACCENT = 0xffe0a14a, SOFT = 0xff372918, DANGER = 0xffe18d85;
    private ServerUi() {}
    static int dp(Context c, int value) { return Math.round(value * c.getResources().getDisplayMetrics().density); }
    static GradientDrawable shape(Context c, int color, int border, int radius) {
        GradientDrawable d = new GradientDrawable(); d.setColor(color); d.setCornerRadius(dp(c, radius));
        if (border != 0) d.setStroke(dp(c, 1), border);
        return d;
    }
    static LinearLayout column(Context c) {
        LinearLayout box = new LinearLayout(c); box.setOrientation(LinearLayout.VERTICAL); return box;
    }
    static TextView text(LinearLayout parent, CharSequence content, int size, int color) {
        Context c = parent.getContext(); TextView text = new TextView(c);
        text.setText(content); text.setTextSize(size); text.setTextColor(color);
        text.setPadding(0, dp(c, 4), 0, dp(c, 4)); parent.addView(text); return text;
    }
    static TextView heading(LinearLayout parent, CharSequence content) {
        TextView text = text(parent, content, 17, TEXT); text.setTypeface(null, Typeface.BOLD); return text;
    }
    static void space(LinearLayout parent, int height) { parent.addView(new View(parent.getContext()), new LinearLayout.LayoutParams(1, dp(parent.getContext(), height))); }
    static LinearLayout card(LinearLayout parent) {
        Context c = parent.getContext(); LinearLayout card = column(c);
        card.setPadding(dp(c, 20), dp(c, 16), dp(c, 20), dp(c, 16));
        card.setBackground(shape(c, CARD, LINE, 12));
        LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        p.bottomMargin = dp(c, 16); parent.addView(card, p); return card;
    }
    static Button button(LinearLayout parent, CharSequence label, boolean primary, Runnable action) {
        Context c = parent.getContext(); Button button = new Button(c); button.setText(label); button.setAllCaps(false);
        button.setTextSize(15); button.setMinHeight(dp(c, 48)); button.setMinimumWidth(0); button.setMinWidth(0);
        button.setGravity(Gravity.CENTER); button.setPadding(dp(c, 14), dp(c, 4), dp(c, 14), dp(c, 4));
        int color = primary ? 0xff22180b : TEXT;
        button.setTextColor(new ColorStateList(new int[][] { new int[] {-android.R.attr.state_enabled}, new int[] {} }, new int[] { MUTED, color }));
        button.setBackground(new RippleDrawable(ColorStateList.valueOf(0x22e0a14a), shape(c, primary ? ACCENT : FIELD, primary ? 0 : LINE, 8), null));
        button.setOnClickListener(v -> action.run());
        LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        p.topMargin = dp(c, 10); parent.addView(button, p); return button;
    }
    static void enabled(Button button, boolean enabled) { button.setEnabled(enabled); button.setAlpha(enabled ? 1f : .5f); }
    static void enabled(ImageButton button, boolean enabled) { button.setEnabled(enabled); button.setAlpha(enabled ? 1f : .4f); }
    static ImageButton icon(LinearLayout parent, int drawable, String label, boolean primary, Runnable action) {
        Context c = parent.getContext(); ImageButton button = new ImageButton(c);
        button.setImageResource(drawable); button.setContentDescription(label); button.setTooltipText(label);
        button.setImageTintList(ColorStateList.valueOf(primary ? 0xff22180b : TEXT));
        button.setPadding(dp(c, 12), dp(c, 12), dp(c, 12), dp(c, 12));
        button.setBackground(new RippleDrawable(ColorStateList.valueOf(0x22e0a14a), shape(c, primary ? ACCENT : 0x00000000, 0, 8), null));
        button.setOnClickListener(v -> action.run());
        LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(dp(c, 48), dp(c, 48)); p.leftMargin = dp(c, 4);
        parent.addView(button, p); return button;
    }
}
