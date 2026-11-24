package com.nova.client.screen;

import com.nova.client.NovaClient;
import com.nova.client.config.ClientConfig;
import com.nova.client.config.ClientConfig.NovaConfig;
import net.minecraft.client.gui.Click;
import net.minecraft.client.gui.DrawContext;
import net.minecraft.client.gui.screen.Screen;
import net.minecraft.client.gui.widget.TextFieldWidget;
import net.minecraft.client.input.KeyInput;
import net.minecraft.text.Text;
import org.lwjgl.glfw.GLFW;

import java.util.ArrayList;
import java.util.List;
import java.util.function.BooleanSupplier;

public final class NovaClientScreen extends Screen {
    private static final int BG = 0xF00D1420;
    private static final int SIDEBAR = 0xFF0A111C;
    private static final int CARD = 0xFF151F2E;
    private static final int TEXT = 0xFFF0F4FA;
    private static final int MUTED = 0xFF8C9CB2;
    private static final int ROW_HEIGHT = 32;
    private static final int ROW_STEP = 38;

    private int page;
    private int hudScroll;
    private int rebinding = -1;
    private String keybindMessage = "Select a binding, then press a key.";
    private TextFieldWidget searchField;

    private record ToggleRow(String title, String description, BooleanSupplier enabled, Runnable toggle) { }

    public NovaClientScreen() {
        super(Text.literal("NOVA Client"));
    }

    @Override
    protected void init() {
        super.init();
        searchField = new TextFieldWidget(
                textRenderer, contentX(), contentY(), panelWidth() - 194, 22,
                Text.literal("Search NOVA settings"));
        searchField.setMaxLength(40);
        searchField.setPlaceholder(Text.literal("Search HUD settings…"));
        searchField.setDrawsBackground(false);
        searchField.setChangedListener(value -> hudScroll = 0);
        searchField.visible = page == 1;
        addDrawableChild(searchField);
    }

    private int panelWidth() { return Math.max(390, Math.min(780, width - 28)); }
    private int panelHeight() { return Math.max(360, Math.min(520, height - 28)); }
    private int panelX() { return (width - panelWidth()) / 2; }
    private int panelY() { return (height - panelHeight()) / 2; }
    private int contentX() { return panelX() + 176; }
    private int contentY() { return panelY() + 94; }
    private int contentWidth() { return panelWidth() - 194; }

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
        String[] tabs = {"Overview", "HUD & Modules", "Performance", "Profiles", "Keybinds"};

        if (searchField != null) {
            searchField.visible = page == 1;
            searchField.setX(contentX());
            searchField.setY(contentY());
            searchField.setWidth(contentWidth());
        }

        context.fill(0, 0, width, height, 0xA900050B);
        context.fill(x, y, x + w, y + h, BG);
        context.fill(x, y, x + w, y + 2, accent);
        context.fill(x, y + 58, x + 154, y + h - 1, SIDEBAR);
        context.fill(x + 154, y + 58, x + 155, y + h - 1, 0xFF263449);
        context.fill(x, y + h - 1, x + w, y + h, 0xFF263449);

        context.drawTextWithShadow(textRenderer, "NOVA", x + 20, y + 18, accent);
        context.drawTextWithShadow(textRenderer, "CLIENT", x + 73, y + 18, TEXT);
        context.drawTextWithShadow(textRenderer, "ONECONFIG-INSPIRED CONTROL", x + 20, y + 38, MUTED);
        context.drawTextWithShadow(textRenderer, NovaClient.getMenuBindingName() + "  /  MENU",
                x + w - 174, y + 23, MUTED);

        for (int i = 0; i < tabs.length; i++) {
            int ty = y + 72 + i * 39;
            if (i == page) {
                context.fill(x + 9, ty - 5, x + 145, ty + 25, 0xFF1D2A3D);
                context.fill(x + 9, ty - 5, x + 11, ty + 25, accent);
            }
            context.drawTextWithShadow(textRenderer, tabs[i], x + 20, ty + 5,
                    i == page ? TEXT : MUTED);
        }

        context.drawTextWithShadow(textRenderer, "NOVA CLIENT  /  " + tabs[page].toUpperCase(),
                contentX(), y + 66, accent);
        context.drawTextWithShadow(textRenderer, subtitle(), contentX(), y + 80, MUTED);

        if (page == 0) renderOverview(context, config, mouseX, mouseY);
        else if (page == 1) renderHudPage(context, config, mouseX, mouseY);
        else if (page == 2) renderPerformancePage(context, config, mouseX, mouseY);
        else if (page == 3) renderProfilesPage(context, config, mouseX, mouseY);
        else renderKeybindsPage(context, mouseX, mouseY);

        context.drawTextWithShadow(textRenderer, "NOVA CLIENT  ·  FABRIC 1.21.11",
                x + 18, y + h - 18, 0xFF60718A);
        context.drawTextWithShadow(textRenderer, "ESC  CLOSE", x + w - 88, y + h - 18, MUTED);
        super.render(context, mouseX, mouseY, delta);
    }

    private String subtitle() {
        return switch (page) {
            case 0 -> "A focused control surface for the game you play.";
            case 1 -> "Search every HUD control and filter the options instantly.";
            case 2 -> "Metrics are sampled away from the render path.";
            case 3 -> "Separate layouts for different play styles.";
            default -> "Rebind NOVA actions without leaving the game.";
        };
    }

    private void renderOverview(DrawContext c, NovaConfig cfg, int mx, int my) {
        int x = contentX();
        int y = contentY();
        int usable = contentWidth();
        c.fill(x, y, x + usable, y + 112, CARD);
        c.fill(x, y, x + 2, y + 112, ClientConfig.accentColor());
        c.drawTextWithShadow(textRenderer, "YOUR GAME. YOUR RULES.", x + 16, y + 16, TEXT);
        c.drawTextWithShadow(textRenderer, "One place for NOVA modules, HUD and shortcuts.",
                x + 16, y + 37, MUTED);
        c.drawTextWithShadow(textRenderer, "Lightweight telemetry. No world scans or render-time I/O.",
                x + 16, y + 57, MUTED);
        drawAction(c, x, y + 132, 190, 34, "OPEN HUD EDITOR", mx, my);
        drawAction(c, x + 202, y + 132, 156, 34, "THEME: " + cfg.theme, mx, my);
        c.drawTextWithShadow(textRenderer, "ACTIVE PROFILE", x, y + 195, MUTED);
        c.drawTextWithShadow(textRenderer, cfg.profile, x, y + 211, TEXT);
        c.drawTextWithShadow(textRenderer, "MENU", x + 185, y + 195, MUTED);
        c.drawTextWithShadow(textRenderer, NovaClient.getMenuBindingName(), x + 185, y + 211, TEXT);
        c.drawTextWithShadow(textRenderer, "HUD EDITOR", x + 185, y + 235, MUTED);
        c.drawTextWithShadow(textRenderer, NovaClient.getEditorBindingName(), x + 185, y + 251, TEXT);
    }

    private static final ToggleRow[] HUD_ROWS = {
            new ToggleRow("Master HUD", "Show or hide every NOVA overlay.",
                    () -> ClientConfig.CONFIG.showHud, () -> ClientConfig.CONFIG.showHud = !ClientConfig.CONFIG.showHud),
            new ToggleRow("Statistics panel", "FPS, ping, CPS, coordinates and heap usage.",
                    () -> ClientConfig.CONFIG.showStats, () -> ClientConfig.CONFIG.showStats = !ClientConfig.CONFIG.showStats),
            new ToggleRow("Keystrokes", "Live WASD and jump state.",
                    () -> ClientConfig.CONFIG.showKeystrokes, () -> ClientConfig.CONFIG.showKeystrokes = !ClientConfig.CONFIG.showKeystrokes),
            new ToggleRow("FPS counter", "Show the current client frame rate.",
                    () -> ClientConfig.CONFIG.showFps, () -> ClientConfig.CONFIG.showFps = !ClientConfig.CONFIG.showFps),
            new ToggleRow("Frame-time estimate", "Show the reciprocal FPS estimate in milliseconds.",
                    () -> ClientConfig.CONFIG.showFrameTimeEstimate,
                    () -> ClientConfig.CONFIG.showFrameTimeEstimate = !ClientConfig.CONFIG.showFrameTimeEstimate),
            new ToggleRow("Network latency", "Show ping reported by the player list.",
                    () -> ClientConfig.CONFIG.showPing, () -> ClientConfig.CONFIG.showPing = !ClientConfig.CONFIG.showPing),
            new ToggleRow("Left CPS", "Count left-click attack events over one second.",
                    () -> ClientConfig.CONFIG.showCps, () -> ClientConfig.CONFIG.showCps = !ClientConfig.CONFIG.showCps),
            new ToggleRow("Coordinates", "Show your current block position.",
                    () -> ClientConfig.CONFIG.showCoordinates, () -> ClientConfig.CONFIG.showCoordinates = !ClientConfig.CONFIG.showCoordinates),
            new ToggleRow("Memory indicator", "Show used and maximum Java heap memory.",
                    () -> ClientConfig.CONFIG.showMemory, () -> ClientConfig.CONFIG.showMemory = !ClientConfig.CONFIG.showMemory),
            new ToggleRow("Armor status", "Show remaining durability for equipped armor.",
                    () -> ClientConfig.CONFIG.showArmor, () -> ClientConfig.CONFIG.showArmor = !ClientConfig.CONFIG.showArmor),
            new ToggleRow("Facing direction", "Show the cardinal direction you are looking.",
                    () -> ClientConfig.CONFIG.showDirection, () -> ClientConfig.CONFIG.showDirection = !ClientConfig.CONFIG.showDirection)
    };

    private final List<ToggleRow> filteredRows = new ArrayList<>();

    private List<ToggleRow> filteredHudRows() {
        filteredRows.clear();
        String query = searchField == null ? "" : searchField.getText().trim().toLowerCase(java.util.Locale.ROOT);
        for (ToggleRow row : HUD_ROWS) {
            if (query.isEmpty()
                    || row.title().toLowerCase(java.util.Locale.ROOT).contains(query)
                    || row.description().toLowerCase(java.util.Locale.ROOT).contains(query)) {
                filteredRows.add(row);
            }
        }
        return filteredRows;
    }

    private int hudListTop() { return contentY() + 30; }
    private int hudListBottom() { return panelY() + panelHeight() - 28; }
    private int hudMaxScroll(int rowCount) {
        int totalHeight = Math.max(0, rowCount * ROW_STEP - 6);
        return Math.max(0, totalHeight - Math.max(0, hudListBottom() - hudListTop()));
    }

    private void renderHudPage(DrawContext c, NovaConfig cfg, int mx, int my) {
        int x = contentX();
        int usable = contentWidth();
        List<ToggleRow> rows = filteredHudRows(cfg);
        int top = hudListTop();
        int bottom = hudListBottom();
        hudScroll = Math.max(0, Math.min(hudScroll, hudMaxScroll(rows.size())));

        c.fill(x, contentY(), x + usable, contentY() + 22, 0xFF111B29);
        c.fill(x, contentY(), x + 2, contentY() + 22, ClientConfig.accentColor());
        c.drawTextWithShadow(textRenderer, "⌕", x + 10, contentY() + 7, ClientConfig.accentColor());
        if (rows.isEmpty()) {
            c.drawTextWithShadow(textRenderer, "No settings match this search.", x + 12, top + 12, MUTED);
            return;
        }

        c.enableScissor(x, top, x + usable, bottom);
        for (int i = 0; i < rows.size(); i++) {
            ToggleRow row = rows.get(i);
            int rowY = top + i * ROW_STEP - hudScroll;
            if (rowY + ROW_HEIGHT < top || rowY > bottom) continue;
            drawToggle(c, x, rowY, usable, row, mx, my);
        }
        c.disableScissor();

        if (hudMaxScroll(rows.size()) > 0) {
            int trackX = x + usable - 3;
            int trackHeight = bottom - top;
            int thumbHeight = Math.max(24, trackHeight * trackHeight / Math.max(trackHeight, rows.size() * ROW_STEP));
            int thumbTravel = trackHeight - thumbHeight;
            int thumbY = top + (int) Math.round((double) hudScroll / hudMaxScroll(rows.size()) * thumbTravel);
            c.fill(trackX, top, trackX + 2, bottom, 0xFF263449);
            c.fill(trackX, thumbY, trackX + 2, thumbY + thumbHeight, ClientConfig.accentColor());
        }
    }

    private void renderPerformancePage(DrawContext c, NovaConfig cfg, int mx, int my) {
        int x = contentX();
        int y = contentY();
        drawToggle(c, x, y, contentWidth(), new ToggleRow(
                "Frame-time estimate", "Derive 1000/FPS; not a GPU frame timer.",
                () -> cfg.showFrameTimeEstimate,
                () -> cfg.showFrameTimeEstimate = !cfg.showFrameTimeEstimate), mx, my);
        drawToggle(c, x, y + 40, contentWidth(), new ToggleRow(
                "Memory indicator", "Show Java heap usage in the HUD.",
                () -> cfg.showMemory, () -> cfg.showMemory = !cfg.showMemory), mx, my);
        int usable = contentWidth();
        c.fill(x, y + 104, x + usable, y + 214, CARD);
        c.drawTextWithShadow(textRenderer, "NOVA PERFORMANCE POLICY", x + 14, y + 118,
                ClientConfig.accentColor());
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

    private void renderKeybindsPage(DrawContext c, int mx, int my) {
        int x = contentX();
        int y = contentY();
        int usable = contentWidth();
        c.drawTextWithShadow(textRenderer, "NOVA ACTIONS", x, y, MUTED);
        drawKeybindRow(c, x, y + 16, usable, 0, "Open control panel",
                "Open the NOVA settings from anywhere in the game.", NovaClient.getMenuBindingName(), mx, my);
        drawKeybindRow(c, x, y + 78, usable, 1, "Open HUD editor",
                "Move your telemetry panels and adjust their layout.", NovaClient.getEditorBindingName(), mx, my);
        drawKeybindRow(c, x, y + 140, usable, 2, "Toggle all HUD overlays",
                "Quickly hide or show every NOVA HUD panel.", NovaClient.getHudToggleBindingName(), mx, my);
        drawAction(c, x, y + 208, 164, 32, "RESET DEFAULT BINDS", mx, my);
        c.drawTextWithShadow(textRenderer, "Click a binding, then press a key. Backspace clears it; Escape cancels.",
                x, y + 248, MUTED);
        if (!keybindMessage.isBlank()) {
            c.drawTextWithShadow(textRenderer, keybindMessage, x, y + 264,
                    keybindMessage.startsWith("Saved") ? ClientConfig.accentColor() : 0xFFFFA66F);
        }
    }

    private void drawKeybindRow(DrawContext c, int x, int y, int usable, int index,
                                String title, String description, String key, int mx, int my) {
        c.fill(x, y, x + usable, y + 56, CARD);
        c.fill(x, y, x + 2, y + 56, index == rebinding ? ClientConfig.accentColor() : 0xFF263449);
        c.drawTextWithShadow(textRenderer, title, x + 12, y + 10, TEXT);
        c.drawTextWithShadow(textRenderer, description, x + 12, y + 29, MUTED);
        int buttonX = x + usable - 142;
        boolean active = inside(mx, my, buttonX, y + 12, 130, 30);
        c.fill(buttonX, y + 12, buttonX + 130, y + 42,
                rebinding == index ? 0xFF32465E : active ? 0xFF2A3C54 : 0xFF1C2A3D);
        c.fill(buttonX, y + 12, buttonX + 2, y + 42, ClientConfig.accentColor());
        String label = rebinding == index ? "PRESS A KEY" : key;
        int textWidth = textRenderer.getWidth(label);
        c.drawTextWithShadow(textRenderer, label,
                buttonX + Math.max(8, (130 - textWidth) / 2), y + 23,
                rebinding == index ? ClientConfig.accentColor() : TEXT);
    }

    private void drawToggle(DrawContext c, int x, int y, int usable, ToggleRow row, int mx, int my) {
        boolean enabled = row.enabled().getAsBoolean();
        boolean hover = inside(mx, my, x, y, usable, ROW_HEIGHT);
        c.fill(x, y, x + usable, y + ROW_HEIGHT, hover ? 0xFF1C2A3E : CARD);
        c.drawTextWithShadow(textRenderer, row.title(), x + 10, y + 5, TEXT);
        c.drawTextWithShadow(textRenderer, row.description(), x + 10, y + 18, MUTED);
        int pillX = x + usable - 44;
        c.fill(pillX, y + 7, pillX + 34, y + 25,
                enabled ? ClientConfig.accentColor() : 0xFF34445A);
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

        for (int i = 0; i < 5; i++) {
            int tabY = y + 67 + i * 39;
            if (inside(mx, my, x + 8, tabY, 138, 32)) {
                page = i;
                hudScroll = 0;
                rebinding = -1;
                if (searchField != null) searchField.visible = page == 1;
                return true;
            }
        }

        int cx = contentX();
        int cy = contentY();
        int usable = contentWidth();
        NovaConfig cfg = ClientConfig.CONFIG;

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
            if (searchField != null && inside(mx, my, cx, cy, usable, 22)) {
                return super.mouseClicked(click, doubled);
            }
            int top = hudListTop();
            int bottom = hudListBottom();
            if (!inside(mx, my, cx, top, usable, bottom - top)) return super.mouseClicked(click, doubled);
            List<ToggleRow> rows = filteredHudRows(cfg);
            for (int i = 0; i < rows.size(); i++) {
                int rowY = top + i * ROW_STEP - hudScroll;
                if (inside(mx, my, cx, rowY, usable, ROW_HEIGHT)) {
                    rows.get(i).toggle().run();
                    ClientConfig.save();
                    return true;
                }
            }
        } else if (page == 2) {
            if (inside(mx, my, cx, cy, usable, ROW_HEIGHT)) {
                cfg.showFrameTimeEstimate = !cfg.showFrameTimeEstimate;
                ClientConfig.save();
                return true;
            }
            if (inside(mx, my, cx, cy + 40, usable, ROW_HEIGHT)) {
                cfg.showMemory = !cfg.showMemory;
                ClientConfig.save();
                return true;
            }
        } else if (page == 3) {
            if (inside(mx, my, cx, cy + 54, 108, 34)) ClientConfig.loadProfile("Default");
            else if (inside(mx, my, cx + 118, cy + 54, 108, 34)) ClientConfig.loadProfile("Competitive");
            else if (inside(mx, my, cx + 236, cy + 54, 108, 34)) ClientConfig.loadProfile("Minimal");
            else if (inside(mx, my, cx, cy + 104, 180, 32)) ClientConfig.saveProfile(cfg.profile);
            return true;
        } else {
            if (inside(mx, my, cx + usable - 142, cy + 28, 130, 30)) {
                rebinding = 0;
                keybindMessage = "Press a key for the control panel.";
                return true;
            }
            if (inside(mx, my, cx + usable - 142, cy + 90, 130, 30)) {
                rebinding = 1;
                keybindMessage = "Press a key for the HUD editor.";
                return true;
            }
            if (inside(mx, my, cx + usable - 142, cy + 152, 130, 30)) {
                rebinding = 2;
                keybindMessage = "Press a key for the HUD toggle.";
                return true;
            }
            if (inside(mx, my, cx, cy + 208, 164, 32)) {
                NovaClient.resetCustomBindings();
                rebinding = -1;
                keybindMessage = "Default NOVA shortcuts restored.";
                return true;
            }
        }

        return super.mouseClicked(click, doubled);
    }

    @Override
    public boolean keyPressed(KeyInput input) {
        if (rebinding >= 0) {
            if (input.key() == GLFW.GLFW_KEY_ESCAPE) {
                rebinding = -1;
                keybindMessage = "Binding cancelled.";
                return true;
            }
            net.minecraft.client.util.InputUtil.Key key = input.key() == GLFW.GLFW_KEY_BACKSPACE
                    ? net.minecraft.client.util.InputUtil.UNKNOWN_KEY
                    : net.minecraft.client.util.InputUtil.fromKeyCode(input);
            String error = NovaClient.updateCustomBinding(rebinding, key);
            rebinding = -1;
            keybindMessage = error == null ? "Saved. The new shortcut is active immediately." : error;
            return true;
        }
        if (input.key() == GLFW.GLFW_KEY_ESCAPE) {
            if (searchField != null && searchField.isFocused()) {
                searchField.setFocused(false);
                return true;
            }
            close();
            return true;
        }
        return super.keyPressed(input);
    }

    @Override
    public boolean mouseScrolled(double mouseX, double mouseY, double horizontalAmount, double verticalAmount) {
        if (page == 1 && inside(mouseX, mouseY, contentX(), hudListTop(),
                contentWidth(), hudListBottom() - hudListTop())) {
            List<ToggleRow> rows = filteredHudRows(ClientConfig.CONFIG);
            hudScroll = Math.max(0, Math.min(hudMaxScroll(rows.size()),
                    hudScroll - (int) Math.round(verticalAmount * 34)));
            return true;
        }
        return super.mouseScrolled(mouseX, mouseY, horizontalAmount, verticalAmount);
    }

    @Override
    public void close() {
        ClientConfig.save();
        client.setScreen(null);
    }

    @Override
    public boolean shouldPause() { return false; }
}
