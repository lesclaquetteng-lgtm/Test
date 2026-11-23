package com.nova.client.telemetry;

import net.minecraft.client.MinecraftClient;
import net.minecraft.client.network.PlayerListEntry;
import net.minecraft.util.math.MathHelper;

public final class ClientTelemetry {
    private static final long[] LEFT_CLICKS = new long[40];
    private static int clickCursor;
    private static int clickSize;
    private static int tickCounter;
    private static int usedMemoryMb;
    private static int maxMemoryMb;
    private static volatile Snapshot snapshot = new Snapshot(0, -1, 0, 0, 0, 0, 0.0f, 0, 0);

    private ClientTelemetry() { }

    public static void initialize() {
        maxMemoryMb = (int) Math.min(Integer.MAX_VALUE, Runtime.getRuntime().maxMemory() / (1024L * 1024L));
    }

    public static void recordLeftClick(int count) {
        long now = System.currentTimeMillis();
        int safeCount = Math.min(Math.max(count, 1), LEFT_CLICKS.length);
        for (int index = 0; index < safeCount; index++) {
            LEFT_CLICKS[clickCursor] = now;
            clickCursor = (clickCursor + 1) % LEFT_CLICKS.length;
            clickSize = Math.min(clickSize + 1, LEFT_CLICKS.length);
        }
    }

    public static void tick(MinecraftClient client) {
        tickCounter++;
        if (tickCounter % 5 != 0) return;

        int fps = client.getCurrentFps();
        int ping = -1;
        int x = 0;
        int y = 0;
        int z = 0;
        if (client.player != null) {
            x = MathHelper.floor(client.player.getX());
            y = MathHelper.floor(client.player.getY());
            z = MathHelper.floor(client.player.getZ());
            if (client.getNetworkHandler() != null) {
                PlayerListEntry entry = client.getNetworkHandler().getPlayerListEntry(client.player.getUuid());
                if (entry != null) ping = entry.getLatency();
            }
        }

        long now = System.currentTimeMillis();
        int cps = countRecentClicks(now);
        float frameTime = fps > 0 ? 1000.0f / fps : 0.0f;
        if (tickCounter % 20 == 0) {
            Runtime runtime = Runtime.getRuntime();
            usedMemoryMb = (int) ((runtime.totalMemory() - runtime.freeMemory()) / (1024L * 1024L));
            maxMemoryMb = (int) Math.min(Integer.MAX_VALUE, runtime.maxMemory() / (1024L * 1024L));
        }
        snapshot = new Snapshot(fps, ping, cps, x, y, z, frameTime, usedMemoryMb, maxMemoryMb);
    }

    private static int countRecentClicks(long now) {
        long cutoff = now - 1000L;
        int count = 0;
        for (int index = 0; index < clickSize; index++) {
            long timestamp = LEFT_CLICKS[index];
            if (timestamp >= cutoff && timestamp <= now) count++;
        }
        return count;
    }

    public static Snapshot snapshot() { return snapshot; }

    public record Snapshot(
            int fps, int ping, int leftCps, int x, int y, int z,
            float frameTimeMs, int usedMemoryMb, int maxMemoryMb
    ) { }
}
