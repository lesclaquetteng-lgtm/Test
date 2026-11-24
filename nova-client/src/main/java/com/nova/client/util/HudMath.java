package com.nova.client.util;

/**
 * Small, allocation-free calculations shared by the in-game HUD and its tests.
 * These helpers deliberately do not depend on Minecraft classes.
 */
public final class HudMath {
    private HudMath() { }

    /** Returns a reciprocal-FPS estimate, not a GPU or frame-pacing measurement. */
    public static float frameTimeEstimateFromFps(int fps) {
        return fps > 0 ? 1000.0f / fps : 0.0f;
    }

    /** Returns remaining durability as a clamped percentage. Items without durability return 100. */
    public static int durabilityPercent(int maxDamage, int damage) {
        if (maxDamage <= 0) return 100;
        long remaining = (long) maxDamage - damage;
        long percentage = remaining * 100L / maxDamage;
        return (int) Math.max(0L, Math.min(100L, percentage));
    }

    /** Counts event timestamps in the inclusive [now-window, now] interval. */
    public static int countWithinWindow(long[] timestamps, int size, long now, long windowMillis) {
        if (timestamps == null) {
            throw new IllegalArgumentException("timestamps must not be null");
        }
        int boundedSize = clamp(size, 0, timestamps.length);
        long window = Math.max(0L, windowMillis);
        long cutoff = now < Long.MIN_VALUE + window ? Long.MIN_VALUE : now - window;
        int count = 0;
        for (int index = 0; index < boundedSize; index++) {
            long timestamp = timestamps[index];
            if (timestamp >= cutoff && timestamp <= now) count++;
        }
        return count;
    }

    public static int clamp(int value, int min, int max) {
        if (min > max) throw new IllegalArgumentException("min must be less than or equal to max");
        return Math.max(min, Math.min(max, value));
    }
}
