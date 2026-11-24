package com.nova.client;

import com.nova.client.config.ClientConfig;
import com.nova.client.hud.HudRenderer;
import com.nova.client.screen.NovaClientScreen;
import com.nova.client.screen.NovaHudEditorScreen;
import com.nova.client.telemetry.ClientTelemetry;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.keybinding.v1.KeyBindingHelper;
import net.fabricmc.fabric.api.client.rendering.v1.hud.HudElementRegistry;
import net.fabricmc.fabric.api.event.client.player.ClientPreAttackCallback;
import net.minecraft.client.option.KeyBinding;
import net.minecraft.client.util.InputUtil;
import net.minecraft.client.option.KeyBinding.Category;
import net.minecraft.util.Identifier;
import org.lwjgl.glfw.GLFW;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

public final class NovaClient implements ClientModInitializer {
    public static final String MOD_ID = "nova-client";
    private static final Logger LOGGER = LoggerFactory.getLogger("NOVA Client");
    private static KeyBinding menuKey;
    private static KeyBinding editorKey;
    private static KeyBinding hudToggleKey;
    private static final Category NOVA_CATEGORY = Category.create(Identifier.of(MOD_ID, "client"));

    @Override
    public void onInitializeClient() {
        ClientConfig.initialize();
        ClientTelemetry.initialize();
        menuKey = KeyBindingHelper.registerKeyBinding(new KeyBinding(
                "key.nova-client.menu", InputUtil.Type.KEYSYM,
                GLFW.GLFW_KEY_RIGHT_SHIFT, NOVA_CATEGORY));
        editorKey = KeyBindingHelper.registerKeyBinding(new KeyBinding(
                "key.nova-client.editor", InputUtil.Type.KEYSYM,
                GLFW.GLFW_KEY_RIGHT_CONTROL, NOVA_CATEGORY));
        hudToggleKey = KeyBindingHelper.registerKeyBinding(new KeyBinding(
                "key.nova-client.toggle_hud", InputUtil.Type.KEYSYM,
                GLFW.GLFW_KEY_F8, NOVA_CATEGORY));

        ClientPreAttackCallback.EVENT.register((client, player, clickCount) -> {
            if (clickCount > 0) ClientTelemetry.recordLeftClick(clickCount);
            return false;
        });
        HudElementRegistry.addLast(
                Identifier.of(MOD_ID, "client_hud"),
                (context, tickCounter) -> HudRenderer.render(context));

        ClientTickEvents.END_CLIENT_TICK.register(client -> {
            ClientTelemetry.tick(client);
            while (menuKey.wasPressed()) {
                if (client.currentScreen == null) client.setScreen(new NovaClientScreen());
            }
            while (editorKey.wasPressed()) {
                if (client.currentScreen == null) client.setScreen(new NovaHudEditorScreen(null));
            }
            while (hudToggleKey.wasPressed()) {
                ClientConfig.CONFIG.showHud = !ClientConfig.CONFIG.showHud;
                ClientConfig.save();
            }
        });
        LOGGER.info("NOVA Client initialized for Minecraft 1.21.11");
    }
}
