package com.nova.client.util;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

class HudMathTest {
    @Test
    void frameTimeEstimateIsDerivedFromFpsAndHandlesUnavailableValues() {
        assertEquals(0.0f, HudMath.frameTimeEstimateFromFps(0));
        assertEquals(0.0f, HudMath.frameTimeEstimateFromFps(-60));
        assertEquals(1000.0f / 60.0f, HudMath.frameTimeEstimateFromFps(60), 0.0001f);
        assertEquals(1000.0f / 240.0f, HudMath.frameTimeEstimateFromFps(240), 0.0001f);
    }

    @Test
    void armorDurabilityIsRoundedDownAndClamped() {
        assertEquals(100, HudMath.durabilityPercent(0, 0));
        assertEquals(100, HudMath.durabilityPercent(100, 0));
        assertEquals(75, HudMath.durabilityPercent(100, 25));
        assertEquals(0, HudMath.durabilityPercent(100, 100));
        assertEquals(0, HudMath.durabilityPercent(100, 140));
        assertEquals(100, HudMath.durabilityPercent(100, -20));
    }

    @Test
    void clickWindowIncludesBothBoundariesAndExcludesFutureEvents() {
        long[] timestamps = {0, 100, 200, 999, 1000, 1001, 1200};
        assertEquals(5, HudMath.countWithinWindow(timestamps, timestamps.length, 1000, 1000));
    }

    @Test
    void clickWindowBoundsRequestedArrayLengthAndWindow() {
        long[] timestamps = {900, 950, 990, 1000};
        assertEquals(2, HudMath.countWithinWindow(timestamps, 2, 1000, 100));
        assertEquals(0, HudMath.countWithinWindow(timestamps, -5, 1000, 100));
        assertEquals(4, HudMath.countWithinWindow(timestamps, 99, 1000, Long.MAX_VALUE));
    }

    @Test
    void clampRejectsAnInvertedRange() {
        assertEquals(0, HudMath.clamp(-4, 0, 10));
        assertEquals(10, HudMath.clamp(14, 0, 10));
        assertThrows(IllegalArgumentException.class, () -> HudMath.clamp(1, 2, 0));
    }
}
