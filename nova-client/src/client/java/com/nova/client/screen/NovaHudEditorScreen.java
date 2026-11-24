package com.nova.client.screen;

import com.nova.client.config.ClientConfig;
import com.nova.client.config.ClientConfig.NovaConfig;
import com.nova.client.hud.HudRenderer;
import com.nova.client.telemetry.ClientTelemetry;
import net.minecraft.client.gui.Click;
import net.minecraft.client.gui.DrawContext;
import net.minecraft.client.gui.screen.Screen;
import net.minecraft.text.Text;

public final class NovaHudEditorScreen extends Screen {
    private final Screen parent;
    private int dragging;

    public NovaHudEditorScreen(Screen parent) {
        super(Text.literal("NOVA HUD Editor"));
        this.parent = parent;
    }

    @Override
    public void render(DrawContext context, int mouseX, int mouseY, float delta) {
        NovaConfig cfg = ClientConfig.CONFIG;
        context.fill(0, 0, width, height, 0xA309101A);
        context.fill(16, 16, width - 16, 72, 0xE5101927);
        context.fill(16, 16, width - 16, 18, ClientConfig.accentColor());
        context.drawTextWithShadow(textRenderer, "NOVA  /  HUD EDITOR", 30, 31, ClientConfig.accentColor());
        context.drawTextWithShadow(textRenderer, "DRAG PANELS TO POSITION THEM  ·  CHANGES SAVE WHEN RELEASED",
                30, 48, 0xFF9AABC0);
        HudRenderer.renderPreview(context, client, cfg, ClientTelemetry.snapshot());
        if (cfg.showStats) outline(context, cfg.statsX, cfg.statsY, HudRenderer.statsWidth(), HudRenderer.statsHeight(cfg));
        if (cfg.showKeystrokes) outline(context, cfg.keysX, cfg.keysY, HudRenderer.keysWidth(), HudRenderer.keysHeight());
        if (cfg.showArmor) outline(context, cfg.armorX, cfg.armorY, HudRenderer.armorWidth(), HudRenderer.armorHeight());
        if (cfg.showDirection) outline(context, cfg.directionX, cfg.directionY, HudRenderer.directionWidth(), HudRenderer.directionHeight());
        drawButton(context, 20, height - 48, 132, 30, "RESET LAYOUT", mouseX, mouseY);
        drawButton(context, width - 142, height - 48, 122, 30, "DONE", mouseX, mouseY);
        context.drawTextWithShadow(textRenderer, "Keep widgets clear of the crosshair and hotbar.",
                168, height - 38, 0xFF8798AE);
        super.render(context, mouseX, mouseY, delta);
    }

    private void outline(DrawContext c, int x, int y, int w, int h) {
        int color = ClientConfig.accentColor();
        c.fill(x - 1, y - 1, x + w + 1, y, color);
        c.fill(x - 1, y + h, x + w + 1, y + h + 1, color);
        c.fill(x - 1, y, x, y + h, color);
        c.fill(x + w, y, x + w + 1, y + h, color);
    }

    private void drawButton(DrawContext c, int x, int y, int w, int h, String label, int mx, int my) {
        boolean hover = mx >= x && mx <= x + w && my >= y && my <= y + h;
        c.fill(x, y, x + w, y + h, hover ? 0xFF2A3C54 : 0xFF1B293B);
        c.fill(x, y, x + 2, y + h, ClientConfig.accentColor());
        c.drawTextWithShadow(textRenderer, label, x + 10, y + 11, 0xFFEAF0F8);
    }

    @Override
    public boolean mouseClicked(Click click, boolean doubled) {
        if (click.button() != 0) return super.mouseClicked(click, doubled);
        int x = (int) click.x();
        int y = (int) click.y();
        if (x >= width - 142 && x <= width - 20 && y >= height - 48 && y <= height - 18) {
            close();
            return true;
        }
        if (x >= 20 && x <= 152 && y >= height - 48 && y <= height - 18) {
            ClientConfig.resetLayout();
            return true;
        }

        NovaConfig cfg = ClientConfig.CONFIG;
        if (cfg.showStats && x >= cfg.statsX - 2 && x <= cfg.statsX + HudRenderer.statsWidth() + 2
                && y >= cfg.statsY - 2 && y <= cfg.statsY + HudRenderer.statsHeight(cfg) + 2) {
            dragging = 1;
            return true;
        }
        if (cfg.showKeystrokes && x >= cfg.keysX - 2 && x <= cfg.keysX + HudRenderer.keysWidth() + 2
                && y >= cfg.keysY - 2 && y <= cfg.keysY + HudRenderer.keysHeight() + 2) {
            dragging = 2;
            return true;
        }
        if (cfg.showArmor && x >= cfg.armorX - 2 && x <= cfg.armorX + HudRenderer.armorWidth() + 2
                && y >= cfg.armorY - 2 && y <= cfg.armorY + HudRenderer.armorHeight() + 2) {
            dragging = 3;
            return true;
        }
        if (cfg.showDirection && x >= cfg.directionX - 2 && x <= cfg.directionX + HudRenderer.directionWidth() + 2
                && y >= cfg.directionY - 2 && y <= cfg.directionY + HudRenderer.directionHeight() + 2) {
            dragging = 4;
            return true;
        }
        return super.mouseClicked(click, doubled);
    }

    @Override
    public boolean mouseDragged(Click click, double offsetX, double offsetY) {
        if (dragging == 0 || click.button() != 0) return super.mouseDragged(click, offsetX, offsetY);
        NovaConfig cfg = ClientConfig.CONFIG;
        int dx = (int) Math.round(offsetX);
        int dy = (int) Math.round(offsetY);
        if (dragging == 1) {
            cfg.statsX = clamp(cfg.statsX + dx, 2, Math.max(2, width - HudRenderer.statsWidth() - 2));
            cfg.statsY = clamp(cfg.statsY + dy, 2, Math.max(2, height - HudRenderer.statsHeight(cfg) - 2));
        } else if (dragging == 2) {
            cfg.keysX = clamp(cfg.keysX + dx, 2, Math.max(2, width - HudRenderer.keysWidth() - 2));
            cfg.keysY = clamp(cfg.keysY + dy, 2, Math.max(2, height - HudRenderer.keysHeight() - 2));
        } else if (dragging == 3) {
            cfg.armorX = clamp(cfg.armorX + dx, 2, Math.max(2, width - HudRenderer.armorWidth() - 2));
            cfg.armorY = clamp(cfg.armorY + dy, 2, Math.max(2, height - HudRenderer.armorHeight() - 2));
        } else {
            cfg.directionX = clamp(cfg.directionX + dx, 2, Math.max(2, width - HudRenderer.directionWidth() - 2));
            cfg.directionY = clamp(cfg.directionY + dy, 2, Math.max(2, height - HudRenderer.directionHeight() - 2));
        }
        return true;
    }

    @Override
    public boolean mouseReleased(Click click) {
        if (dragging != 0) {
            dragging = 0;
            ClientConfig.save();
            return true;
        }
        return super.mouseReleased(click);
    }

    private int clamp(int value, int min, int max) {
        return Math.max(min, Math.min(max, value));
    }

    @Override
    public void close() {
        ClientConfig.save();
        client.setScreen(parent);
    }

    @Override
    public boolean shouldPause() { return false; }
}
