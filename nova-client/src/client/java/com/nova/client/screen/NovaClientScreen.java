package com.nova.client.screen;

import com.nova.client.config.ClientConfig;
import com.nova.client.config.ClientConfig.NovaConfig;
import net.minecraft.client.gui.Click;
import net.minecraft.client.gui.DrawContext;
import net.minecraft.client.gui.screen.Screen;
import net.minecraft.text.Text;

public final class NovaClientScreen extends Screen {
    private static final int BG = 0xF00D1420;
    private static final int SIDEBAR = 0xFF0A111C;
    private static final int CARD = 0xFF151F2E;
    private static final int TEXT = 0xFFF0F4FA;
    private static final int MUTED = 0xFF8C9CB2;
    private int page;

    public NovaClientScreen() { super(Text.literal("NOVA Client")); }

    private int panelWidth() { return Math.max(390, Math.min(780, width - 28)); }
    private int panelHeight() { return Math.max(320, Math.min(460, height - 28)); }
    private int panelX() { return (width - panelWidth()) / 2; }
    private int panelY() { return (height - panelHeight()) / 2; }
    private int contentX() { return panelX() + 176; }
    private int contentY() { return panelY() + 94; }
    private boolean inside(double mx, double my, int x, int y, int w, int h) {
        return mx >= x && mx <= x + w && my >= y && my <= y + h;
    }

    @Override
    public void render(DrawContext context, int mouseX, int mouseY, float delta) {
        NovaConfig config = ClientConfig.CONFIG;
        int x = panelX();
        int y = panelY();
        int w = panelWidth();
        int h = panelHeight();
        int accent = ClientConfig.accentColor();
        String[] tabs = {"Overview", "HUD & Modules", "Performance", "Profiles"};

        context.fill(0, 0, width, height, 0xA900050B);
        context.fill(x, y, x + w, y + h, BG);
        context.fill(x, y, x + w, y + 2, accent);
        context.fill(x, y + 58, x + 154, y + h - 1, SIDEBAR);
        context.fill(x + 154, y + 58, x + 155, y + h - 1, 0xFF263449);
        context.fill(x, y + h - 1, x + w, y + h, 0xFF263449);

        context.drawTextWithShadow(textRenderer, "NOVA", x + 20, y + 18, accent);
        context.drawTextWithShadow(textRenderer, "CLIENT", x + 73, y + 18, TEXT);
        context.drawTextWithShadow(textRenderer, "LIGHTWEIGHT GAME LAYER", x + 20, y + 38, MUTED);
        context.drawTextWithShadow(textRenderer, "RIGHT SHIFT  /  MENU", x + w - 154, y + 23, MUTED);

        for (int i = 0; i < tabs.length; i++) {
            int ty = y + 78 + i * 43;
            if (i == page) {
                context.fill(x + 9, ty - 5, x + 145, ty + 25, 0xFF1D2A3D);
                context.fill(x + 9, ty - 5, x + 11, ty + 25, accent);
            }
            context.drawTextWithShadow(textRenderer, tabs[i], x + 20, ty + 5, i == page ? TEXT : MUTED);
        }

        context.drawTextWithShadow(textRenderer, "NOVA CLIENT  /  " + tabs[page].toUpperCase(),
                contentX(), y + 73, accent);
        context.drawTextWithShadow(textRenderer, subtitle(), contentX(), y + 86, MUTED);

        if (page == 0) renderOverview(context, config, mouseX, mouseY);
        else if (page == 1) renderHudPage(context, config, mouseX, mouseY);
        else if (page == 2) renderPerformancePage(context, config, mouseX, mouseY);
        else renderProfilesPage(context, config, mouseX, mouseY);

        context.drawTextWithShadow(textRenderer, "NOVA CLIENT  ·  FABRIC 1.21.11",
                x + 18, y + h - 18, 0xFF60718A);
        context.drawTextWithShadow(textRenderer, "ESC  CLOSE", x + w - 88, y + h - 18, MUTED);
        super.render(context, mouseX, mouseY, delta);
    }

    private String subtitle() {
        return switch (page) {
            case 0 -> "A focused control surface for the game you play.";
            case 1 -> "Build a cleaner HUD without clutter.";
            case 2 -> "Metrics are sampled off the render path.";
            default -> "Separate layouts for different play styles.";
        };
    }

    private void renderOverview(DrawContext c, NovaConfig cfg, int mx, int my) {
        int x = contentX();
        int y = contentY();
        int usable = panelWidth() - 194;
        c.fill(x, y, x + usable, y + 112, CARD);
        c.fill(x, y, x + 2, y + 112, ClientConfig.accentColor());
        c.drawTextWithShadow(textRenderer, "YOUR GAME. YOUR RULES.", x + 16, y + 16, TEXT);
        c.drawTextWithShadow(textRenderer, "A low-overhead client layer with personal control.",
                x + 16, y + 37, MUTED);
        c.drawTextWithShadow(textRenderer, "No world scans or network work in the HUD renderer.",
                x + 16, y + 57, MUTED);
        drawAction(c, x, y + 132, 190, 34, "OPEN HUD EDITOR", mx, my);
        drawAction(c, x + 202, y + 132, 156, 34, "THEME: " + cfg.theme, mx, my);
        c.drawTextWithShadow(textRenderer, "ACTIVE PROFILE", x, y + 195, MUTED);
        c.drawTextWithShadow(textRenderer, cfg.profile, x, y + 211, TEXT);
        c.drawTextWithShadow(textRenderer, "MENU  Right Shift", x + 185, y + 195, MUTED);
        c.drawTextWithShadow(textRenderer, "HUD EDITOR  Right Ctrl", x + 185, y + 211, MUTED);
        c.drawTextWithShadow(textRenderer, "TOGGLE HUD  F8", x + 185, y + 227, MUTED);
    }

    private void renderHudPage(DrawContext c, NovaConfig cfg, int mx, int my) {
        int x = contentX();
        int y = contentY();
        drawToggle(c, x, y, "Master HUD", "Show or hide every NOVA overlay.", cfg.showHud, mx, my);
        drawToggle(c, x, y + 36, "Performance panel", "FPS, ping, clicks, coordinates and memory.", cfg.showStats, mx, my);
        drawToggle(c, x, y + 72, "Keystrokes", "Live WASD and jump display.", cfg.showKeystrokes, mx, my);
        drawToggle(c, x, y + 108, "FPS / frame time", "Approximate frame-time readout.", cfg.showFps, mx, my);
        drawToggle(c, x, y + 144, "Network latency", "Server ping from the player list.", cfg.showPing, mx, my);
        drawToggle(c, x, y + 180, "Left CPS", "Counts attack-button click events.", cfg.showCps, mx, my);
        drawToggle(c, x, y + 216, "Coordinates", "Block position sampled on tick.", cfg.showCoordinates, mx, my);
        drawAction(c, x, y + 264, 190, 32, "EDIT HUD LAYOUT", mx, my);
    }

    private void renderPerformancePage(DrawContext c, NovaConfig cfg, int mx, int my) {
        int x = contentX();
        int y = contentY();
        drawToggle(c, x, y, "Low-overhead mode", "Displays an approximate frame-time readout.", cfg.lowOverheadMode, mx, my);
        drawToggle(c, x, y + 40, "Memory indicator", "Show Java heap usage in the HUD.", cfg.showMemory, mx, my);
        int usable = panelWidth() - 194;
        c.fill(x, y + 104, x + usable, y + 214, CARD);
        c.drawTextWithShadow(textRenderer, "NOVA PERFORMANCE POLICY", x + 14, y + 118, ClientConfig.accentColor());
        c.drawTextWithShadow(textRenderer, "• Metrics sampled every five ticks", x + 14, y + 142, TEXT);
        c.drawTextWithShadow(textRenderer, "• Heap sampled every twenty ticks", x + 14, y + 161, TEXT);
        c.drawTextWithShadow(textRenderer, "• No HTTP or file I/O during rendering", x + 14, y + 180, TEXT);
        c.drawTextWithShadow(textRenderer, "• No chunk or entity iteration", x + 14, y + 199, TEXT);
    }

    private void renderProfilesPage(DrawContext c, NovaConfig cfg, int mx, int my) {
        int x = contentX();
        int y = contentY();
        c.drawTextWithShadow(textRenderer, "CURRENT PROFILE", x, y, MUTED);
        c.drawTextWithShadow(textRenderer, cfg.profile, x, y + 18, TEXT);
        drawAction(c, x, y + 54, 108, 34, "DEFAULT", mx, my);
        drawAction(c, x + 118, y + 54, 108, 34, "COMPETITIVE", mx, my);
        drawAction(c, x + 236, y + 54, 108, 34, "MINIMAL", mx, my);
        drawAction(c, x, y + 104, 180, 32, "SAVE CURRENT PROFILE", mx, my);
        c.drawTextWithShadow(textRenderer, "Profiles keep their own toggles, theme and HUD positions.",
                x, y + 162, MUTED);
        c.drawTextWithShadow(textRenderer, "Stored locally inside the Minecraft config folder.",
                x, y + 180, MUTED);
    }

    private void drawToggle(DrawContext c, int x, int y, String title, String description,
                            boolean enabled, int mx, int my) {
        int usable = panelWidth() - 194;
        boolean hover = inside(mx, my, x, y, usable, 32);
        c.fill(x, y, x + usable, y + 32, hover ? 0xFF1C2A3E : CARD);
        c.drawTextWithShadow(textRenderer, title, x + 10, y + 5, TEXT);
        c.drawTextWithShadow(textRenderer, description, x + 10, y + 18, MUTED);
        int pillX = x + usable - 44;
        c.fill(pillX, y + 7, pillX + 34, y + 25, enabled ? ClientConfig.accentColor() : 0xFF34445A);
        c.drawTextWithShadow(textRenderer, enabled ? "ON" : "OFF", pillX + 7, y + 12,
                enabled ? 0xFF0D1722 : TEXT);
    }

    private void drawAction(DrawContext c, int x, int y, int w, int h, String label, int mx, int my) {
        boolean hover = inside(mx, my, x, y, w, h);
        c.fill(x, y, x + w, y + h, hover ? 0xFF2A3C54 : 0xFF1C2A3D);
        c.fill(x, y, x + 2, y + h, ClientConfig.accentColor());
        c.drawTextWithShadow(textRenderer, label, x + 10, y + (h - 8) / 2, TEXT);
    }

    @Override
    public boolean mouseClicked(Click click, boolean doubled) {
        if (click.button() != 0) return super.mouseClicked(click, doubled);
        int mx = (int) click.x();
        int my = (int) click.y();
        int x = panelX();
        int y = panelY();
        int w = panelWidth();
        NovaConfig cfg = ClientConfig.CONFIG;

        for (int i = 0; i < 4; i++) {
            int tabY = y + 73 + i * 43;
            if (inside(mx, my, x + 8, tabY, 138, 32)) {
                page = i;
                return true;
            }
        }
        int cx = contentX();
        int cy = contentY();
        if (page == 0) {
            if (inside(mx, my, cx, cy + 132, 190, 34)) {
                client.setScreen(new NovaHudEditorScreen(this));
                return true;
            }
            if (inside(mx, my, cx + 202, cy + 132, 156, 34)) {
                ClientConfig.cycleTheme();
                return true;
            }
        } else if (page == 1) {
            int[] rows = {0, 36, 72, 108, 144, 180, 216};
            if (inside(mx, my, cx, cy + 264, 190, 32)) {
                client.setScreen(new NovaHudEditorScreen(this));
                return true;
            }
            for (int i = 0; i < rows.length; i++) {
                if (!inside(mx, my, cx, cy + rows[i], w - 194, 32)) continue;
                switch (i) {
                    case 0 -> cfg.showHud = !cfg.showHud;
                    case 1 -> cfg.showStats = !cfg.showStats;
                    case 2 -> cfg.showKeystrokes = !cfg.showKeystrokes;
                    case 3 -> cfg.showFps = !cfg.showFps;
                    case 4 -> cfg.showPing = !cfg.showPing;
                    case 5 -> cfg.showCps = !cfg.showCps;
                    case 6 -> cfg.showCoordinates = !cfg.showCoordinates;
                    default -> { }
                }
                ClientConfig.save();
                return true;
            }
        } else if (page == 2) {
            if (inside(mx, my, cx, cy, w - 194, 32)) {
                cfg.lowOverheadMode = !cfg.lowOverheadMode;
                ClientConfig.save();
                return true;
            }
            if (inside(mx, my, cx, cy + 40, w - 194, 32)) {
                cfg.showMemory = !cfg.showMemory;
                ClientConfig.save();
                return true;
            }
        } else {
            if (inside(mx, my, cx, cy + 54, 108, 34)) ClientConfig.loadProfile("Default");
            else if (inside(mx, my, cx + 118, cy + 54, 108, 34)) ClientConfig.loadProfile("Competitive");
            else if (inside(mx, my, cx + 236, cy + 54, 108, 34)) ClientConfig.loadProfile("Minimal");
            else if (inside(mx, my, cx, cy + 104, 180, 32)) ClientConfig.saveProfile(cfg.profile);
            return true;
        }
        return super.mouseClicked(click, doubled);
    }

    @Override
    public void close() {
        ClientConfig.save();
        client.setScreen(null);
    }

    @Override
    public boolean shouldPause() { return false; }
}
