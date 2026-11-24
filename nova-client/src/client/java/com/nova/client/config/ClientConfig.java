package com.nova.client.config;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonParseException;
import com.google.gson.annotations.SerializedName;
import com.nova.client.NovaClient;
import com.nova.client.util.HudMath;
import net.fabricmc.loader.api.FabricLoader;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.io.IOException;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;

public final class ClientConfig {
    private static final Logger LOGGER = LoggerFactory.getLogger("NOVA Client/Config");
    private static final Gson GSON = new GsonBuilder().setPrettyPrinting().create();
    public static volatile NovaConfig CONFIG = new NovaConfig();

    private ClientConfig() { }

    public static void initialize() {
        load();
        ensureBuiltInProfiles();
    }

    public static synchronized void load() {
        Path file = configDirectory().resolve("settings.json");
        if (!Files.isRegularFile(file)) {
            CONFIG = new NovaConfig();
            save();
            return;
        }
        try {
            NovaConfig loaded = GSON.fromJson(Files.readString(file), NovaConfig.class);
            CONFIG = loaded == null ? new NovaConfig() : loaded;
            normalize(CONFIG);
        } catch (IOException | JsonParseException exception) {
            LOGGER.warn("Unable to read NOVA Client settings. Defaults will be used.", exception);
            CONFIG = new NovaConfig();
            save();
        }
    }

    public static synchronized void save() {
        normalize(CONFIG);
        writeJson(configDirectory().resolve("settings.json"), CONFIG);
        writeJson(profilePath(CONFIG.profile), CONFIG);
    }

    public static synchronized boolean saveProfile(String name) {
        if (!isValidProfileName(name)) return false;
        CONFIG.profile = name;
        save();
        return true;
    }

    public static synchronized boolean loadProfile(String name) {
        if (!isValidProfileName(name)) return false;
        Path file = profilePath(name);
        if (!Files.isRegularFile(file)) return false;
        try {
            NovaConfig loaded = GSON.fromJson(Files.readString(file), NovaConfig.class);
            if (loaded == null) return false;
            normalize(loaded);
            loaded.profile = name;
            CONFIG = loaded;
            save();
            NovaClient.syncCustomBindings();
            return true;
        } catch (IOException | JsonParseException exception) {
            LOGGER.warn("Unable to load NOVA Client profile '{}'.", name, exception);
            return false;
        }
    }

    public static synchronized void ensureBuiltInProfiles() {
        createProfileIfMissing("Default", new NovaConfig());
        NovaConfig competitive = new NovaConfig();
        competitive.theme = "AMETHYST";
        competitive.showCoordinates = false;
        competitive.showMemory = false;
        createProfileIfMissing("Competitive", competitive);
        NovaConfig minimal = new NovaConfig();
        minimal.showKeystrokes = false;
        minimal.showPing = false;
        minimal.showCps = false;
        minimal.showCoordinates = false;
        createProfileIfMissing("Minimal", minimal);
    }

    private static void createProfileIfMissing(String name, NovaConfig preset) {
        Path file = profilePath(name);
        if (Files.isRegularFile(file)) return;
        preset.profile = name;
        writeJson(file, preset);
    }

    public static boolean isValidProfileName(String name) {
        return name != null && name.matches("[A-Za-z0-9_-]{1,24}");
    }

    public static void resetLayout() {
        CONFIG.statsX = 12;
        CONFIG.statsY = 12;
        CONFIG.keysX = 12;
        CONFIG.keysY = 112;
        CONFIG.armorX = 12;
        CONFIG.armorY = 212;
        CONFIG.directionX = 12;
        CONFIG.directionY = 280;
        CONFIG.statsScale = 1.0f;
        CONFIG.keysScale = 1.0f;
        CONFIG.armorScale = 1.0f;
        CONFIG.directionScale = 1.0f;
        save();
    }

    public static void cycleTheme() {
        CONFIG.theme = switch (CONFIG.theme) {
            case "AURORA" -> "AMETHYST";
            case "AMETHYST" -> "EMBER";
            default -> "AURORA";
        };
        save();
    }

    public static int accentColor() {
        return switch (CONFIG.theme) {
            case "AMETHYST" -> 0xFFB69CFF;
            case "EMBER" -> 0xFFFFA66F;
            default -> 0xFF83E6C1;
        };
    }

    private static void normalize(NovaConfig config) {
        if (config.theme == null || !config.theme.matches("AURORA|AMETHYST|EMBER")) config.theme = "AURORA";
        if (!isValidProfileName(config.profile)) config.profile = "Default";
        if (config.menuKey == null || config.menuKey.isBlank()) config.menuKey = "key.keyboard.right.shift";
        if (config.editorKey == null || config.editorKey.isBlank()) config.editorKey = "key.keyboard.right.control";
        if (config.hudToggleKey == null || config.hudToggleKey.isBlank()) config.hudToggleKey = "key.keyboard.f8";
        config.statsX = clamp(config.statsX, 0, 10000);
        config.statsY = clamp(config.statsY, 0, 10000);
        config.keysX = clamp(config.keysX, 0, 10000);
        config.keysY = clamp(config.keysY, 0, 10000);
        config.armorX = clamp(config.armorX, 0, 10000);
        config.armorY = clamp(config.armorY, 0, 10000);
        config.directionX = clamp(config.directionX, 0, 10000);
        config.directionY = clamp(config.directionY, 0, 10000);
        config.statsScale = HudMath.normalizeWidgetScale(config.statsScale);
        config.keysScale = HudMath.normalizeWidgetScale(config.keysScale);
        config.armorScale = HudMath.normalizeWidgetScale(config.armorScale);
        config.directionScale = HudMath.normalizeWidgetScale(config.directionScale);
    }

    private static int clamp(int value, int min, int max) {
        return Math.max(min, Math.min(max, value));
    }

    private static Path configDirectory() {
        Path directory = FabricLoader.getInstance().getConfigDir().resolve("nova-client");
        try {
            Files.createDirectories(directory);
        } catch (IOException exception) {
            LOGGER.error("Unable to create NOVA Client configuration directory.", exception);
        }
        return directory;
    }

    private static Path profilePath(String name) {
        Path profiles = configDirectory().resolve("profiles");
        try {
            Files.createDirectories(profiles);
        } catch (IOException exception) {
            LOGGER.error("Unable to create NOVA Client profile directory.", exception);
        }
        String safe = isValidProfileName(name) ? name : "Default";
        return profiles.resolve(safe + ".json");
    }

    private static void writeJson(Path destination, NovaConfig config) {
        try {
            Files.createDirectories(destination.getParent());
            Path temporary = destination.resolveSibling(destination.getFileName() + ".tmp");
            Files.writeString(temporary, GSON.toJson(config));
            try {
                Files.move(temporary, destination, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
            } catch (AtomicMoveNotSupportedException exception) {
                Files.move(temporary, destination, StandardCopyOption.REPLACE_EXISTING);
            }
        } catch (IOException exception) {
            LOGGER.error("Unable to save NOVA Client settings to {}.", destination, exception);
        }
    }

    public static final class NovaConfig {
        public boolean showHud = true;
        public boolean showStats = true;
        public boolean showKeystrokes = true;
        public boolean showFps = true;
        public boolean showPing = true;
        public boolean showCps = true;
        public boolean showCoordinates = true;
        public boolean showMemory = false;
        @SerializedName(value = "showFrameTimeEstimate", alternate = {"lowOverheadMode"})
        public boolean showFrameTimeEstimate = true;
        public int statsX = 12;
        public int statsY = 12;
        public int keysX = 12;
        public int keysY = 112;
        public boolean showArmor = false;
        public boolean showDirection = false;
        public int armorX = 12;
        public int armorY = 212;
        public int directionX = 12;
        public int directionY = 280;
        public String theme = "AURORA";
        public String profile = "Default";
        public String menuKey = "key.keyboard.right.shift";
        public String editorKey = "key.keyboard.right.control";
        public String hudToggleKey = "key.keyboard.f8";
        public float statsScale = 1.0f;
        public float keysScale = 1.0f;
        public float armorScale = 1.0f;
        public float directionScale = 1.0f;
    }
}
