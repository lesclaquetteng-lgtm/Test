package com.nova.client.hud;

import com.nova.client.config.ClientConfig;
import com.nova.client.config.ClientConfig.NovaConfig;
import com.nova.client.telemetry.ClientTelemetry;
import com.nova.client.telemetry.ClientTelemetry.Snapshot;
import net.minecraft.client.MinecraftClient;
import net.minecraft.client.gui.DrawContext;

public final class HudRenderer {
    private static final int PANEL = 0xD9101724;
    private static final int PANEL_EDGE = 0xFF2C3A50;
    private static final int PRIMARY = 0xFFF0F4FA;
    private static final int SECONDARY = 0xFF95A5BA;

    private HudRenderer() { }

    public static void render(DrawContext context) {
        MinecraftClient client = MinecraftClient.getInstance();
        NovaConfig config = ClientConfig.CONFIG;
        if (client.currentScreen != null || client.player == null || client.world == null || !config.showHud) return;
        renderPreview(context, client, config, ClientTelemetry.snapshot());
    }

    public static void renderPreview(DrawContext context, MinecraftClient client, NovaConfig config, Snapshot snapshot) {
        if (config.showStats) {
            drawScaled(context, config.statsX, config.statsY, config.statsScale,
                    () -> renderStats(context, client, config, snapshot, 0, 0));
        }
        if (config.showKeystrokes) {
            drawScaled(context, config.keysX, config.keysY, config.keysScale,
                    () -> renderKeystrokes(context, client, config, 0, 0));
        }
        if (config.showArmor) {
            drawScaled(context, config.armorX, config.armorY, config.armorScale,
                    () -> renderArmor(context, client, snapshot, 0, 0));
        }
        if (config.showDirection) {
            drawScaled(context, config.directionX, config.directionY, config.directionScale,
                    () -> renderDirection(context, client, snapshot, 0, 0));
        }
    }

    /**
     * Applies a local transform per widget. The hot render path allocates no objects and performs
     * no file, network, world-scan or entity-iteration work.
     */
    private static void drawScaled(DrawContext context, int x, int y, float scale, Runnable draw) {
        var matrices = context.getMatrices();
        matrices.pushMatrix();
        try {
            matrices.translate((float) x, (float) y);
            matrices.scale(scale, scale);
            draw.run();
        } finally {
            matrices.popMatrix();
        }
    }

    public static int scaleDimension(int dimension, float scale) {
        return Math.max(1, Math.round(dimension * scale));
    }

    public static int statsWidth(NovaConfig config) { return scaleDimension(statsWidth(), config.statsScale); }
    public static int statsHeightScaled(NovaConfig config) { return scaleDimension(statsHeight(config), config.statsScale); }
    public static int keysWidth(NovaConfig config) { return scaleDimension(keysWidth(), config.keysScale); }
    public static int keysHeight(NovaConfig config) { return scaleDimension(keysHeight(), config.keysScale); }
    public static int armorWidth(NovaConfig config) { return scaleDimension(armorWidth(), config.armorScale); }
    public static int armorHeight(NovaConfig config) { return scaleDimension(armorHeight(), config.armorScale); }
    public static int directionWidth(NovaConfig config) { return scaleDimension(directionWidth(), config.directionScale); }
    public static int directionHeight(NovaConfig config) { return scaleDimension(directionHeight(), config.directionScale); }

    public static int statsHeight(NovaConfig config) {
        int rows = 0;
        if (config.showFps) rows++;
        if (config.showPing) rows++;
        if (config.showCps) rows++;
        if (config.showCoordinates) rows++;
        if (config.showMemory) rows++;
        return 27 + Math.max(rows, 1) * 12 + 8;
    }

    public static int statsWidth() { return 168; }
    public static int keysWidth() { return 104; }
    public static int keysHeight() { return 82; }

    private static void renderStats(DrawContext context, MinecraftClient client, NovaConfig config,
                                    Snapshot snapshot, int x, int y) {
        int height = statsHeight(config);
        int accent = ClientConfig.accentColor();
        context.fill(x, y, x + statsWidth(), y + height, PANEL);
        context.fill(x, y, x + statsWidth(), y + 1, PANEL_EDGE);
        context.fill(x, y, x + 2, y + height, accent);
        context.fill(x + statsWidth() - 1, y, x + statsWidth(), y + height, 0x442C3A50);
        context.drawTextWithShadow(client.textRenderer, "NOVA", x + 10, y + 7, accent);
        context.drawTextWithShadow(client.textRenderer, "CLIENT TELEMETRY", x + 49, y + 8, SECONDARY);
        int rowY = y + 25;
        if (config.showFps) {
            String fps = snapshot.fps() > 0 ? snapshot.fps() + " FPS" : "-- FPS";
            if (config.showFrameTimeEstimate && snapshot.frameTimeMs() > 0.0f) {
                fps += " / " + String.format(java.util.Locale.ROOT, "%.1f ms", snapshot.frameTimeMs());
            }
            drawRow(context, client, x + 10, rowY, "FRAME", fps);
            rowY += 12;
        }
        if (config.showPing) {
            drawRow(context, client, x + 10, rowY, "PING", snapshot.ping() >= 0 ? snapshot.ping() + " ms" : "--");
            rowY += 12;
        }
        if (config.showCps) {
            drawRow(context, client, x + 10, rowY, "L-CPS", Integer.toString(snapshot.leftCps()));
            rowY += 12;
        }
        if (config.showCoordinates) {
            drawRow(context, client, x + 10, rowY, "XYZ", snapshot.x() + "  " + snapshot.y() + "  " + snapshot.z());
            rowY += 12;
        }
        if (config.showMemory) {
            String memory = snapshot.maxMemoryMb() > 0
                    ? snapshot.usedMemoryMb() + " / " + snapshot.maxMemoryMb() + " MB" : "--";
            drawRow(context, client, x + 10, rowY, "RAM", memory);
        }
    }

    private static void drawRow(DrawContext context, MinecraftClient client, int x, int y, String label, String value) {
        context.drawTextWithShadow(client.textRenderer, label, x, y, SECONDARY);
        context.drawTextWithShadow(client.textRenderer, value, x + 53, y, PRIMARY);
    }

    private static void renderKeystrokes(DrawContext context, MinecraftClient client, NovaConfig config, int x, int y) {
        int accent = ClientConfig.accentColor();
        context.fill(x, y, x + keysWidth(), y + keysHeight(), PANEL);
        context.fill(x, y, x + keysWidth(), y + 1, PANEL_EDGE);
        context.fill(x, y, x + 2, y + keysHeight(), accent);
        int key = 22;
        int gap = 4;
        int start = x + 15;
        renderKey(context, client, start + key + gap, y + 7, key, key, "W", client.options.forwardKey.isPressed(), accent);
        renderKey(context, client, start, y + 7 + key + gap, key, key, "A", client.options.leftKey.isPressed(), accent);
        renderKey(context, client, start + key + gap, y + 7 + key + gap, key, key, "S", client.options.backKey.isPressed(), accent);
        renderKey(context, client, start + (key + gap) * 2, y + 7 + key + gap, key, key, "D", client.options.rightKey.isPressed(), accent);
        int spaceY = y + 7 + (key + gap) * 2 + 2;
        context.fill(start, spaceY, start + 78, spaceY + 8, client.options.jumpKey.isPressed() ? accent : 0xFF263449);
        context.drawTextWithShadow(client.textRenderer, "SPACE", start + 30, spaceY,
                client.options.jumpKey.isPressed() ? 0xFF0E1824 : PRIMARY);
    }

    public static int armorWidth() { return 140; }
    public static int armorHeight() { return 52; }
    public static int directionWidth() { return 112; }
    public static int directionHeight() { return 30; }

    private static void renderArmor(DrawContext context, MinecraftClient client, Snapshot snapshot, int x, int y) {
        int accent = ClientConfig.accentColor();
        context.fill(x, y, x + armorWidth(), y + armorHeight(), PANEL);
        context.fill(x, y, x + armorWidth(), y + 1, PANEL_EDGE);
        context.fill(x, y, x + 2, y + armorHeight(), accent);
        context.drawTextWithShadow(client.textRenderer, "ARMOR", x + 9, y + 6, accent);
        drawArmorValue(context, client, "H", snapshot.helmetDurability(), x + 10, y + 23);
        drawArmorValue(context, client, "C", snapshot.chestDurability(), x + 74, y + 23);
        drawArmorValue(context, client, "L", snapshot.leggingsDurability(), x + 10, y + 38);
        drawArmorValue(context, client, "B", snapshot.bootsDurability(), x + 74, y + 38);
    }

    private static void drawArmorValue(DrawContext context, MinecraftClient client, String slot, int value, int x, int y) {
        int color = value < 0 ? SECONDARY : value > 60 ? 0xFF83E6C1 : value > 30 ? 0xFFFFCF70 : 0xFFFF7F88;
        String label = value < 0 ? "--" : value + "%";
        context.drawTextWithShadow(client.textRenderer, slot, x, y, SECONDARY);
        context.drawTextWithShadow(client.textRenderer, label, x + 15, y, color);
    }

    private static void renderDirection(DrawContext context, MinecraftClient client, Snapshot snapshot, int x, int y) {
        int accent = ClientConfig.accentColor();
        context.fill(x, y, x + directionWidth(), y + directionHeight(), PANEL);
        context.fill(x, y, x + directionWidth(), y + 1, PANEL_EDGE);
        context.fill(x, y, x + 2, y + directionHeight(), accent);
        context.drawTextWithShadow(client.textRenderer, "FACING", x + 9, y + 11, SECONDARY);
        context.drawTextWithShadow(client.textRenderer, snapshot.direction(), x + 63, y + 11, PRIMARY);
    }

    private static void renderKey(DrawContext context, MinecraftClient client, int x, int y,
                                  int width, int height, String label, boolean pressed, int accent) {
        context.fill(x, y, x + width, y + height, pressed ? accent : 0xFF263449);
        int textWidth = client.textRenderer.getWidth(label);
        context.drawTextWithShadow(client.textRenderer, label, x + (width - textWidth) / 2, y + 7,
                pressed ? 0xFF0E1824 : PRIMARY);
    }
}
