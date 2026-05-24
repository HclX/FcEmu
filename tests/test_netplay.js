import { test, expect } from '@playwright/test';

test.describe('Netplay E2E Tests', () => {

  test('Test 1: P2P Connection Setup & URL Auto-Join', async ({ browser }) => {
    // 1. Setup Host and Guest browser contexts
    const hostContext = await browser.newContext();
    const guestContext = await browser.newContext();

    const hostPage = await hostContext.newPage();
    const guestPage = await guestContext.newPage();

    await hostPage.goto('/');
    await guestPage.goto('/');

    // Boot emulator and open multiplayer modal lobby
    await hostPage.click('#boot-btn', { force: true });
    await guestPage.click('#boot-btn', { force: true });
    await hostPage.click('#btn-open-multiplayer', { force: true });
    await guestPage.click('#btn-open-multiplayer', { force: true });

    // 2. Click Host Game on Host
    await hostPage.click('#btn-host-game');

    // 3. Wait for and retrieve Peer ID
    await hostPage.waitForFunction(() => {
      const el = document.querySelector('#peer-id-input');
      return el && el.value && el.value.trim().length > 0;
    });
    const peerId = await hostPage.$eval('#peer-id-input', el => el.value);
    expect(peerId).toBeTruthy();

    // 4. Fill Peer ID on Guest and Join Game
    await guestPage.fill('#peer-id-input', peerId);
    await guestPage.click('#btn-join-game');

    // 5. Verify connected status on both browsers
    await expect(guestPage.locator('#connection-status')).toHaveText(/Connected to Player 2!|Connected/);

    // Click Disconnect on Guest to cleanly close WebRTC channel instantly
    await guestPage.click('#btn-join-game');

    // Close first guest context
    await guestContext.close();

    // Wait for Host to detect disconnection and return to Hosting state
    await expect(hostPage.locator('#connection-status')).toHaveText(/Hosting/);

    // 6. Test URL Room ID Auto-Joining
    const autoJoinContext = await browser.newContext();
    const autoJoinPage = await autoJoinContext.newPage();
    
    // Open the URL with the room parameter
    await autoJoinPage.goto(`/?room=${peerId}`);

    // Boot and open multiplayer lobby to verify auto-join status
    await autoJoinPage.click('#boot-btn', { force: true });
    await autoJoinPage.click('#btn-open-multiplayer', { force: true });
    
    // Verify auto-joining successfully establishes connection
    await expect(autoJoinPage.locator('#connection-status')).toHaveText(/Connected to Player 2!|Connected/);

    await hostContext.close();
    await guestContext.close();
    await autoJoinContext.close();
  });

  test('Test 2: Lockstep Sync & Frame Stepping', async ({ browser }) => {
    const hostContext = await browser.newContext();
    const guestContext = await browser.newContext();

    const hostPage = await hostContext.newPage();
    const guestPage = await guestContext.newPage();

    await hostPage.goto('/');
    await guestPage.goto('/');

    // Boot emulator and open multiplayer modal lobby
    await hostPage.click('#boot-btn', { force: true });
    await guestPage.click('#boot-btn', { force: true });
    await hostPage.click('#btn-open-multiplayer', { force: true });
    await guestPage.click('#btn-open-multiplayer', { force: true });

    // Establish P2P Connection
    await hostPage.click('#btn-host-game');
    await hostPage.waitForFunction(() => {
      const el = document.querySelector('#peer-id-input');
      return el && el.value && el.value.trim().length > 0;
    });
    const peerId = await hostPage.$eval('#peer-id-input', el => el.value);
    await guestPage.fill('#peer-id-input', peerId);
    await guestPage.click('#btn-join-game');

    await expect(hostPage.locator('#connection-status')).toHaveText(/Connected to Player 2!|Connected/);
    await expect(guestPage.locator('#connection-status')).toHaveText(/Connected to Player 2!|Connected/);

    // Verify localFrameIndex is exposed in global scope on both contexts
    const hostFrameInit = await hostPage.evaluate(() => window.localFrameIndex);
    const guestFrameInit = await guestPage.evaluate(() => window.localFrameIndex);
    expect(hostFrameInit).toBeDefined();
    expect(guestFrameInit).toBeDefined();

    // Simulate keyboard press on Host browser
    await hostPage.focus('#nes-canvas');
    await hostPage.keyboard.press('ArrowRight');

    // Assert both browsers advance in lockstep (equal frames or within minimal jitter threshold)
    await hostPage.waitForTimeout(100); // allow emulation step
    
    const hostFrameFinal = await hostPage.evaluate(() => window.localFrameIndex);
    const guestFrameFinal = await guestPage.evaluate(() => window.localFrameIndex);

    expect(Math.abs(hostFrameFinal - guestFrameFinal)).toBeLessThanOrEqual(2);

    await hostContext.close();
    await guestContext.close();
  });

  test('Test 3: Gamepad Input Mapping', async ({ browser }) => {
    const hostContext = await browser.newContext();
    const guestContext = await browser.newContext();

    const hostPage = await hostContext.newPage();
    const guestPage = await guestContext.newPage();

    // Inject Mock Gamepad API before loading page on Guest
    await guestPage.addInitScript(() => {
      const mockGamepad = {
        index: 1, // Player 2 is typically index 1
        id: 'Standard Gamepad (Controller 2)',
        connected: true,
        buttons: Array.from({ length: 16 }, () => ({ pressed: false, value: 0 })),
        axes: [0, 0],
        timestamp: Date.now()
      };
      window.mockGamepads = [null, mockGamepad];
      Object.defineProperty(navigator, 'getGamepads', {
        value: () => window.mockGamepads,
        writable: true
      });
    });

    await hostPage.goto('/');
    await guestPage.goto('/');

    // Boot emulator and open multiplayer modal lobby
    await hostPage.click('#boot-btn', { force: true });
    await guestPage.click('#boot-btn', { force: true });
    await hostPage.click('#btn-open-multiplayer', { force: true });
    await guestPage.click('#btn-open-multiplayer', { force: true });

    // Connect Guest to Host
    await hostPage.click('#btn-host-game');
    await hostPage.waitForFunction(() => {
      const el = document.querySelector('#peer-id-input');
      return el && el.value && el.value.trim().length > 0;
    });
    const peerId = await hostPage.$eval('#peer-id-input', el => el.value);
    await guestPage.fill('#peer-id-input', peerId);
    await guestPage.click('#btn-join-game');

    await expect(hostPage.locator('#connection-status')).toHaveText(/Connected to Player 2!|Connected/);

    // Check if mock gamepads are successfully injected and accessible on Guest
    const mockGamepadState = await guestPage.evaluate(() => {
      if (!window.mockGamepads) return "MISSING_MOCK_GAMEPADS";
      if (!window.mockGamepads[1]) return "MISSING_MOCK_GAMEPAD_P2";
      return {
        connected: window.mockGamepads[1].connected,
        buttonsLength: window.mockGamepads[1].buttons.length
      };
    });
    console.log("[Test 3 Diagnostic] Guest Page Mock Gamepad Status:", mockGamepadState);

    // Press button 0 (A button) on virtual gamepad 2 on Guest
    await guestPage.evaluate(() => {
      if (window.mockGamepads && window.mockGamepads[1]) {
        window.mockGamepads[1].buttons[0].pressed = true;
        window.mockGamepads[1].buttons[0].value = 1;
        window.mockGamepads[1].timestamp = Date.now();
      } else {
        throw new Error("Cannot press button: window.mockGamepads[1] is not initialized!");
      }
    });

    // Wait for inputs to propagate and execute
    await guestPage.waitForTimeout(300);

    // Verify button press is registered inside the emulator inputs
    const guestP2Input = await guestPage.evaluate(() => {
      // Inspect that Player 2 inputs contain BUTTON_A (0x01) bitmask
      return window.controller2State & 0x01;
    });
    expect(guestP2Input).toEqual(1);

    await hostContext.close();
    await guestContext.close();
  });

  test('Test 4: Buffering Spinner UI', async ({ browser }) => {
    const hostContext = await browser.newContext();
    const guestContext = await browser.newContext();

    const hostPage = await hostContext.newPage();
    const guestPage = await guestContext.newPage();

    await hostPage.goto('/');
    await guestPage.goto('/');

    // Boot emulator and open multiplayer modal lobby
    await hostPage.click('#boot-btn', { force: true });
    await guestPage.click('#boot-btn', { force: true });
    await hostPage.click('#btn-open-multiplayer', { force: true });
    await guestPage.click('#btn-open-multiplayer', { force: true });

    // Establish connection
    await hostPage.click('#btn-host-game');
    await hostPage.waitForFunction(() => {
      const el = document.querySelector('#peer-id-input');
      return el && el.value && el.value.trim().length > 0;
    });
    const peerId = await hostPage.$eval('#peer-id-input', el => el.value);
    await guestPage.fill('#peer-id-input', peerId);
    await guestPage.click('#btn-join-game');

    await expect(hostPage.locator('#connection-status')).toHaveText(/Connected to Player 2!|Connected/);

    // Assert spinner overlay is hidden initially
    await expect(hostPage.locator('#buffering-spinner')).toBeHidden();

    // Simulate interruption by setting the mock global pause hook
    await hostPage.evaluate(() => {
      window.pauseIncomingPackets = true;
    });

    // Verify that #buffering-spinner becomes visible on Host canvas
    await expect(hostPage.locator('#buffering-spinner')).toBeVisible();

    // Resume transmission
    await hostPage.evaluate(() => {
      window.pauseIncomingPackets = false;
    });

    // Assert spinner disappears and emulation resumes
    await expect(hostPage.locator('#buffering-spinner')).toBeHidden();

    await hostContext.close();
    await guestContext.close();
  });

});
