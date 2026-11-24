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
import net.minecraft.client.option.KeyBinding.Category;
import net.minecraft.client.util.InputUtil;
import net.minecraft.util.Identifier;
import org.lwjgl.glfw.GLFW;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

public final class NovaClient implements ClientModInitializer {
    public static final String MOD_ID = "nova-client";
    private static final Logger LOGGER = LoggerFactory.getLogger("NOVA Client");
    private static final String DEFAULT_MENU = "key.keyboard.right.shift";
    private static final String DEFAULT_EDITOR = "key.keyboard.right.control";
    private static final String DEFAULT_HUD_TOGGLE = "key.keyboard.f8";
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
        syncCustomBindings();

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

    private static InputUtil.Key configuredKey(String value, String fallback) {
        try {
            return InputUtil.fromTranslationKey(value == null || value.isBlank() ? fallback : value);
        } catch (RuntimeException exception) {
            LOGGER.warn("Invalid NOVA key binding '{}'; using {}", value, fallback);
            return InputUtil.fromTranslationKey(fallback);
        }
    }

    public static void syncCustomBindings() {
        if (menuKey == null || editorKey == null || hudToggleKey == null) return;
        menuKey.setBoundKey(configuredKey(ClientConfig.CONFIG.menuKey, DEFAULT_MENU));
        editorKey.setBoundKey(configuredKey(ClientConfig.CONFIG.editorKey, DEFAULT_EDITOR));
        hudToggleKey.setBoundKey(configuredKey(ClientConfig.CONFIG.hudToggleKey, DEFAULT_HUD_TOGGLE));
        KeyBinding.updateKeysByCode();
    }

    public static String updateCustomBinding(int index, InputUtil.Key key) {
        KeyBinding target = bindingAt(index);
        if (target == null || key == null) return "Unknown NOVA binding.";
        if (!key.equals(InputUtil.UNKNOWN_KEY)) {
            for (int other = 0; other < 3; other++) {
                if (other == index) continue;
                KeyBinding existing = bindingAt(other);
                if (existing != null && existing.getBoundKeyTranslationKey().equals(key.getTranslationKey())) {
                    return "That key is already assigned to another NOVA action.";
                }
            }
        }

        target.setBoundKey(key);
        switch (index) {
            case 0 -> ClientConfig.CONFIG.menuKey = key.getTranslationKey();
            case 1 -> ClientConfig.CONFIG.editorKey = key.getTranslationKey();
            case 2 -> ClientConfig.CONFIG.hudToggleKey = key.getTranslationKey();
            default -> { return "Unknown NOVA binding."; }
        }
        KeyBinding.updateKeysByCode();
        ClientConfig.save();
        return null;
    }

    public static void resetCustomBindings() {
        ClientConfig.CONFIG.menuKey = DEFAULT_MENU;
        ClientConfig.CONFIG.editorKey = DEFAULT_EDITOR;
        ClientConfig.CONFIG.hudToggleKey = DEFAULT_HUD_TOGGLE;
        syncCustomBindings();
        ClientConfig.save();
    }

    private static KeyBinding bindingAt(int index) {
        return switch (index) {
            case 0 -> menuKey;
            case 1 -> editorKey;
            case 2 -> hudToggleKey;
            default -> null;
        };
    }

    public static String getMenuBindingName() {
        return menuKey == null ? "Right Shift" : menuKey.getBoundKeyLocalizedText().getString();
    }

    public static String getEditorBindingName() {
        return editorKey == null ? "Right Ctrl" : editorKey.getBoundKeyLocalizedText().getString();
    }

    public static String getHudToggleBindingName() {
        return hudToggleKey == null ? "F8" : hudToggleKey.getBoundKeyLocalizedText().getString();
    }
}
