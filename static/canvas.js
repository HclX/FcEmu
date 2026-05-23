import init, { WasmEmulator } from "../pkg/fce_core.js";

// Global emulator and Audio state
let emulator = null;
let wasm_exports = null;
let audioCtx = null;
let nextPlayTime = 0;
let isRunning = false;

// SRAM Persistence variables
let currentRomHash = null;
let currentRomName = null;
let lastSavedSram = null;
let autoSaveIntervalId = null;

// Display size variables
let currentScale = "2";
let currentRatio = "native";

// DOM Elements
const canvas = document.getElementById("nes-canvas");
const ctx = canvas.getContext("2d");
const imgData = ctx.createImageData(256, 240);

const bootOverlay = document.getElementById("boot-overlay");
const bootBtn = document.getElementById("boot-btn");
const dropZone = document.getElementById("drop-zone");
const fileInput = document.getElementById("file-input");
const ratioButtons = document.querySelectorAll("#ratio-group .btn-control");
const sizeButtons = document.querySelectorAll("#size-group .btn-control");

// IndexedDB Helpers
const DB_NAME = "FcEmuDB";
const STORE_NAME = "sram_saves";
const DB_VERSION = 1;

function openDB() {
    return new Promise((resolve, reject) => {
        const request = indexedDB.open(DB_NAME, DB_VERSION);
        request.onupgradeneeded = (event) => {
            const db = event.target.result;
            if (!db.objectStoreNames.contains(STORE_NAME)) {
                db.createObjectStore(STORE_NAME, { keyPath: "romHash" });
            }
        };
        request.onsuccess = (event) => resolve(event.target.result);
        request.onerror = (event) => reject(event.error);
    });
}

async function saveSRAMToDB(romHash, romName, sramData) {
    const db = await openDB();
    return new Promise((resolve, reject) => {
        const transaction = db.transaction(STORE_NAME, "readwrite");
        const store = transaction.objectStore(STORE_NAME);
        const record = {
            romHash: romHash,
            romName: romName,
            sramData: sramData,
            updatedAt: Date.now()
        };
        const request = store.put(record);
        request.onsuccess = () => resolve();
        request.onerror = () => reject(request.error);
    });
}

async function loadSRAMFromDB(romHash) {
    const db = await openDB();
    return new Promise((resolve, reject) => {
        const transaction = db.transaction(STORE_NAME, "readonly");
        const store = transaction.objectStore(STORE_NAME);
        const request = store.get(romHash);
        request.onsuccess = () => {
            const record = request.result;
            resolve(record ? record.sramData : null);
        };
        request.onerror = () => reject(request.error);
    });
}

// Web Crypto SHA-256 Helper
async function computeROMHash(arrayBuffer) {
    const hashBuffer = await crypto.subtle.digest("SHA-256", arrayBuffer);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    const hashHex = hashArray.map(b => b.toString(16).padStart(2, "0")).join("");
    return hashHex;
}

// Magic Bytes iNES Check
function validateNESHeader(arrayBuffer) {
    if (arrayBuffer.byteLength < 16) return false;
    const header = new Uint8Array(arrayBuffer, 0, 4);
    return header[0] === 0x4E && // 'N'
           header[1] === 0x45 && // 'E'
           header[2] === 0x53 && // 'S'
           header[3] === 0x1A;   // EOF
}

// Sizing Engine Layout Function
function applyLayoutSize() {
    const targetRatio = currentRatio === "crt" ? (4 / 3) : (16 / 15);
    if (currentScale === "fit") {
        const reservedHeight = 260;
        const reservedWidth = 48;
        const maxW = window.innerWidth - reservedWidth;
        const maxH = window.innerHeight - reservedHeight;
        
        let finalWidth = maxW;
        let finalHeight = maxW / targetRatio;
        if (finalHeight > maxH) {
            finalHeight = maxH;
            finalWidth = maxH * targetRatio;
        }
        canvas.style.setProperty("--canvas-width", `${Math.floor(finalWidth)}px`);
        canvas.style.setProperty("--canvas-height", `${Math.floor(finalHeight)}px`);
    } else {
        const scaleFactor = parseInt(currentScale);
        const baseWidth = currentRatio === "crt" ? 320 : 256;
        const baseHeight = 240;
        canvas.style.setProperty("--canvas-width", `${baseWidth * scaleFactor}px`);
        canvas.style.setProperty("--canvas-height", `${baseHeight * scaleFactor}px`);
    }
}

// NES Joypad Bitmasks
const BUTTON_A = 1 << 0;
const BUTTON_B = 1 << 1;
const BUTTON_SELECT = 1 << 2;
const BUTTON_START = 1 << 3;
const BUTTON_UP = 1 << 4;
const BUTTON_DOWN = 1 << 5;
const BUTTON_LEFT = 1 << 6;
const BUTTON_RIGHT = 1 << 7;

// Keyboard Event mappings to Joypad bits
const KEY_MAP = {
    "KeyZ": BUTTON_A,
    "KeyJ": BUTTON_A,
    "KeyX": BUTTON_B,
    "KeyK": BUTTON_B,
    "ShiftRight": BUTTON_SELECT,
    "Space": BUTTON_SELECT,
    "Enter": BUTTON_START,
    "ArrowUp": BUTTON_UP,
    "KeyW": BUTTON_UP,
    "ArrowDown": BUTTON_DOWN,
    "KeyS": BUTTON_DOWN,
    "ArrowLeft": BUTTON_LEFT,
    "KeyA": BUTTON_LEFT,
    "ArrowRight": BUTTON_RIGHT,
    "KeyD": BUTTON_RIGHT
};

let controllerState = 0;

// Setup event listeners for keyboard input
window.addEventListener("keydown", (event) => {
    if (KEY_MAP[event.code] !== undefined) {
        controllerState |= KEY_MAP[event.code];
        event.preventDefault();
    }
});

window.addEventListener("keyup", (event) => {
    if (KEY_MAP[event.code] !== undefined) {
        controllerState &= ~KEY_MAP[event.code];
        event.preventDefault();
    }
});

// Initialize the WASM Module
async function initWasm() {
    try {
        wasm_exports = await init();
        emulator = new WasmEmulator();
        console.log("WASM Emulator core initialized.");
    } catch (err) {
        console.error("Failed to initialize WASM module:", err);
    }
}

// Initialize Web Audio context on interactive gesture
async function startAudioAndCore() {
    if (!audioCtx) {
        audioCtx = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 44100 });
        nextPlayTime = audioCtx.currentTime;
    }
    if (audioCtx.state === "suspended") {
        await audioCtx.resume();
    }
    bootOverlay.classList.add("hidden");
    console.log("Audio Context initialized.");
}

// Main Emulation Render and Audio Loop
function loop() {
    if (!isRunning) return;

    // Step 1: Input propagation
    emulator.write_controller(controllerState);

    // Step 2: Run engine to produce one PPU frame and synthesize audio
    emulator.step_frame();

    // Step 3: Visual Output (zero-copy 32-bit cast RGB to RGBA expansion loop)
    const framePtr = emulator.frame_buffer_ptr();
    const rgbBuffer = new Uint8Array(wasm_exports.memory.buffer, framePtr, 256 * 240 * 3);
    const rgba32 = new Uint32Array(imgData.data.buffer);
    
    let srcIdx = 0;
    for (let i = 0; i < 256 * 240; i++) {
        const r = rgbBuffer[srcIdx];
        const g = rgbBuffer[srcIdx + 1];
        const b = rgbBuffer[srcIdx + 2];
        
        // Little-endian system pixel packing: 0xFFBBGGRR
        // R is at offset 0 (bits 0-7), G is at offset 1 (bits 8-15), B is at offset 2 (bits 16-23), A is 255 (bits 24-31)
        rgba32[i] = 0xFF000000 | (b << 16) | (g << 8) | r;
        srcIdx += 3;
    }
    ctx.putImageData(imgData, 0, 0);

    // Step 4: Web Audio Scheduling (Dynamic short play nodes with latency control)
    if (audioCtx && audioCtx.state !== "suspended") {
        const samplePtr = emulator.sample_buffer_ptr();
        const sampleLen = emulator.sample_buffer_len();
        
        if (sampleLen > 0) {
            const sampleBuffer = new Float32Array(wasm_exports.memory.buffer, samplePtr, sampleLen);
            
            // Create Audio Buffer and copy samples
            const audioBuffer = audioCtx.createBuffer(1, sampleLen, 44100);
            audioBuffer.getChannelData(0).set(sampleBuffer);

            // Create short play node
            const source = audioCtx.createBufferSource();
            source.buffer = audioBuffer;
            source.connect(audioCtx.destination);

            const duration = sampleLen / 44100;
            let playTime = nextPlayTime;

            // Snaps-on-underflow logic
            if (playTime < audioCtx.currentTime) {
                playTime = audioCtx.currentTime;
            }

            // 100ms ceiling latency snapping, resetting to 20ms budget
            if (playTime - audioCtx.currentTime > 0.1) {
                playTime = audioCtx.currentTime + 0.02;
            }

            source.start(playTime);
            nextPlayTime = playTime + duration;

            // Clear/drain the sample buffer in WASM core
            emulator.clear_sample_buffer();
        }
    }

    requestAnimationFrame(loop);
}

// Window-level Drag Interceptors to Prevent Tab Overwriting
window.addEventListener("dragover", (e) => e.preventDefault(), false);
window.addEventListener("drop", (e) => e.preventDefault(), false);

// ROM loading handler with DB auto-restore and auto-save initialization
async function handleROMBuffer(arrayBuffer, romName = "unknown_rom.nes") {
    if (!emulator) {
        alert("Emulator core is not ready yet. Please wait.");
        return;
    }
    
    // 1. Inspect iNES magic bytes before injection
    if (!validateNESHeader(arrayBuffer)) {
        alert("Error: Invalid ROM file format. Magic signature does not match iNES standard.");
        return;
    }

    await startAudioAndCore();

    // 2. Compute SHA-256 ROM Hash
    const romHash = await computeROMHash(arrayBuffer);
    console.log(`[FcEmu] Loaded ROM: ${romName} (SHA-256: ${romHash})`);

    // 3. Clear previous save loops
    if (autoSaveIntervalId) {
        clearInterval(autoSaveIntervalId);
        autoSaveIntervalId = null;
    }

    currentRomHash = romHash;
    currentRomName = romName;
    lastSavedSram = null;

    // 4. Load the ROM bytes into the WASM core
    const uint8Array = new Uint8Array(arrayBuffer);
    const success = emulator.load_rom(uint8Array);
    
    if (success) {
        console.log("ROM loaded successfully. Starting loop.");
        
        // Update dropzone text
        const dropZonePara = dropZone.querySelector("p");
        const dropZoneSpan = dropZone.querySelector("span");
        if (dropZonePara) dropZonePara.textContent = `ROM Active: ${romName}`;
        if (dropZoneSpan) dropZoneSpan.textContent = "Click or drop to swap ROMs";

        // 5. Auto-Restore SRAM if cartridge supports battery-backed SRAM
        if (emulator.has_battery_backed_sram()) {
            try {
                const savedSram = await loadSRAMFromDB(romHash);
                if (savedSram) {
                    const restoreSuccess = emulator.set_sram(savedSram);
                    if (restoreSuccess) {
                        lastSavedSram = savedSram.slice();
                        console.log("[FcEmu] Successfully restored SRAM save state.");
                    }
                } else {
                    console.log("[FcEmu] No existing SRAM save state found. Starting fresh.");
                    const freshSram = emulator.get_sram();
                    if (freshSram) lastSavedSram = freshSram.slice();
                }
            } catch (err) {
                console.error("[FcEmu] Failed to restore SRAM save state:", err);
            }

            // 6. Initialize 5-second auto-save interval
            autoSaveIntervalId = setInterval(triggerAutoSave, 5000);
        }

        if (!isRunning) {
            isRunning = true;
            requestAnimationFrame(loop);
        }
    } else {
        alert("Failed to load ROM. Ensure it is a valid iNES format file.");
    }
}

// Auto-Save dirty checking save function
async function triggerAutoSave() {
    if (!emulator || !currentRomHash) return;
    if (!emulator.has_battery_backed_sram()) return;

    const currentSram = emulator.get_sram();
    if (!currentSram) return;

    if (!isSramDirty(currentSram, lastSavedSram)) {
        return; 
    }

    try {
        await saveSRAMToDB(currentRomHash, currentRomName, currentSram);
        lastSavedSram = currentSram.slice();
        console.log(`[FcEmu] Auto-saved SRAM state (${currentSram.length} bytes).`);
    } catch (err) {
        console.error("[FcEmu] Failed to auto-save SRAM:", err);
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

// Setup modern visibilitychange listener
document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") {
        console.log("[FcEmu] Tab became hidden, executing immediate SRAM save...");
        triggerAutoSave();
    }
});

// Drag and Drop event handlers
dropZone.addEventListener("dragover", (e) => {
    e.preventDefault();
    dropZone.classList.add("dragover");
});

dropZone.addEventListener("dragleave", () => {
    dropZone.classList.remove("dragover");
});

dropZone.addEventListener("drop", async (e) => {
    e.preventDefault();
    dropZone.classList.remove("dragover");
    
    const files = e.dataTransfer.files;
    if (files.length > 0 && files[0].name.endsWith(".nes")) {
        const reader = new FileReader();
        const fileName = files[0].name;
        reader.onload = (event) => {
            handleROMBuffer(event.target.result, fileName);
        };
        reader.readAsArrayBuffer(files[0]);
    } else {
        alert("Please drop a valid .nes ROM file.");
    }
});

// File click selection handler
dropZone.addEventListener("click", () => {
    fileInput.click();
});

fileInput.addEventListener("change", (e) => {
    const files = e.target.files;
    if (files.length > 0) {
        const reader = new FileReader();
        const fileName = files[0].name;
        reader.onload = (event) => {
            handleROMBuffer(event.target.result, fileName);
        };
        reader.readAsArrayBuffer(files[0]);
    }
});

// Sizing Buttons Click Listeners
ratioButtons.forEach(btn => {
    btn.addEventListener("click", () => {
        ratioButtons.forEach(b => b.classList.remove("active"));
        btn.classList.add("active");
        currentRatio = btn.dataset.ratio;
        applyLayoutSize();
    });
});

sizeButtons.forEach(btn => {
    btn.addEventListener("click", () => {
        sizeButtons.forEach(b => b.classList.remove("active"));
        btn.classList.add("active");
        currentScale = btn.dataset.size;
        applyLayoutSize();
    });
});

window.addEventListener("resize", () => {
    if (currentScale === "fit") {
        requestAnimationFrame(applyLayoutSize);
    }
});

// Boot button overlay click event
bootBtn.addEventListener("click", startAudioAndCore);

const btnLoadDefault = document.getElementById("btn-load-default");
if (btnLoadDefault) {
    btnLoadDefault.addEventListener("click", async () => {
        btnLoadDefault.disabled = true;
        const originalText = btnLoadDefault.textContent;
        btnLoadDefault.textContent = "⚡ Fetching ROM...";
        
        try {
            let response = null;
            // Try production/Vite-server relative path first
            try {
                response = await fetch("./roms/super_mario_bro.nes");
                if (!response.ok) throw new Error();
            } catch (e) {
                // Fallback for local manual development serving from workspace root
                response = await fetch("./public/roms/super_mario_bro.nes");
                if (!response.ok) {
                    throw new Error(`Server returned status ${response.status}`);
                }
            }
            
            const arrayBuffer = await response.arrayBuffer();
            await handleROMBuffer(arrayBuffer, "super_mario_bro.nes");
        } catch (err) {
            console.error("Failed to load default ROM:", err);
            alert(`Failed to load default ROM: ${err.message}. Ensure the ROM file exists at 'roms/super_mario_bro.nes' in your static build folder.`);
        } finally {
            btnLoadDefault.disabled = false;
            btnLoadDefault.textContent = originalText;
        }
    });
}

// Apply base sizing and initialize WASM Emulator core
applyLayoutSize();
initWasm();
