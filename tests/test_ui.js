import { test, expect } from '@playwright/test';
import fs from 'fs';
import path from 'path';
import { bootEmulator } from './helpers.js';

// Desktop UI interaction E2E tests: boot the emulator, then exercise the
// controls a real user touches — keyboard, remapping, pause, display
// controls, ROM loading / switching, save states, SRAM, fullscreen.
// (Recording + replay live in test_replay.js, netplay in test_netplay.js.)

async function boot(page) {
  await page.goto('/');
  await bootEmulator(page);
}

async function controllerState(page) {
  // Keyboard state (window.controllerState is gamepad-only: pollGamepads
  // overwrites it every frame).
  return page.evaluate(() => window.keyboardControllerState);
}

async function canvasSize(page) {
  return page.evaluate(() => {
    const cs = getComputedStyle(document.getElementById('nes-canvas'));
    return {
      width: cs.getPropertyValue('--canvas-width').trim(),
      height: cs.getPropertyValue('--canvas-height').trim(),
    };
  });
}

function collectConsole(page) {
  const messages = [];
  page.on('console', (msg) => messages.push(msg.text()));
  return messages;
}

async function waitForMessage(messages, substring, timeout = 15000) {
  await expect.poll(() => messages.some((m) => m.includes(substring)), { timeout }).toBe(true);
}

// Read an IndexedDB object store, reduced to plain JSON-safe values so
// typed arrays (Uint8Array) survive Playwright's serialization.
async function readStore(page, storeName) {
  return page.evaluate(async (name) => {
    const db = await new Promise((resolve, reject) => {
      const req = indexedDB.open('FcEmuDB', 3);
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    const records = await new Promise((resolve, reject) => {
      const req = db.transaction(name, 'readonly').objectStore(name).getAll();
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    db.close();
    return records.map((r) => ({
      romHash: r.romHash,
      romName: r.romName,
      sramFirstByte: r.sramData ? r.sramData[0] : null,
      stateDataLength: r.stateData ? r.stateData.length : null,
    }));
  }, storeName);
}

// Minimal battery-backed NROM test ROM: on reset it writes 0x42 to SRAM
// ($6000) and loops forever. Used to prove the SRAM auto-save/restore path
// without depending on any commercial game's behavior.
function buildBatteryRom() {
  const rom = new Uint8Array(16 + 16384);
  rom[0] = 0x4e; rom[1] = 0x45; rom[2] = 0x53; rom[3] = 0x1a; // "NES\x1A"
  rom[4] = 1; // 1 x 16KB PRG
  rom[5] = 0; // 0 CHR (CHR RAM)
  rom[6] = 0x02; // battery-backed, mapper 0, horizontal mirroring
  // $C000: SEI, CLD, LDA #$42, STA $6000, JMP $C007 (self-loop); $C00A: RTI
  const code = [0x78, 0xd8, 0xa9, 0x42, 0x8d, 0x00, 0x60, 0x4c, 0x07, 0xc0, 0x40];
  rom.set(code, 16);
  const vec = 16 + 0x3ffa; // $FFFA
  rom[vec] = 0x0a; rom[vec + 1] = 0xc0; // NMI -> $C00A (RTI)
  rom[vec + 2] = 0x00; rom[vec + 3] = 0xc0; // RESET -> $C000
  rom[vec + 4] = 0x0a; rom[vec + 5] = 0xc0; // IRQ -> $C00A (RTI)
  return rom;
}

test.describe('Desktop UI interactions', () => {
  test('Keyboard controls: all eight buttons map to controller bits', async ({ page }) => {
    await boot(page);

    // [Playwright key, NES joypad bit]
    const cases = [
      ['ArrowUp', 0x10],
      ['ArrowDown', 0x20],
      ['ArrowLeft', 0x40],
      ['ArrowRight', 0x80],
      ['ControlLeft', 0x01], // A
      ['AltLeft', 0x02], // B
      ['Space', 0x04], // Select
      ['Enter', 0x08], // Start
    ];
    for (const [key, bit] of cases) {
      await page.keyboard.down(key);
      const held = await controllerState(page);
      expect(held & bit).toBe(bit);
      await page.keyboard.up(key);
      const released = await controllerState(page);
      expect(released & bit).toBe(0);
    }
  });

  test('Key remapping: rebind A to KeyW and verify old key stops working', async ({ page }) => {
    await boot(page);

    // Open the key config modal and remap A -> KeyW
    await page.click('#btn-open-key-config', { force: true });
    await expect(page.locator('#key-config-modal')).toBeVisible();
    await page.click('[data-button="A"]');
    await expect(page.locator('#kbd-A')).toHaveText('Listening...');
    await page.keyboard.press('KeyW');
    await expect(page.locator('#kbd-A')).toHaveText('W');
    await page.click('#btn-save-keys');
    await expect(page.locator('#key-config-modal')).toBeHidden();

    // Binding persisted to localStorage
    const bindings = await page.evaluate(() => JSON.parse(localStorage.getItem('nes_key_bindings')));
    expect(bindings.A).toBe('KeyW');

    // New key drives A; old key no longer does
    await page.keyboard.down('KeyW');
    expect((await controllerState(page)) & 0x01).toBe(0x01);
    await page.keyboard.up('KeyW');
    await page.keyboard.down('ControlLeft');
    expect((await controllerState(page)) & 0x01).toBe(0);
    await page.keyboard.up('ControlLeft');
  });

  test('Pause: Escape freezes frames, Resume restarts them', async ({ page }) => {
    await boot(page);

    await page.keyboard.press('Escape');
    // .hidden uses opacity, not display:none, so assert on the class.
    await expect(page.locator('#paused-overlay')).not.toHaveClass(/hidden/);
    const frozen = await page.evaluate(() => window.localFrameIndex);
    await page.waitForTimeout(600);
    expect(await page.evaluate(() => window.localFrameIndex)).toBe(frozen);

    await page.click('#btn-resume');
    await expect(page.locator('#paused-overlay')).toHaveClass(/hidden/);
    await page.waitForFunction((f) => window.localFrameIndex > f + 5, frozen, { timeout: 5000 });
  });

  test('Aspect ratio toggle switches Native 8:7 <-> CRT 4:3 canvas size', async ({ page }) => {
    await boot(page);

    // Default viewport 1280x720, Fit mode: Native 8:7 -> 768x672
    expect(await canvasSize(page)).toEqual({ width: '768px', height: '672px' });
    expect(await page.textContent('#btn-aspect-ratio')).toBe('8:7');

    await page.click('#btn-aspect-ratio', { force: true });
    expect(await page.textContent('#btn-aspect-ratio')).toBe('4:3');
    // Fit + CRT 4:3 -> 888x666
    expect(await canvasSize(page)).toEqual({ width: '888px', height: '666px' });

    await page.click('#btn-aspect-ratio', { force: true });
    expect(await page.textContent('#btn-aspect-ratio')).toBe('8:7');
    expect(await canvasSize(page)).toEqual({ width: '768px', height: '672px' });
  });

  test('Display size toggle cycles Fit -> 1x -> 2x -> 3x -> Fit', async ({ page }) => {
    await boot(page);

    expect(await page.textContent('#btn-display-size')).toBe('Fit');
    expect(await canvasSize(page)).toEqual({ width: '768px', height: '672px' });

    await page.click('#btn-display-size', { force: true });
    expect(await page.textContent('#btn-display-size')).toBe('1x');
    expect(await canvasSize(page)).toEqual({ width: '256px', height: '224px' });

    await page.click('#btn-display-size', { force: true });
    expect(await page.textContent('#btn-display-size')).toBe('2x');
    expect(await canvasSize(page)).toEqual({ width: '512px', height: '448px' });

    await page.click('#btn-display-size', { force: true });
    expect(await page.textContent('#btn-display-size')).toBe('3x');
    expect(await canvasSize(page)).toEqual({ width: '768px', height: '672px' });

    await page.click('#btn-display-size', { force: true });
    expect(await page.textContent('#btn-display-size')).toBe('Fit');
    expect(await canvasSize(page)).toEqual({ width: '768px', height: '672px' });
  });

  test('Graphics filter toggle switches Crisp <-> Smooth', async ({ page }) => {
    await boot(page);

    const canvas = page.locator('#nes-canvas');
    await expect(canvas).toHaveClass(/crisp/);
    await page.click('#btn-toggle-filter', { force: true });
    await expect(canvas).not.toHaveClass(/crisp/);
    await expect(page.locator('#btn-toggle-filter')).toHaveAttribute('title', /Switch to Crisp/);
    await page.click('#btn-toggle-filter', { force: true });
    await expect(canvas).toHaveClass(/crisp/);
    await expect(page.locator('#btn-toggle-filter')).toHaveAttribute('title', /Switch to Smooth/);
  });

  test('ROM loading via file picker', async ({ page }) => {
    await boot(page);

    await page.click('#btn-open-library', { force: true });
    await page.locator('#file-input').setInputFiles('tests/roms/nestest.nes');
    await page.waitForFunction(() => window.currentRomName === 'nestest', null, { timeout: 15000 });
    await page.waitForFunction(() => window.localFrameIndex > 5, null, { timeout: 10000 });
  });

  test('ROM loading via drag-and-drop onto the drop zone', async ({ page }) => {
    await boot(page);

    await page.click('#btn-open-library', { force: true });
    await page.evaluate(async () => {
      const response = await fetch('public/roms/flappy-bird.nes');
      const blob = await response.blob();
      const file = new File([blob], 'flappy-bird.nes', { type: 'application/octet-stream' });
      const dt = new DataTransfer();
      dt.items.add(file);
      document.getElementById('drop-zone').dispatchEvent(
        new DragEvent('drop', { dataTransfer: dt, bubbles: true, cancelable: true })
      );
    });
    // Drop handler names the ROM after the file (minus extension).
    await page.waitForFunction(() => window.currentRomName === 'flappy-bird', null, { timeout: 15000 });
    await page.waitForFunction(() => window.localFrameIndex > 5, null, { timeout: 10000 });
  });

  test('ROM switching via library: swap between default games', async ({ page }) => {
    await boot(page);
    await page.waitForFunction(() => window.currentRomName === 'Nova the Squirrel (Platformer)', null, { timeout: 15000 });

    await page.click('#btn-open-library', { force: true });
    await page.selectOption('#library-select', 'flappybird');
    await page.click('#btn-load-rom');
    await page.waitForFunction(() => window.currentRomName === 'Flappy Bird (Arcade)', null, { timeout: 15000 });
    await expect(page.locator('#rom-library-modal')).toBeHidden();
    await page.waitForFunction(() => window.localFrameIndex > 5, null, { timeout: 10000 });

    await page.click('#btn-open-library', { force: true });
    await page.selectOption('#library-select', 'novathesquirrel');
    await page.click('#btn-load-rom');
    await page.waitForFunction(() => window.currentRomName === 'Nova the Squirrel (Platformer)', null, { timeout: 15000 });
  });

  test('Save state persists to IndexedDB and loads back after reload', async ({ page }) => {
    const messages = collectConsole(page);
    await boot(page);

    await page.click('#btn-save-state', { force: true });
    await waitForMessage(messages, 'State saved successfully');
    const saved = await readStore(page, 'save_states');
    expect(saved.length).toBe(1);
    expect(saved[0].stateDataLength).toBeGreaterThan(0);

    // Fresh page load: the saved state must still be there and loadable.
    await page.reload({ waitUntil: 'networkidle' });
    await page.waitForSelector('#boot-btn', { state: 'visible' });
    await bootEmulator(page);

    await page.click('#btn-load-state', { force: true });
    await waitForMessage(messages, 'State loaded successfully');
    const afterLoad = await page.evaluate(() => window.localFrameIndex);
    await page.waitForFunction((f) => window.localFrameIndex > f + 5, afterLoad, { timeout: 5000 });
  });

  test('SRAM persistence: battery ROM auto-saves SRAM and restores it on reload', async ({ page }) => {
    const messages = collectConsole(page);
    const tempRomPath = path.join(process.cwd(), 'tests', 'temp_sram_test.nes');
    fs.writeFileSync(tempRomPath, buildBatteryRom());
    try {
      await boot(page);
      await page.locator('#file-input').setInputFiles(tempRomPath);
      await page.waitForFunction(() => window.currentRomName === 'temp_sram_test', null, { timeout: 15000 });

      // The ROM writes 0x42 to SRAM ($6000) on its first frames; the 5s
      // auto-save interval must persist it to IndexedDB.
      await expect.poll(async () => (await readStore(page, 'sram_saves')).length, { timeout: 30000 }).toBe(1);
      const records = await readStore(page, 'sram_saves');
      expect(records[0].romName).toBe('temp_sram_test');
      expect(records[0].sramFirstByte).toBe(0x42);

      // Reload, boot, load the same ROM again: SRAM must be restored.
      await page.reload({ waitUntil: 'networkidle' });
      await page.waitForSelector('#boot-btn', { state: 'visible' });
      await bootEmulator(page);
      await page.locator('#file-input').setInputFiles(tempRomPath);
      await waitForMessage(messages, 'Successfully restored SRAM save state');
    } finally {
      if (fs.existsSync(tempRomPath)) fs.unlinkSync(tempRomPath);
    }
  });

  test('Fullscreen toggle enters and exits fullscreen on the canvas wrapper', async ({ page }) => {
    await boot(page);

    await page.click('#btn-fullscreen', { force: true });
    await page.waitForFunction(() => !!document.fullscreenElement, null, { timeout: 5000 });
    expect(await page.evaluate(() => document.fullscreenElement.id)).toBe('canvas-wrapper');

    await page.click('#btn-fullscreen', { force: true });
    await page.waitForFunction(() => !document.fullscreenElement, null, { timeout: 5000 });
  });
});
