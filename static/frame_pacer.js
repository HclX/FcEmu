// Fixed-timestep frame pacing.
//
// The NES runs at its own fixed frame rate, independent of the display's
// refresh rate. Driving one emulated frame per requestAnimationFrame ties
// game speed to the monitor (2.4x too fast at 144 Hz, uneven when the
// browser drops or delays vsyncs). Instead, accumulate real elapsed time
// and run however many emulated frames are due, then present the latest.

// Frame rates derived from the core's timing specs (src/core/region.rs):
// PPU clock / PPU dots per frame. NTSC skips one dot every other frame.
const NTSC_FRAME_MS = 1000 / ((1789773 * 3) / (262 * 341 - 0.5)); // ~60.0988 Hz
const PAL_FRAME_MS = 1000 / ((1661925 * 3.2) / (312 * 341)); // ~49.99 Hz

// Cap catch-up so a long stall (tab in background, debugger pause, GC)
// doesn't fast-forward the game; excess time is dropped instead.
const MAX_CATCHUP_FRAMES = 4;

export function frameDurationMs(region) {
    return region === 1 ? PAL_FRAME_MS : NTSC_FRAME_MS;
}

export class FramePacer {
    constructor() {
        this.reset();
    }

    // Call when the loop (re)starts so time spent paused isn't counted.
    reset() {
        this.lastTime = null;
        this.accumulator = 0;
    }

    // Returns how many emulated frames are due at timestamp `now` (ms).
    // The first call after reset() returns 1 so a resumed loop draws
    // immediately.
    due(now, frameMs) {
        if (this.lastTime === null) {
            this.lastTime = now;
            this.accumulator = 0;
            return 1;
        }
        this.accumulator += now - this.lastTime;
        this.lastTime = now;

        let frames = Math.floor(this.accumulator / frameMs);
        if (frames > MAX_CATCHUP_FRAMES) {
            frames = MAX_CATCHUP_FRAMES;
            this.accumulator = 0;
        } else {
            this.accumulator -= frames * frameMs;
        }
        return frames;
    }

    // Drop leftover time, e.g. when emulation could not advance
    // (netplay waiting on peer input) so it doesn't burst afterwards.
    clamp(frameMs) {
        this.accumulator = Math.min(this.accumulator, frameMs);
    }
}
