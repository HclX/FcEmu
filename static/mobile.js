import init, { WasmEmulator } from "./pkg/fce_core.js";

// Emulator states
let wasm_exports = null;
let emulator = null;
let isRunning = false;
let localFrameIndex = 0;

// Audio context state
let audioCtx = null;
let gainNode = null;
let nextPlayTime = 0;
let isMuted = false;

// ROM storage state
let currentRomHash = null;
let currentRomName = null;
let userRomsCache = {};
let autoSaveIntervalId = null;
let lastSavedSram = null;

// Built-in Homebrew / Default ROMs
const DEFAULT_ROMS = {
    "novathesquirrel": { name: "Nova the Squirrel (Platformer)", path: "./public/roms/novathesquirrel.nes" },
    "flappybird": { name: "Flappy Bird (Arcade)", path: "./public/roms/flappy-bird.nes" },
    "lizard": { name: "Lizard (Adventure Demo)", path: "./public/roms/lizard_demo.nes" }
};

// IndexedDB Configurations
const DB_NAME = "FcEmuDB";
const DB_VERSION = 3;
const STORE_NAME = "sram_saves";
const ROM_STORE_NAME = "user_roms";

// Controller bitmasks
const NES_BUTTON_A      = 0x01;
const NES_BUTTON_B      = 0x02;
const NES_BUTTON_SELECT = 0x04;
const NES_BUTTON_START  = 0x08;
const NES_BUTTON_UP     = 0x10;
const NES_BUTTON_DOWN   = 0x20;
const NES_BUTTON_LEFT   = 0x40;
const NES_BUTTON_RIGHT  = 0x80;

// Multi-touch states
let activeTouchMap = {}; // touch.identifier -> bitmask
let mobileControllerState = 0;

// UI Elements
const canvas = document.getElementById("emulator-canvas");
const ctx = canvas.getContext("2d");
const touchOverlay = document.getElementById("touch-overlay");
const romSelect = document.getElementById("mobile-rom-select");
const btnSelect = document.getElementById("btn-mobile-select");
const btnStart = document.getElementById("btn-mobile-start");
const btnReset = document.getElementById("btn-mobile-reset");
const btnMute = document.getElementById("btn-mobile-mute");
const btnFullscreen = document.getElementById("btn-mobile-fullscreen");
const virtualDpad = document.getElementById("virtual-dpad");

// IndexedDB Helpers
function openDB() {
    return new Promise((resolve, reject) => {
        const request = indexedDB.open(DB_NAME, DB_VERSION);
        request.onupgradeneeded = (event) => {
            const db = event.target.result;
            if (!db.objectStoreNames.contains(STORE_NAME)) {
                db.createObjectStore(STORE_NAME, { keyPath: "romHash" });
            }
            if (!db.objectStoreNames.contains(ROM_STORE_NAME)) {
                db.createObjectStore(ROM_STORE_NAME, { keyPath: "romHash" });
            }
        };
        request.onsuccess = (event) => resolve(event.target.result);
        request.onerror = (event) => reject(event.target.error);
    });
}

async function loadAllROMsFromDB() {
    const db = await openDB();
    return new Promise((resolve, reject) => {
        const transaction = db.transaction(ROM_STORE_NAME, "readonly");
        const store = transaction.objectStore(ROM_STORE_NAME);
        const request = store.getAll();
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
    });
}

async function loadSRAMFromDB(romHash) {
    const db = await openDB();
    return new Promise((resolve, reject) => {
        const transaction = db.transaction(STORE_NAME, "readonly");
        const store = transaction.objectStore(STORE_NAME);
        const request = store.get(romHash);
        request.onsuccess = () => resolve(request.result ? request.result.sramData : null);
        request.onerror = () => reject(request.error);
    });
}

async function saveSRAMToDB(romHash, romName, sramData) {
    const db = await openDB();
    return new Promise((resolve, reject) => {
        const transaction = db.transaction(STORE_NAME, "readwrite");
        const store = transaction.objectStore(STORE_NAME);
        const request = store.put({
            romHash,
            romName,
            sramData,
            timestamp: Date.now()
        });
        request.onsuccess = () => resolve();
        request.onerror = () => reject(request.error);
    });
}

// Compute SHA-256 of ROM
async function computeROMHash(arrayBuffer) {
    const hashBuffer = await crypto.subtle.digest("SHA-256", arrayBuffer);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    return hashArray.map(b => b.toString(16).padStart(2, "0")).join("");
}

// Validate iNES Header
function validateNESHeader(arrayBuffer) {
    if (arrayBuffer.byteLength < 16) return false;
    const bytes = new Uint8Array(arrayBuffer);
    return bytes[0] === 0x4E && bytes[1] === 0x45 && bytes[2] === 0x53 && bytes[3] === 0x1A; // 'NES' \x1a
}

// Haptic Feedback
const triggerHaptic = () => {
    if (navigator.vibrate) {
        navigator.vibrate(10);
    }
};

// Orientation Lock to Landscape
const requestLandscapeLock = () => {
    if (screen.orientation && screen.orientation.lock) {
        screen.orientation.lock("landscape").catch(err => {
            console.warn("Screen orientation lock rejected:", err.message);
        });
    }
};

// Setup Audio context
async function startAudio() {
    if (!audioCtx) {
        audioCtx = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 44100 });
        gainNode = audioCtx.createGain();
        gainNode.connect(audioCtx.destination);
        gainNode.gain.setValueAtTime(isMuted ? 0 : 1, audioCtx.currentTime);
        nextPlayTime = audioCtx.currentTime;
    }
    if (audioCtx.state === "suspended") {
        await audioCtx.resume();
    }
}

// Main emulator loop
function loop() {
    if (!isRunning || !emulator) return;

    // Merged inputs mapping
    let consolidatedMask = 0;
    for (let id in activeTouchMap) {
        consolidatedMask |= activeTouchMap[id];
    }
    mobileControllerState = consolidatedMask;
    window.controllerState = mobileControllerState;
    emulator.write_controller(mobileControllerState);
    emulator.write_controller2(0); // Mobile is strictly Singleplayer

    emulator.step_frame();
    localFrameIndex++;
    window.localFrameIndex = localFrameIndex;

    // Render frame buffer (zero-copy)
    const framePtr = emulator.frame_buffer_ptr();
    const rgbaBuffer = new Uint8ClampedArray(wasm_exports.memory.buffer, framePtr, 256 * 240 * 4);
    const frameImgData = new ImageData(rgbaBuffer, 256, 240);
    ctx.putImageData(frameImgData, 0, 0);

    // Handle audio sampling
    if (audioCtx && audioCtx.state !== "suspended" && !isMuted) {
        const samplePtr = emulator.sample_buffer_ptr();
        const sampleLen = emulator.sample_buffer_len();
        
        if (sampleLen > 0) {
            const sampleBuffer = new Float32Array(wasm_exports.memory.buffer, samplePtr, sampleLen);
            const audioBuffer = audioCtx.createBuffer(1, sampleLen, 44100);
            audioBuffer.getChannelData(0).set(sampleBuffer);

            const source = audioCtx.createBufferSource();
            source.buffer = audioBuffer;
            source.connect(gainNode);

            const duration = sampleLen / 44100;
            let playTime = nextPlayTime;

            if (playTime < audioCtx.currentTime) {
                playTime = audioCtx.currentTime + 0.05;
            }
            if (playTime - audioCtx.currentTime > 0.1) {
                playTime = audioCtx.currentTime + 0.02;
            }

            source.start(playTime);
            nextPlayTime = playTime + duration;
            emulator.clear_sample_buffer();
        }
    }

    requestAnimationFrame(loop);
}

// Load ROM Buffer
async function handleROMBuffer(arrayBuffer, romName = "unknown_rom.nes") {
    if (!emulator) return;

    if (!validateNESHeader(arrayBuffer)) {
        alert("Invalid ROM format!");
        return;
    }

    const romHash = await computeROMHash(arrayBuffer);
    console.log(`[FcEmu Mobile] Loading ROM: ${romName} (${romHash})`);

    if (autoSaveIntervalId) {
        clearInterval(autoSaveIntervalId);
        autoSaveIntervalId = null;
    }

    currentRomHash = romHash;
    currentRomName = romName;
    lastSavedSram = null;

    const uint8Array = new Uint8Array(arrayBuffer);
    const success = emulator.load_rom(uint8Array);

    if (success) {
        // PAL/NTSC Region Auto-detection
        const region = emulator.get_cartridge_detected_region();
        emulator.set_region(region);
        emulator.reset();

        console.log(`[FcEmu Mobile] Region automatically set to: ${region === 0 ? 'NTSC' : 'PAL'}`);

        localFrameIndex = 0;

        // Auto-Restore SRAM
        if (emulator.has_battery_backed_sram()) {
            try {
                const savedSram = await loadSRAMFromDB(romHash);
                if (savedSram) {
                    const restoreSuccess = emulator.set_sram(savedSram);
                    if (restoreSuccess) {
                        lastSavedSram = savedSram.slice();
                        console.log("[FcEmu Mobile] SRAM restored.");
                    }
                } else {
                    const freshSram = emulator.get_sram();
                    if (freshSram) lastSavedSram = freshSram.slice();
                }
            } catch (err) {
                console.error("[FcEmu Mobile] Failed to restore SRAM:", err);
            }
            autoSaveIntervalId = setInterval(triggerAutoSave, 5000);
        }

        if (!isRunning) {
            isRunning = true;
            requestAnimationFrame(loop);
        }
    } else {
        alert("Failed to load ROM");
    }
}

async function triggerAutoSave() {
    if (!emulator || !currentRomHash) return;
    if (!emulator.has_battery_backed_sram()) return;

    const currentSram = emulator.get_sram();
    if (!currentSram) return;

    if (!isSramDirty(currentSram, lastSavedSram)) return;

    try {
        await saveSRAMToDB(currentRomHash, currentRomName, currentSram);
        lastSavedSram = currentSram.slice();
        console.log("[FcEmu Mobile] Auto-saved SRAM.");
    } catch (err) {
        console.error("[FcEmu Mobile] Auto-save failed:", err);
    }
}

function isSramDirty(current, cached) {
    if (!cached) return true;
    if (current.length !== cached.length) return true;
    for (let i = 0; i < current.length; i++) {
        if (current[i] !== cached[i]) return true;
    }
    return false;
}

// Populates the Mobile ROM Library Selector
async function refreshRomLibrary() {
    if (!romSelect) return;
    romSelect.innerHTML = '<option value="">📚 Load ROM</option>';

    for (const key in DEFAULT_ROMS) {
        const opt = document.createElement("option");
        opt.value = key;
        opt.textContent = `⚡ ${DEFAULT_ROMS[key].name}`;
        romSelect.appendChild(opt);
    }

    try {
        const userRoms = await loadAllROMsFromDB();
        userRoms.forEach(rom => {
            const opt = document.createElement("option");
            opt.value = `user-${rom.romHash}`;
            opt.textContent = `💾 ${rom.romName}`;
            romSelect.appendChild(opt);

            userRomsCache[rom.romHash] = {
                name: rom.romName,
                data: rom.romData
            };
        });
    } catch (err) {
        console.error("[FcEmu Mobile] Failed to load ROMs:", err);
    }
}

// Handle D-Pad Angular Sliding logic
const handleDpadTouch = (touch) => {
    const dpadRect = virtualDpad.getBoundingClientRect();
    const centerX = dpadRect.left + dpadRect.width / 2;
    const centerY = dpadRect.top + dpadRect.height / 2;
    const dx = touch.clientX - centerX;
    const dy = touch.clientY - centerY;
    const distance = Math.sqrt(dx * dx + dy * dy);
    const maxRadius = dpadRect.width / 2;

    // Deadzone checking
    if (distance < maxRadius * 0.15) {
        activeTouchMap[touch.identifier] = 0;
        return;
    }

    let angle = Math.atan2(dy, dx) * (180 / Math.PI);
    let activeVectorMask = 0;

    // 45-degree segments
    if (angle >= -22.5 && angle < 22.5) {
        activeVectorMask = NES_BUTTON_RIGHT;
    } else if (angle >= 22.5 && angle < 67.5) {
        activeVectorMask = NES_BUTTON_DOWN | NES_BUTTON_RIGHT;
    } else if (angle >= 67.5 && angle < 112.5) {
        activeVectorMask = NES_BUTTON_DOWN;
    } else if (angle >= 112.5 && angle < 157.5) {
        activeVectorMask = NES_BUTTON_DOWN | NES_BUTTON_LEFT;
    } else if (angle >= 157.5 || angle < -157.5) {
        activeVectorMask = NES_BUTTON_LEFT;
    } else if (angle >= -157.5 && angle < -112.5) {
        activeVectorMask = NES_BUTTON_UP | NES_BUTTON_LEFT;
    } else if (angle >= -112.5 && angle < -67.5) {
        activeVectorMask = NES_BUTTON_UP;
    } else if (angle >= -67.5 && angle < -22.5) {
        activeVectorMask = NES_BUTTON_UP | NES_BUTTON_RIGHT;
    }

    activeTouchMap[touch.identifier] = activeVectorMask;
};

// Touch Listeners Setup
function setupTouchControls() {
    // Prevent default double-tap, scrolling, and gesture interruptions globally on the overlays
    const overlays = document.querySelectorAll(".interactive");
    overlays.forEach(el => {
        el.addEventListener("touchstart", (e) => {
            e.preventDefault();
        }, { passive: false });
        el.addEventListener("touchmove", (e) => {
            e.preventDefault();
        }, { passive: false });
    });

    // D-pad Events
    virtualDpad.addEventListener("touchstart", (e) => {
        triggerHaptic();
        Array.from(e.changedTouches).forEach(touch => {
            handleDpadTouch(touch);
        });
    });

    virtualDpad.addEventListener("touchmove", (e) => {
        Array.from(e.changedTouches).forEach(touch => {
            handleDpadTouch(touch);
        });
    });

    virtualDpad.addEventListener("touchend", (e) => {
        Array.from(e.changedTouches).forEach(touch => {
            delete activeTouchMap[touch.identifier];
        });
    });

    virtualDpad.addEventListener("touchcancel", (e) => {
        Array.from(e.changedTouches).forEach(touch => {
            delete activeTouchMap[touch.identifier];
        });
    });

    // Action Buttons setup
    const setupActionButton = (btnId, bitmask) => {
        const el = document.getElementById(btnId);
        if (!el) return;

        el.addEventListener("touchstart", (e) => {
            triggerHaptic();
            Array.from(e.changedTouches).forEach(touch => {
                activeTouchMap[touch.identifier] = bitmask;
            });
        });

        el.addEventListener("touchend", (e) => {
            Array.from(e.changedTouches).forEach(touch => {
                delete activeTouchMap[touch.identifier];
            });
        });

        el.addEventListener("touchcancel", (e) => {
            Array.from(e.changedTouches).forEach(touch => {
                delete activeTouchMap[touch.identifier];
            });
        });
    };

    setupActionButton("btn-action-a", NES_BUTTON_A);
    setupActionButton("btn-action-b", NES_BUTTON_B);

    // System Buttons
    const setupSystemButton = (btnId, bitmask) => {
        const el = document.getElementById(btnId);
        if (!el) return;

        el.addEventListener("touchstart", (e) => {
            triggerHaptic();
            Array.from(e.changedTouches).forEach(touch => {
                activeTouchMap[touch.identifier] = bitmask;
            });
        });

        el.addEventListener("touchend", (e) => {
            Array.from(e.changedTouches).forEach(touch => {
                delete activeTouchMap[touch.identifier];
            });
        });
    };

    setupSystemButton("btn-mobile-select", NES_BUTTON_SELECT);
    setupSystemButton("btn-mobile-start", NES_BUTTON_START);

    // Core Controls
    btnReset.addEventListener("touchstart", () => {
        triggerHaptic();
        if (emulator) emulator.reset();
    });

    btnMute.addEventListener("touchstart", () => {
        triggerHaptic();
        isMuted = !isMuted;
        if (gainNode) {
            gainNode.gain.setValueAtTime(isMuted ? 0 : 1, audioCtx.currentTime);
        }
        btnMute.textContent = isMuted ? "UNMUTE" : "MUTE";
    });

    btnFullscreen.addEventListener("touchstart", () => {
        triggerHaptic();
        const docEl = document.documentElement;
        if (!document.fullscreenElement) {
            docEl.requestFullscreen().catch(err => {
                console.error(`Fullscreen error: ${err.message}`);
            });
        } else {
            document.exitFullscreen();
        }
    });
}

// Handle ROM library loading
romSelect.addEventListener("change", async () => {
    const val = romSelect.value;
    if (!val) return;

    if (val.startsWith("user-")) {
        const hash = val.substring(5);
        const cached = userRomsCache[hash];
        if (cached) {
            await handleROMBuffer(cached.data.slice(0), cached.name);
        }
    } else {
        // Default ROMs
        const details = DEFAULT_ROMS[val];
        if (details) {
            try {
                const response = await fetch(details.path);
                const buf = await response.arrayBuffer();
                await handleROMBuffer(buf, details.name);
            } catch (err) {
                alert(`Failed to fetch default ROM: ${err.message}`);
            }
        }
    }
    // Fade HUD back out
    document.getElementById("mobile-hud").classList.remove("active");
});

// First-touch gesture initializations (Audio Context + Screen Lock)
touchOverlay.addEventListener("touchstart", async (e) => {
    e.preventDefault();
    triggerHaptic();
    requestLandscapeLock();
    await startAudio();
    touchOverlay.classList.add("hidden");

    // Initialize core WASM
    try {
        wasm_exports = await init();
        emulator = new WasmEmulator();
        await refreshRomLibrary();
        setupTouchControls();
        console.log("[FcEmu Mobile] Wasm Module Loaded!");

        // Check URL search params for instant loading
        const urlParams = new URLSearchParams(window.location.search);
        const romParam = urlParams.get("rom");
        if (romParam && DEFAULT_ROMS[romParam]) {
            romSelect.value = romParam;
            romSelect.dispatchEvent(new Event("change"));
        }
    } catch (err) {
        console.error("[FcEmu Mobile] Core initialization failed:", err);
    }
}, { once: true });

// Keep HUD interactive on hover/touch focus
const hud = document.getElementById("mobile-hud");
hud.addEventListener("touchstart", () => {
    hud.classList.add("active");
});
document.addEventListener("touchstart", (e) => {
    if (!hud.contains(e.target)) {
        hud.classList.remove("active");
    }
}, { passive: true });

document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") {
        if (audioCtx && audioCtx.state === "suspended") {
            audioCtx.resume();
        }
    } else {
        triggerAutoSave();
    }
});
