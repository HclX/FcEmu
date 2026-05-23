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
let currentScale = "fit";
let currentRatio = "native";

// PeerJS Netplay Variables
let peer = null;
let conn = null;
let localFrameIndex = 0;
let syncFrameIndex = 0;
let peerInputs = {};
let localInputs = {};
let isHost = false;
let netplayBlockStartTime = null;
const DEFAULT_ROMS = {
    "mario": { name: "Super Mario Bros.", path: "./public/roms/super_mario_bro.nes" },
    "contra": { name: "Contra", path: "./public/roms/contra.nes" },
    "multicart": { name: "1200-in-1 Multicart", path: "./public/roms/1200-in-1.nes" }
};
let userRomsCache = {}; // Cache user ROM ArrayBuffers by Hash key


// Expose globals for Playwright E2E tests
window.localFrameIndex = 0;
window.peerInputs = peerInputs;
window.localInputs = localInputs;
window.pauseIncomingPackets = false;
window.controllerState = 0;
window.controller2State = 0;

// DOM Elements
const canvas = document.getElementById("nes-canvas");
const ctx = canvas.getContext("2d");

const bootOverlay = document.getElementById("boot-overlay");
const bootBtn = document.getElementById("boot-btn");
const dropZone = document.getElementById("drop-zone");
const fileInput = document.getElementById("file-input");

// IndexedDB Helpers
const DB_NAME = "FcEmuDB";
const STORE_NAME = "sram_saves";
const ROM_STORE_NAME = "user_roms";
const SAVE_STATE_STORE_NAME = "save_states";
const DB_VERSION = 3;

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
            if (!db.objectStoreNames.contains(SAVE_STATE_STORE_NAME)) {
                db.createObjectStore(SAVE_STATE_STORE_NAME, { keyPath: "romHash" });
            }
        };
        request.onsuccess = (event) => resolve(event.target.result);
        request.onerror = (event) => reject(event.error);
    });
}

async function saveROMToDB(romHash, romName, romData) {
    const db = await openDB();
    return new Promise((resolve, reject) => {
        const transaction = db.transaction(ROM_STORE_NAME, "readwrite");
        const store = transaction.objectStore(ROM_STORE_NAME);
        const record = {
            romHash: romHash,
            romName: romName,
            romData: romData, // ArrayBuffer bytes
            addedAt: Date.now()
        };
        const request = store.put(record);
        request.onsuccess = () => resolve();
        request.onerror = () => reject(request.error);
    });
}

async function loadAllROMsFromDB() {
    const db = await openDB();
    return new Promise((resolve, reject) => {
        const transaction = db.transaction(ROM_STORE_NAME, "readonly");
        const store = transaction.objectStore(ROM_STORE_NAME);
        const request = store.getAll();
        request.onsuccess = () => resolve(request.result || []);
        request.onerror = () => reject(request.error);
    });
}

async function deleteROMFromDB(romHash) {
    const db = await openDB();
    return new Promise((resolve, reject) => {
        const transaction = db.transaction(ROM_STORE_NAME, "readwrite");
        const store = transaction.objectStore(ROM_STORE_NAME);
        const request = store.delete(romHash);
        request.onsuccess = () => resolve();
        request.onerror = () => reject(request.error);
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

async function saveSaveStateToDB(romHash, stateData) {
    const db = await openDB();
    return new Promise((resolve, reject) => {
        const transaction = db.transaction(SAVE_STATE_STORE_NAME, "readwrite");
        const store = transaction.objectStore(SAVE_STATE_STORE_NAME);
        const record = {
            romHash: romHash,
            stateData: stateData, // Uint8Array
            updatedAt: Date.now()
        };
        const request = store.put(record);
        request.onsuccess = () => resolve();
        request.onerror = () => reject(request.error);
    });
}

async function loadSaveStateFromDB(romHash) {
    const db = await openDB();
    return new Promise((resolve, reject) => {
        const transaction = db.transaction(SAVE_STATE_STORE_NAME, "readonly");
        const store = transaction.objectStore(SAVE_STATE_STORE_NAME);
        const request = store.get(romHash);
        request.onsuccess = () => {
            const record = request.result;
            resolve(record ? record.stateData : null);
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
    const targetRatio = 16 / 15; // Always keep native aspect ratio
    
    const reservedHeight = 48; // Page vertical padding/margins
    const reservedWidth = 392; // Sidebar (320px) + Gap (24px) + Margins (48px)
    
    // Calculate maximum available space, enforcing strict 2x minimums (512x480)
    const maxW = Math.max(512, window.innerWidth - reservedWidth);
    const maxH = Math.max(480, window.innerHeight - reservedHeight);
    
    let finalWidth = maxW;
    let finalHeight = maxW / targetRatio;
    
    if (finalHeight > maxH) {
        finalHeight = maxH;
        finalWidth = maxH * targetRatio;
    }
    
    canvas.style.setProperty("--canvas-width", `${Math.floor(finalWidth)}px`);
    canvas.style.setProperty("--canvas-height", `${Math.floor(finalHeight)}px`);
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
    "ControlLeft": BUTTON_A,
    "KeyZ": BUTTON_A,
    "AltLeft": BUTTON_B,
    "KeyX": BUTTON_B,
    "Space": BUTTON_SELECT,
    "Enter": BUTTON_START,
    "ArrowUp": BUTTON_UP,
    "ArrowDown": BUTTON_DOWN,
    "ArrowLeft": BUTTON_LEFT,
    "ArrowRight": BUTTON_RIGHT
};

let controllerState = 0;

// Setup event listeners for keyboard input
window.addEventListener("keydown", (event) => {
    if (KEY_MAP[event.code] !== undefined) {
        controllerState |= KEY_MAP[event.code];
        event.preventDefault();
    }
    if (event.code === "F5") {
        event.preventDefault();
        saveState();
    }
    if (event.code === "F9") {
        event.preventDefault();
        loadState();
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
    try {
        if (!audioCtx) {
            audioCtx = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 44100 });
            nextPlayTime = audioCtx.currentTime;
        }
        if (audioCtx.state === "suspended") {
            await audioCtx.resume();
        }
        if (audioCtx.state === "running") {
            bootOverlay.classList.add("hidden");
            console.log("Audio Context initialized and running.");
        } else {
            console.warn("Audio Context is suspended. Keeping overlay visible.");
            bootOverlay.classList.remove("hidden");
        }
    } catch (e) {
        console.error("Failed to start audio:", e);
        bootOverlay.classList.remove("hidden");
    }
}

// Helper to map gamepad button states to NES joypad bitmask
function mapGamepadToBitmask(gp) {
    if (!gp || !gp.buttons) return 0;
    let state = 0;
    if (gp.buttons[0] && gp.buttons[0].pressed) state |= BUTTON_A;
    if (gp.buttons[1] && gp.buttons[1].pressed) state |= BUTTON_B;
    if (gp.buttons[8] && gp.buttons[8].pressed) state |= BUTTON_SELECT;
    if (gp.buttons[9] && gp.buttons[9].pressed) state |= BUTTON_START;
    if (gp.buttons[12] && gp.buttons[12].pressed) state |= BUTTON_UP;
    if (gp.buttons[13] && gp.buttons[13].pressed) state |= BUTTON_DOWN;
    if (gp.buttons[14] && gp.buttons[14].pressed) state |= BUTTON_LEFT;
    if (gp.buttons[15] && gp.buttons[15].pressed) state |= BUTTON_RIGHT;
    return state;
}

// Dynamic gamepad polling based on Host/Guest context
function pollGamepads() {
    const gamepads = typeof navigator.getGamepads === "function" ? navigator.getGamepads() : [];
    const isGuest = !isHost && conn && conn.open;

    if (isGuest) {
        // Guest context: map the first available active gamepad to Player 2
        const gp = gamepads[0] || gamepads[1];
        window.controller2State = mapGamepadToBitmask(gp);
        window.controllerState = 0;
    } else {
        // Host/Local context: gamepads[0] is Player 1, gamepads[1] is Player 2
        window.controllerState = mapGamepadToBitmask(gamepads[0]);
        window.controller2State = mapGamepadToBitmask(gamepads[1]);
    }
}

// Main Emulation Render and Audio Loop
function loop() {
    if (!isRunning) return;

    // Poll virtual or real gamepads
    pollGamepads();

    const spinner = document.getElementById("buffering-spinner");

    if (conn && conn.open) {
        const currentFrame = localFrameIndex;
        const currentLocalInput = isHost ? (controllerState | window.controllerState) : (controllerState | window.controller2State);
        
        // Buffer local input and transmit to peer for execution 2 frames later
        localInputs[currentFrame] = currentLocalInput;
        conn.send(encodeInputPacket(currentFrame + 2, currentLocalInput));

        const isInitialFrame = (currentFrame - syncFrameIndex) < 2;
        const peerInputReceived = isInitialFrame || (!window.pauseIncomingPackets && (peerInputs[currentFrame] !== undefined));

        if (!peerInputReceived) {
            if (!netplayBlockStartTime) {
                netplayBlockStartTime = performance.now();
            } else {
                const elapsed = performance.now() - netplayBlockStartTime;
                if (elapsed > 5000) { // 5 seconds
                    console.warn("[Netplay] Lockstep timed out waiting for peer inputs. Disconnecting...");
                    alert("Connection lost: Peer timed out.");
                    netplayBlockStartTime = null;
                    if (conn) {
                        conn.close();
                    }
                    return;
                }
            }
            if (spinner) {
                spinner.classList.remove("hidden");
                spinner.style.display = "flex";
            }
            requestAnimationFrame(loop);
            return;
        }

        netplayBlockStartTime = null; // Reset keepalive/timeout

        if (spinner) {
            spinner.classList.add("hidden");
            spinner.style.display = "none";
        }

        const localInput = isInitialFrame ? 0 : localInputs[currentFrame - 2];
        const peerInput = isInitialFrame ? 0 : peerInputs[currentFrame];

        if (isHost) {
            emulator.write_controller(localInput);
            emulator.write_controller2(peerInput);
        } else {
            emulator.write_controller(peerInput);
            emulator.write_controller2(localInput);
        }

        emulator.step_frame();

        // Prune old input queues to prevent unbounded memory leaks in long sessions
        delete localInputs[currentFrame - 2];
        delete peerInputs[currentFrame];

        localFrameIndex = currentFrame + 1;
        window.localFrameIndex = localFrameIndex;
    } else {
        if (spinner) {
            spinner.classList.add("hidden");
            spinner.style.display = "none";
        }

        emulator.write_controller(controllerState | window.controllerState);
        emulator.write_controller2(window.controller2State);
        emulator.step_frame();

        localFrameIndex++;
        window.localFrameIndex = localFrameIndex;
    }

    // Step 3: Visual Output (100% Pure Zero-Copy Direct Memory Sharing)
    const framePtr = emulator.frame_buffer_ptr();
    const rgbaBuffer = new Uint8ClampedArray(wasm_exports.memory.buffer, framePtr, 256 * 240 * 4);
    const frameImgData = new ImageData(rgbaBuffer, 256, 240);
    ctx.putImageData(frameImgData, 0, 0);

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

            // Snaps-on-underflow logic: add a 50ms safety buffer to absorb frame/thread jitter and prevent crackle
            if (playTime < audioCtx.currentTime) {
                playTime = audioCtx.currentTime + 0.05;
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

    // If multiplayer session is active, close P2P connection to prevent state corruption
    if (conn && conn.open) {
        console.warn("[Netplay] Active connection detected. Disconnecting to load new ROM safely.");
        conn.close();
        if (!isHost && peer) {
            peer.destroy();
            peer = null;
            updateMultiplayerUI("idle");
            const statusEl = document.getElementById("connection-status");
            if (statusEl) {
                statusEl.textContent = "Disconnected";
                statusEl.style.color = "var(--text-muted)";
            }
        }
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
    if (document.visibilityState === "visible") {
        if (audioCtx && audioCtx.state === "suspended") {
            audioCtx.resume().then(() => {
                console.log("[FcEmu] Audio Context resumed after tab became visible.");
            });
        }
    } else if (document.visibilityState === "hidden") {
        console.log("[FcEmu] Tab became hidden, executing immediate SRAM save...");
        triggerAutoSave();
    }
});

// Process and extract standard iNES ROM files inside a ZIP archive
async function handleZipBuffer(arrayBuffer) {
    if (typeof JSZip === "undefined") {
        alert("ZIP extraction engine is not ready. Please wait or refresh the page.");
        return;
    }

    try {
        console.log("[FcEmu] Loading ZIP archive bytes into JSZip...");
        const zip = await JSZip.loadAsync(arrayBuffer);
        let addedCount = 0;
        let lastSelectedHash = null;
        let lastSelectedName = null;
        let lastSelectedData = null;

        const promises = [];
        zip.forEach((relativePath, file) => {
            if (relativePath.endsWith(".nes") && !file.dir) {
                const promise = file.async("arraybuffer").then(async (romData) => {
                    if (validateNESHeader(romData)) {
                        const hash = await computeROMHash(romData);
                        // Extract game name: strip extension and trim spaces
                        const cleanName = file.name.replace(/\.[^/.]+$/, "");
                        console.log(`[FcEmu] Extracted game ROM from ZIP: "${cleanName}" (SHA-256: ${hash})`);
                        await saveROMToDB(hash, cleanName, romData);
                        
                        addedCount++;
                        lastSelectedHash = hash;
                        lastSelectedName = cleanName;
                        lastSelectedData = romData;
                    }
                });
                promises.push(promise);
            }
        });

        await Promise.all(promises);

        if (addedCount > 0) {
            console.log(`[FcEmu] Persistent ROM library successfully imported ${addedCount} game(s) from ZIP archive.`);
            await refreshRomLibraryUI();
            
            // Automatically select and load the last imported game instantly!
            if (selectLibrary && lastSelectedHash) {
                selectLibrary.value = `user-${lastSelectedHash}`;
                syncDeleteButtonState();
            }
            if (lastSelectedData) {
                await handleROMBuffer(lastSelectedData.slice(0), lastSelectedName);
            }
            alert(`Successfully imported ${addedCount} game(s) from ZIP archive to your library!`);
        } else {
            alert("No valid iNES (.nes) ROM files found inside the uploaded ZIP archive.");
        }
    } catch (err) {
        console.error("[FcEmu] Failed to process ZIP file:", err);
        alert(`Failed to extract ZIP file: ${err.message}`);
    }
}

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
    if (files.length > 0) {
        const file = files[0];
        const reader = new FileReader();
        
        if (file.name.endsWith(".zip")) {
            reader.onload = async (event) => {
                await handleZipBuffer(event.target.result);
            };
            reader.readAsArrayBuffer(file);
        } else if (file.name.endsWith(".nes")) {
            reader.onload = async (event) => {
                const arrayBuffer = event.target.result;
                if (!validateNESHeader(arrayBuffer)) {
                    alert("Error: Invalid ROM file format. Magic signature does not match iNES standard.");
                    return;
                }
                try {
                    const hash = await computeROMHash(arrayBuffer);
                    // Clean up extension to extract game name
                    const cleanName = file.name.replace(/\.[^/.]+$/, "");
                    console.log(`[FcEmu] Storing dropped ROM into persistent IndexedDB library: ${cleanName}`);
                    await saveROMToDB(hash, cleanName, arrayBuffer);
                    await refreshRomLibraryUI();
                    
                    if (selectLibrary) {
                        selectLibrary.value = `user-${hash}`;
                        syncDeleteButtonState();
                    }
                    await handleROMBuffer(arrayBuffer.slice(0), cleanName);
                } catch (err) {
                    console.error("[FcEmu] Failed to import dropped ROM:", err);
                    alert("Failed to save dropped ROM to your persistent library.");
                }
            };
            reader.readAsArrayBuffer(file);
        } else {
            alert("Please drop a valid .nes ROM or .zip archive file.");
        }
    }
});

// File click selection handler (isolated to the browse text link to prevent click conflicts!)
const btnBrowseRoms = document.getElementById("btn-browse-roms");
if (btnBrowseRoms) {
    btnBrowseRoms.addEventListener("click", (e) => {
        e.stopPropagation(); // Prevent bubbling to parent dropdown container!
        fileInput.click();
    });
}

fileInput.addEventListener("change", (e) => {
    const files = e.target.files;
    if (files.length > 0) {
        const file = files[0];
        const reader = new FileReader();
        
        if (file.name.endsWith(".zip")) {
            reader.onload = async (event) => {
                await handleZipBuffer(event.target.result);
            };
            reader.readAsArrayBuffer(file);
        } else if (file.name.endsWith(".nes")) {
            reader.onload = async (event) => {
                const arrayBuffer = event.target.result;
                if (!validateNESHeader(arrayBuffer)) {
                    alert("Error: Invalid ROM file format. Magic signature does not match iNES standard.");
                    return;
                }
                try {
                    const hash = await computeROMHash(arrayBuffer);
                    // Clean up extension to extract game name
                    const cleanName = file.name.replace(/\.[^/.]+$/, "");
                    console.log(`[FcEmu] Storing selected ROM into persistent IndexedDB library: ${cleanName}`);
                    await saveROMToDB(hash, cleanName, arrayBuffer);
                    await refreshRomLibraryUI();
                    
                    if (selectLibrary) {
                        selectLibrary.value = `user-${hash}`;
                        syncDeleteButtonState();
                    }
                    await handleROMBuffer(arrayBuffer.slice(0), cleanName);
                } catch (err) {
                    console.error("[FcEmu] Failed to import selected ROM:", err);
                    alert("Failed to save selected ROM to your persistent library.");
                }
            };
            reader.readAsArrayBuffer(file);
        } else {
            alert("Please select a valid .nes ROM or .zip archive file.");
        }
    }
});

window.addEventListener("resize", () => {
    requestAnimationFrame(applyLayoutSize);
});

// Boot button overlay click event
bootBtn.addEventListener("click", startAudioAndCore);

// ==========================================
// ROM Library Management (IndexedDB Persistent Store)
// ==========================================
const selectLibrary = document.getElementById("library-select");
const btnLoadRom = document.getElementById("btn-load-rom");
const btnDeleteRom = document.getElementById("btn-delete-rom");

async function refreshRomLibraryUI() {
    if (!selectLibrary) return;
    
    // Clear dropdown options
    selectLibrary.innerHTML = "";
    userRomsCache = {};

    // 1. Add default ROM collection options
    for (const key in DEFAULT_ROMS) {
        const opt = document.createElement("option");
        opt.value = key;
        opt.textContent = `⚡ ${DEFAULT_ROMS[key].name}`;
        selectLibrary.appendChild(opt);
    }

    // 2. Add user-uploaded persistent ROMs from DB
    try {
        const userRoms = await loadAllROMsFromDB();
        userRoms.forEach(rom => {
            const opt = document.createElement("option");
            opt.value = `user-${rom.romHash}`;
            opt.textContent = `💾 ${rom.romName}`;
            selectLibrary.appendChild(opt);
            
            // Cache the ROM ArrayBuffer in memory for 0ms instant loads!
            userRomsCache[rom.romHash] = {
                name: rom.romName,
                data: rom.romData
            };
        });
    } catch (err) {
        console.error("[FcEmu] Failed to load user ROMs from IndexedDB:", err);
    }

    // Synchronize the Delete button visibility state
    syncDeleteButtonState();
}

function syncDeleteButtonState() {
    if (!selectLibrary || !btnDeleteRom) return;
    const val = selectLibrary.value;
    if (val && val.startsWith("user-")) {
        btnDeleteRom.style.display = "block";
    } else {
        btnDeleteRom.style.display = "none";
    }
}

if (selectLibrary) {
    selectLibrary.addEventListener("change", syncDeleteButtonState);
}

if (btnLoadRom) {
    btnLoadRom.addEventListener("click", async () => {
        const val = selectLibrary.value;
        if (!val) return;

        btnLoadRom.disabled = true;
        const originalText = btnLoadRom.textContent;
        btnLoadRom.textContent = "Ticking...";

        try {
            if (DEFAULT_ROMS[val]) {
                const romMeta = DEFAULT_ROMS[val];
                const response = await fetch(romMeta.path);
                if (!response.ok) throw new Error(`Server returned ${response.statusText}`);
                const arrayBuffer = await response.arrayBuffer();
                await handleROMBuffer(arrayBuffer, romMeta.name);
            } else if (val.startsWith("user-")) {
                // Load user ROM instantly from memory cache!
                const hash = val.replace("user-", "");
                const cached = userRomsCache[hash];
                if (cached) {
                    console.log(`[FcEmu] Loading user ROM "${cached.name}" from local IndexedDB cache...`);
                    await handleROMBuffer(cached.data.slice(0), cached.name);
                }
            }
        } catch (err) {
            console.error("[FcEmu] ROM loading failed:", err);
            alert(`Failed to load selected ROM: ${err.message}`);
        } finally {
            btnLoadRom.disabled = false;
            btnLoadRom.textContent = originalText;
        }
    });
}

if (btnDeleteRom) {
    btnDeleteRom.addEventListener("click", async () => {
        const val = selectLibrary.value;
        if (!val || !val.startsWith("user-")) return;

        const hash = val.replace("user-", "");
        const cached = userRomsCache[hash];
        if (confirm(`Are you sure you want to delete "${cached ? cached.name : 'this ROM'}" from your local library?`)) {
            btnDeleteRom.disabled = true;
            try {
                await deleteROMFromDB(hash);
                console.log(`[FcEmu] Successfully deleted ROM ${hash} from IndexedDB.`);
                await refreshRomLibraryUI();
            } catch (err) {
                console.error("[FcEmu] Failed to delete ROM:", err);
            } finally {
                btnDeleteRom.disabled = false;
            }
        }
    });
}

async function reloadCurrentSelectedROM() {
    const val = selectLibrary ? selectLibrary.value : "mario";
    if (!val) return;
    try {
        if (DEFAULT_ROMS[val]) {
            const romMeta = DEFAULT_ROMS[val];
            const response = await fetch(romMeta.path);
            if (!response.ok) throw new Error(`Server returned ${response.statusText}`);
            const arrayBuffer = await response.arrayBuffer();
            await handleROMBuffer(arrayBuffer, romMeta.name);
        } else if (val.startsWith("user-")) {
            const hash = val.replace("user-", "");
            const cached = userRomsCache[hash];
            if (cached) {
                console.log(`[FcEmu] Reloading user ROM "${cached.name}" from local cache...`);
                await handleROMBuffer(cached.data.slice(0), cached.name);
            }
        }
    } catch (e) {
        console.error("[FcEmu] Failed to reload active selected ROM:", e);
    }
}


// Graphics Filter Toggle Listeners
const btnFilterCrisp = document.getElementById("btn-filter-crisp");
const btnFilterSmooth = document.getElementById("btn-filter-smooth");

if (btnFilterCrisp && btnFilterSmooth) {
    btnFilterCrisp.addEventListener("click", () => {
        btnFilterCrisp.classList.add("active");
        btnFilterSmooth.classList.remove("active");
        canvas.classList.add("crisp");
    });

    btnFilterSmooth.addEventListener("click", () => {
        btnFilterSmooth.classList.add("active");
        btnFilterCrisp.classList.remove("active");
        canvas.classList.remove("crisp");
    });
}

// ==========================================
// PeerJS Netplay (Milestone 2)
// ==========================================

// Network Binary Packet Helpers (Milestone 4)
const PKT_TYPE_INPUT = 1;
const PKT_TYPE_SYNC = 2;

function encodeInputPacket(frame, input) {
    const buffer = new ArrayBuffer(6); // 1 (type) + 4 (frame) + 1 (input)
    const view = new DataView(buffer);
    view.setUint8(0, PKT_TYPE_INPUT);
    view.setUint32(1, frame, true); // Little-endian
    view.setUint8(5, input);
    return new Uint8Array(buffer);
}

function encodeSyncPacket(frame, stateBuffer) {
    const buffer = new ArrayBuffer(5 + stateBuffer.length); // 1 (type) + 4 (frame) + state_length
    const view = new DataView(buffer);
    view.setUint8(0, PKT_TYPE_SYNC);
    view.setUint32(1, frame, true);
    
    const pktArray = new Uint8Array(buffer);
    pktArray.set(stateBuffer, 5); // Copy state bytes at offset 5
    return pktArray;
}

function decodePacket(arrayBuffer) {
    try {
        const view = new DataView(arrayBuffer);
        const type = view.getUint8(0);
        const frame = view.getUint32(1, true);
        
        if (type === PKT_TYPE_INPUT) {
            const input = view.getUint8(5);
            return { type: "INPUT", frame, input };
        } else if (type === PKT_TYPE_SYNC) {
            // Extract state slice
            const state = new Uint8Array(arrayBuffer, 5);
            return { type: "SYNC_STATE", frame, state };
        }
    } catch (e) {
        console.error("[Netplay] Failed to decode binary packet:", e);
    }
    return null;
}

// UI State Machine for Multiplayer Panel (Visual locks & mutual exclusion)
function updateMultiplayerUI(state) {
    const hostBtn = document.getElementById("btn-host-game");
    const joinBtn = document.getElementById("btn-join-game");
    const joinPeerInput = document.getElementById("peer-id-input");
    const statusEl = document.getElementById("connection-status");

    if (!hostBtn || !joinBtn || !joinPeerInput || !statusEl) return;

    switch (state) {
        case "idle":
            hostBtn.disabled = false;
            hostBtn.textContent = "Host Game";
            hostBtn.style.opacity = "1.0";
            hostBtn.style.backgroundColor = "";
            hostBtn.style.color = "";
            
            joinPeerInput.readOnly = false;
            joinPeerInput.disabled = false;
            joinPeerInput.value = "";
            joinPeerInput.placeholder = "Enter ID or Room Link";
            
            joinBtn.disabled = false;
            joinBtn.textContent = "Join";
            break;
            
        case "hosting":
            hostBtn.disabled = false; // Enabled so Host can click to STOP hosting!
            hostBtn.textContent = "Stop Hosting";
            hostBtn.style.opacity = "1.0";
            hostBtn.style.backgroundColor = "#f7768e"; // Soft red style
            hostBtn.style.color = "#1a1b26";
            
            joinPeerInput.readOnly = true;
            
            joinBtn.disabled = false;
            joinBtn.textContent = "Copy Link";
            break;
            
        case "host-connected":
            hostBtn.disabled = true; // Disabled when Player 2 is actively connected!
            hostBtn.textContent = "Hosting";
            hostBtn.style.opacity = "0.5";
            hostBtn.style.backgroundColor = "";
            hostBtn.style.color = "";
            
            joinPeerInput.readOnly = true;
            
            joinBtn.disabled = false;
            joinBtn.textContent = "Disconnect";
            break;
            
        case "guest-connecting":
            hostBtn.disabled = true;
            hostBtn.textContent = "Host Game";
            hostBtn.style.opacity = "0.5";
            hostBtn.style.backgroundColor = "";
            hostBtn.style.color = "";
            
            joinPeerInput.readOnly = true;
            
            joinBtn.disabled = false;
            joinBtn.textContent = "Cancel";
            break;
            
        case "guest-connected":
            hostBtn.disabled = true;
            hostBtn.textContent = "Host Game";
            hostBtn.style.opacity = "0.5";
            hostBtn.style.backgroundColor = "";
            hostBtn.style.color = "";
            
            joinPeerInput.readOnly = true;
            
            joinBtn.disabled = false;
            joinBtn.textContent = "Disconnect";
            break;
    }
}

function initPeer(asHost = true) {
    if (peer) return;

    // Generate 4-digit numeric code
    const code = Math.floor(1000 + Math.random() * 9000).toString();
    const namespacedId = `fce-${code}`;
    console.log(`[Netplay] Initializing PeerJS client with namespaced ID: ${namespacedId}`);
    
    peer = new Peer(namespacedId);

    peer.on("open", (id) => {
        console.log(`[Netplay] PeerJS initialized. My Peer ID: ${id}`);
        
        if (asHost) {
            isHost = true;
            const displayId = id.startsWith("fce-") ? id.replace("fce-", "") : id;
            const upperDisplayId = displayId.toUpperCase();

            const peerIdInput = document.getElementById("peer-id-input");
            if (peerIdInput) {
                peerIdInput.value = upperDisplayId;
            }
            const statusEl = document.getElementById("connection-status");
            if (statusEl) {
                statusEl.textContent = `Hosting. ID: ${upperDisplayId}`;
                statusEl.style.color = "var(--accent-color)";
            }

            updateMultiplayerUI("hosting");

            // Construct shareable room URL (keeps the full namespaced ID to preserve URL joins!)
            const shareUrl = `${window.location.origin}${window.location.pathname}?room=${id}`;
            console.log(`[Netplay] Shareable connection link: ${shareUrl}`);
        }
    });

    peer.on("connection", (incomingConn) => {
        // Enforce 1v1 limitation: reject incoming connections if session is active
        if (conn) {
            console.log("[Netplay] Rejecting extra connection to maintain 1v1 session.");
            incomingConn.close();
            return;
        }
        conn = incomingConn;
        setupConnectionHandlers(conn);
    });

    peer.on("error", (err) => {
        console.error("[Netplay] PeerJS Error:", err);
        const statusEl = document.getElementById("connection-status");
        if (statusEl) {
            statusEl.textContent = `Error: ${err.type}`;
            statusEl.style.color = "#f7768e";
        }
        
        // Release visual locks on connection failures
        if (err.type === "peer-not-found" || err.type === "network" || err.type === "webrtc") {
            const wasHost = isHost;
            if (conn) {
                conn.close();
                conn = null;
            }
            updateMultiplayerUI(wasHost ? "hosting" : "idle");
        }
    });
}

function setupConnectionHandlers(connection) {
    connection.on("open", () => {
        console.log(`[Netplay] Data channel open with peer: ${connection.peer}`);
        
        // Clear stale input queues to prevent desync carry-over
        for (const key in peerInputs) {
            delete peerInputs[key];
        }
        for (const key in localInputs) {
            delete localInputs[key];
        }

        // Host captures and transmits full emulator save snapshot to hot-join the Guest
        if (isHost) {
            syncFrameIndex = localFrameIndex;
            const stateBuffer = emulator.save_state();
            console.log(`[Netplay] Capturing and transmitting Host savestate at frame ${syncFrameIndex} (size: ${stateBuffer.length} bytes)`);
            connection.send(encodeSyncPacket(syncFrameIndex, stateBuffer));
        }

        const statusEl = document.getElementById("connection-status");
        if (statusEl) {
            statusEl.textContent = isHost ? "Connected to Player 2!" : "Connected to Player 1 (Host)!";
            statusEl.style.color = "#9ece6a";
        }

        updateMultiplayerUI(isHost ? "host-connected" : "guest-connected");

        // Send verification handshake payload
        connection.send({ type: "HANDSHAKE", message: "Hello from FcEmu Peer!" });
    });

    connection.on("data", (data) => {
        const packet = decodePacket(data instanceof ArrayBuffer ? data : data.buffer || data);
        if (!packet) return;

        if (packet.type === "INPUT") {
            peerInputs[packet.frame] = packet.input;
        } else if (packet.type === "SYNC_STATE") {
            // Guest receives full host snapshot and aligns state/frame variables
            syncFrameIndex = packet.frame;
            localFrameIndex = packet.frame;
            window.localFrameIndex = localFrameIndex;
            
            const loaded = emulator.load_state(packet.state);
            console.log(`[Netplay] Received Host savestate. Loaded: ${loaded}. Aligned to frame: ${localFrameIndex}`);
            
            // Clear only pre-sync stale inputs, preserving valid look-ahead future inputs!
            for (const key in peerInputs) {
                if (parseInt(key) < syncFrameIndex) {
                    delete peerInputs[key];
                }
            }
            for (const key in localInputs) {
                if (parseInt(key) < syncFrameIndex) {
                    delete localInputs[key];
                }
            }
        }
    });

    connection.on("close", () => {
        console.log("[Netplay] Data channel closed by remote peer.");
        const statusEl = document.getElementById("connection-status");
        if (statusEl) {
            statusEl.textContent = isHost ? `Hosting. ID: ${peer.id}` : "Disconnected";
            statusEl.style.color = isHost ? "var(--accent-color)" : "var(--text-muted)";
        }
        const wasHost = isHost;
        conn = null;
        updateMultiplayerUI(wasHost ? "hosting" : "idle");
        
        if (!wasHost) {
            console.log("[Netplay] Reverting Guest emulator back to clean starting state...");
            reloadCurrentSelectedROM();
        }
    });

    connection.on("error", (err) => {
        console.error("[Netplay] Data channel error:", err);
        const statusEl = document.getElementById("connection-status");
        if (statusEl) {
            statusEl.textContent = isHost ? `Hosting. ID: ${peer.id}` : "Connection Failed";
            statusEl.style.color = "#f7768e";
        }
        const wasHost = isHost;
        conn = null;
        updateMultiplayerUI(wasHost ? "hosting" : "idle");
        
        if (!wasHost) {
            console.log("[Netplay] Reverting Guest emulator back to clean starting state...");
            reloadCurrentSelectedROM();
        }
    });
}

function connectToHost(targetId) {
    isHost = false;
    if (conn) {
        console.log("[Netplay] Closing current active connection before connecting to new host.");
        conn.close();
    }
    const statusEl = document.getElementById("connection-status");
    if (statusEl) {
        statusEl.textContent = "Connecting...";
        statusEl.style.color = "var(--accent-hover)";
    }
    updateMultiplayerUI("guest-connecting");
    conn = peer.connect(targetId);
    setupConnectionHandlers(conn);
}

// Bind Multiplayer Connection UI Event Listeners
const hostBtn = document.getElementById("btn-host-game");
const joinBtn = document.getElementById("btn-join-game");
const joinPeerInput = document.getElementById("peer-id-input");

if (hostBtn) {
    hostBtn.addEventListener("click", () => {
        const action = hostBtn.textContent.trim();
        if (action === "Stop Hosting") {
            console.log("[Netplay] Stopping P2P host server...");
            if (conn) {
                conn.close();
            }
            if (peer) {
                peer.destroy();
                peer = null;
            }
            isHost = false;
            syncFrameIndex = 0;
            updateMultiplayerUI("idle");
            const statusEl = document.getElementById("connection-status");
            if (statusEl) {
                statusEl.textContent = "Disconnected";
                statusEl.style.color = "var(--text-muted)";
            }
            return;
        }
        
        initPeer();
    });
}

if (joinBtn && joinPeerInput) {
    joinBtn.addEventListener("click", async () => {
        const action = joinBtn.textContent.trim();
        
        if (action === "Cancel") {
            console.log("[Netplay] Canceling connection attempt...");
            if (conn) {
                conn.close();
                conn = null;
            }
            if (peer) {
                peer.destroy();
                peer = null;
            }
            updateMultiplayerUI("idle");
            const statusEl = document.getElementById("connection-status");
            if (statusEl) {
                statusEl.textContent = "Disconnected";
                statusEl.style.color = "var(--text-muted)";
            }
            return;
        }

        if (action === "Copy Link" || action === "Copy") {
            try {
                const id = joinPeerInput.value.trim();
                const shareUrl = `${window.location.origin}${window.location.pathname}?room=${id}`;
                await navigator.clipboard.writeText(shareUrl);
                joinBtn.textContent = "Link Copied!";
                joinBtn.style.backgroundColor = "#9ece6a";
                joinBtn.style.color = "#1a1b26";
                setTimeout(() => {
                    if (joinBtn.textContent === "Link Copied!") {
                        joinBtn.textContent = "Copy Link";
                        joinBtn.style.backgroundColor = "";
                        joinBtn.style.color = "";
                    }
                }, 1500);
            } catch (err) {
                console.error("Failed to copy Room URL:", err);
            }
            return;
        }
        
        if (action === "Disconnect") {
            if (conn) {
                conn.close();
            }
            if (peer) {
                if (!isHost) {
                    peer.destroy();
                    peer = null;
                    updateMultiplayerUI("idle");
                    const statusEl = document.getElementById("connection-status");
                    if (statusEl) {
                        statusEl.textContent = "Disconnected";
                        statusEl.style.color = "var(--text-muted)";
                    }
                }
            }
            return;
        }
        
        let targetId = joinPeerInput.value.trim();
        if (!targetId) {
            alert("Please enter a Host Peer ID to join.");
            return;
        }

        // Extract Peer ID if Guest entered/pasted a full Room URL
        if (targetId.includes("?room=")) {
            try {
                const url = new URL(targetId);
                const room = url.searchParams.get("room");
                if (room) {
                    targetId = room;
                }
            } catch (e) {
                const parts = targetId.split("?room=");
                if (parts.length > 1) {
                    targetId = parts[1].split("&")[0];
                }
            }
        }

        // Enforce 4-digit code format namespacing
        if (/^\d{4}$/.test(targetId)) {
            targetId = `fce-${targetId}`;
        } else if (targetId.toLowerCase().startsWith("fce-")) {
            targetId = targetId.toLowerCase();
        }
        
        if (!peer) {
            initPeer(false);
        }

        if (peer.open) {
            connectToHost(targetId);
        } else {
            peer.once("open", () => {
                connectToHost(targetId);
            });
        }
    });
}

// Automatically parse room query parameter on page load
const urlParams = new URLSearchParams(window.location.search);
const roomParam = urlParams.get("room");
if (roomParam) {
    let targetId = roomParam;
    if (/^\d{4}$/.test(targetId)) {
        targetId = `fce-${targetId}`;
    } else if (targetId.toLowerCase().startsWith("fce-")) {
        targetId = targetId.toLowerCase();
    }
    console.log(`[Netplay] Detected room query parameter. Auto-connecting to: ${targetId}`);
    if (!peer) {
        initPeer(false);
    }
    if (peer.open) {
        connectToHost(targetId);
    } else {
        peer.once("open", () => {
            connectToHost(targetId);
        });
    }
}

async function saveState() {
    if (!emulator || !currentRomHash) {
        console.warn("[FcEmu] No active emulator or ROM to save state.");
        return;
    }
    try {
        const stateBuffer = emulator.save_state();
        await saveSaveStateToDB(currentRomHash, stateBuffer);
        console.log(`[FcEmu] State saved successfully (${stateBuffer.length} bytes).`);
    } catch (err) {
        console.error("[FcEmu] Failed to save state:", err);
        alert("Failed to save state.");
    }
}

async function loadState() {
    if (!emulator || !currentRomHash) {
        console.warn("[FcEmu] No active emulator or ROM to load state.");
        return;
    }
    try {
        const stateData = await loadSaveStateFromDB(currentRomHash);
        if (stateData) {
            const success = emulator.load_state(stateData);
            if (success) {
                console.log("[FcEmu] State loaded successfully.");
            } else {
                console.error("[FcEmu] Emulator failed to load state.");
                alert("Failed to load state: Emulator error.");
            }
        } else {
            alert("No saved state found for this ROM.");
        }
    } catch (err) {
        console.error("[FcEmu] Failed to load state:", err);
        alert("Failed to load state.");
    }
}

// Bind Savestate UI Buttons
const btnSaveState = document.getElementById("btn-save-state");
const btnLoadState = document.getElementById("btn-load-state");
if (btnSaveState) {
    btnSaveState.addEventListener("click", saveState);
}
if (btnLoadState) {
    btnLoadState.addEventListener("click", loadState);
}

// Apply base sizing and initialize WASM Emulator core
applyLayoutSize();
initWasm().then(async () => {
    console.log("[FcEmu] Initializing persistent ROM Library Selector & Matchmaker...");
    await refreshRomLibraryUI();
    
    // Auto-load the default Mario game on initial page bootup!
    if (selectLibrary) {
        selectLibrary.value = "mario";
        syncDeleteButtonState();
        
        const romMeta = DEFAULT_ROMS["mario"];
        const response = await fetch(romMeta.path);
        if (response && response.ok) {
            const arrayBuffer = await response.arrayBuffer();
            await handleROMBuffer(arrayBuffer, romMeta.name);
        }
    }
});
