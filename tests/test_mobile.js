import { test, expect, devices } from '@playwright/test';

// Mobile-only tests — Pixel 5 landscape device emulation
// Note: Desktop redirect test is in test_desktop.js (separate file
// so this file's test.use() device emulation doesn't affect it)
test.use({
  ...devices['Pixel 5 landscape'],
});

test('Mobile user visiting / is automatically redirected to /mobile.html', async ({ page }) => {
  await page.goto('/');
  // URL should redirect to mobile.html
  await page.waitForURL(/\/mobile\.html/);
  expect(page.url()).toContain('mobile.html');
  
  // Verify touch overlay is visible
  await expect(page.locator('#touch-overlay')).toBeVisible();
});

test('Tapping touch-overlay boots emulator, sliding D-pad runs ROM', async ({ page }) => {
  // Go straight to mobile.html with a default ROM query
  await page.goto('/mobile.html?rom=novathesquirrel');

  // Bind console loggers
  page.on('console', msg => console.log(`[MOBILE BROWSER] ${msg.type()}: ${msg.text()}`));
  page.on('pageerror', err => console.error(`[MOBILE BROWSER ERROR] ${err.toString()}`));

  // Mock vibration and orientation locks to prevent environment exceptions
  await page.addInitScript(() => {
    navigator.vibrate = () => true;
    if (screen.orientation) {
      screen.orientation.lock = async () => true;
    }
  });

  // 1. Trigger touch overlay tap
  await page.tap('#touch-overlay');
  
  // The touch overlay should hide
  await expect(page.locator('#touch-overlay')).toHaveClass(/hidden/);

  // 2. Wait for emulator WASM to compile, mount, load ROM, and start ticking
  console.log('Waiting for mobile emulator to initialize and start frame ticks...');
  await page.waitForFunction(() => window.localFrameIndex > 15, null, { timeout: 12000 });
  
  const currentFrame = await page.evaluate(() => window.localFrameIndex);
  console.log(`Mobile emulator is successfully ticking! Frame: ${currentFrame}`);
  expect(currentFrame).toBeGreaterThan(15);

  // 3. Verify layout contains responsive canvas, dpad and action buttons
  await expect(page.locator('#emulator-canvas')).toBeVisible();
  await expect(page.locator('#virtual-dpad')).toBeVisible();
  await expect(page.locator('#btn-action-a')).toBeVisible();
  await expect(page.locator('#btn-action-b')).toBeVisible();

  // 4. Emulate Virtual D-Pad slide coordinate calculations
  // Fetch virtual-dpad bounding rect to compute coordinates
  const dpadRect = await page.locator('#virtual-dpad').boundingBox();
  expect(dpadRect).not.toBeNull();

  const centerX = dpadRect.x + dpadRect.width / 2;
  const centerY = dpadRect.y + dpadRect.height / 2;
  
  console.log(`D-pad bounds: center at (${centerX}, ${centerY})`);

  // Touch right side of D-pad (angle 0, should trigger NES_BUTTON_RIGHT = 0x80)
  const rightX = centerX + dpadRect.width * 0.35;
  const rightY = centerY;

  console.log(`Triggering TouchStart (RIGHT vector) on virtual-dpad...`);
  await page.dispatchEvent('#virtual-dpad', 'touchstart', {
    changedTouches: [{ identifier: 1, clientX: rightX, clientY: rightY }]
  });

  // Let it run with RIGHT held for 10 frames
  const startHoldFrame = await page.evaluate(() => window.localFrameIndex);
  await page.waitForFunction((target) => window.localFrameIndex >= target, startHoldFrame + 10, { timeout: 5000 });

  // Check if input state has NES_BUTTON_RIGHT (0x80) active
  const rightInputActive = await page.evaluate(() => {
    return (window.controllerState & 0x80) !== 0;
  });
  
  expect(rightInputActive).toBeTruthy();
  console.log(`Success: D-pad right coordinate calculation mapped to NES controller right!`);

  // Release D-pad
  await page.dispatchEvent('#virtual-dpad', 'touchend', {
    changedTouches: [{ identifier: 1, clientX: rightX, clientY: rightY }]
  });

  // Wait a couple frames for input release
  const startReleaseFrame = await page.evaluate(() => window.localFrameIndex);
  await page.waitForFunction((target) => window.localFrameIndex >= target, startReleaseFrame + 2, { timeout: 2000 });

  // Right input should now be inactive
  const rightInputReleased = await page.evaluate(() => {
    return (window.controllerState & 0x80) === 0;
  });
  expect(rightInputReleased).toBeTruthy();

  // 5. Emulate Button A touch (A = 0x01)
  const btnARect = await page.locator('#btn-action-a').boundingBox();
  expect(btnARect).not.toBeNull();
  const aCenterX = btnARect.x + btnARect.width / 2;
  const aCenterY = btnARect.y + btnARect.height / 2;

  console.log(`Triggering TouchStart on Button A...`);
  await page.dispatchEvent('#btn-action-a', 'touchstart', {
    changedTouches: [{ identifier: 2, clientX: aCenterX, clientY: aCenterY }]
  });

  const startButtonFrame = await page.evaluate(() => window.localFrameIndex);
  await page.waitForFunction((target) => window.localFrameIndex >= target, startButtonFrame + 5, { timeout: 5000 });

  // Check if Button A input is active
  const aInputActive = await page.evaluate(() => {
    return (window.controllerState & 0x01) !== 0;
  });
  expect(aInputActive).toBeTruthy();
  console.log(`Success: Button A touch mapped to NES controller A!`);

  await page.dispatchEvent('#btn-action-a', 'touchend', {
    changedTouches: [{ identifier: 2, clientX: aCenterX, clientY: aCenterY }]
  });

  console.log(`Touch gestures successfully emulated and routed!`);
});

test('Mobile B, Select, Start buttons and D-pad diagonal map to controller bits', async ({ page }) => {
  await page.goto('/mobile.html?rom=novathesquirrel');

  page.on('console', msg => console.log(`[MOBILE BROWSER] ${msg.type()}: ${msg.text()}`));
  page.on('pageerror', err => console.error(`[MOBILE BROWSER ERROR] ${err.toString()}`));

  await page.addInitScript(() => {
    navigator.vibrate = () => true;
    if (screen.orientation) {
      screen.orientation.lock = async () => true;
    }
  });

  await page.tap('#touch-overlay');
  await expect(page.locator('#touch-overlay')).toHaveClass(/hidden/);
  await page.waitForFunction(() => window.localFrameIndex > 15, null, { timeout: 12000 });

  // Helper: press a touch button, assert its bit is set while held, release, assert cleared.
  async function tapButton(selector, bit, label) {
    const rect = await page.locator(selector).boundingBox();
    expect(rect).not.toBeNull();
    const x = rect.x + rect.width / 2;
    const y = rect.y + rect.height / 2;

    await page.dispatchEvent(selector, 'touchstart', {
      changedTouches: [{ identifier: 7, clientX: x, clientY: y }]
    });
    const startFrame = await page.evaluate(() => window.localFrameIndex);
    await page.waitForFunction((target) => window.localFrameIndex >= target, startFrame + 3, { timeout: 5000 });
    const held = await page.evaluate(() => window.controllerState);
    expect(held & bit).toBe(bit);
    console.log(`Success: ${label} touch mapped to bit 0x${bit.toString(16)}`);

    await page.dispatchEvent(selector, 'touchend', {
      changedTouches: [{ identifier: 7, clientX: x, clientY: y }]
    });
    const releaseFrame = await page.evaluate(() => window.localFrameIndex);
    await page.waitForFunction((target) => window.localFrameIndex >= target, releaseFrame + 2, { timeout: 2000 });
    const released = await page.evaluate(() => window.controllerState);
    expect(released & bit).toBe(0);
  }

  // B (0x02), Select (0x04), Start (0x08)
  await tapButton('#btn-action-b', 0x02, 'Button B');
  await tapButton('#btn-mobile-select', 0x04, 'Select');
  await tapButton('#btn-mobile-start', 0x08, 'Start');

  // D-pad diagonal: touch at exactly -135 degrees from center (equal -x/-y
  // offsets) must map to UP (0x10) + LEFT (0x40) simultaneously.
  const dpadRect = await page.locator('#virtual-dpad').boundingBox();
  expect(dpadRect).not.toBeNull();
  const centerX = dpadRect.x + dpadRect.width / 2;
  const centerY = dpadRect.y + dpadRect.height / 2;
  const r = Math.min(dpadRect.width, dpadRect.height) * 0.3;
  const diagX = centerX - r;
  const diagY = centerY - r;

  await page.dispatchEvent('#virtual-dpad', 'touchstart', {
    changedTouches: [{ identifier: 8, clientX: diagX, clientY: diagY }]
  });
  const diagStartFrame = await page.evaluate(() => window.localFrameIndex);
  await page.waitForFunction((target) => window.localFrameIndex >= target, diagStartFrame + 3, { timeout: 5000 });
  const diagHeld = await page.evaluate(() => window.controllerState);
  expect(diagHeld & 0x10).toBe(0x10);
  expect(diagHeld & 0x40).toBe(0x40);
  console.log('Success: D-pad diagonal mapped to UP + LEFT!');

  await page.dispatchEvent('#virtual-dpad', 'touchend', {
    changedTouches: [{ identifier: 8, clientX: diagX, clientY: diagY }]
  });
  const diagReleaseFrame = await page.evaluate(() => window.localFrameIndex);
  await page.waitForFunction((target) => window.localFrameIndex >= target, diagReleaseFrame + 2, { timeout: 2000 });
  const diagReleased = await page.evaluate(() => window.controllerState);
  expect(diagReleased & (0x10 | 0x40)).toBe(0);
});
