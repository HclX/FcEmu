import { test, expect } from '@playwright/test';
import fs from 'fs';
import path from 'path';

test.describe('Gameplay Replay E2E Verification', () => {

  test('Record gameplay and replay with bit-perfect determinism', async ({ page }) => {
    // Go to emulator page
    await page.goto('/');

    // Bind console logging to capture browser-side output in Node process
    page.on('console', msg => console.log(`[BROWSER] ${msg.type()}: ${msg.text()}`));
    page.on('pageerror', err => console.error(`[BROWSER ERROR] ${err.toString()}`));

    // 1. Boot the emulator
    await page.click('#boot-btn', { force: true });
    await expect(page.locator('#boot-overlay')).toHaveClass(/hidden/);

    // 2. Default ROM "novathesquirrel" auto-loads on startup, wait for it to start ticking
    await page.waitForFunction(() => window.localFrameIndex > 10, null, { timeout: 10000 });


    // Helper function to wait for a precise number of frames to execute
    async function waitForFrames(targetCount) {
      const startFrame = await page.evaluate(() => window.localFrameIndex);
      const targetFrame = startFrame + targetCount;
      await page.waitForFunction((target) => window.localFrameIndex >= target, targetFrame, { timeout: 10000 });
    }

    // 3. Start input recording
    console.log('Starting gameplay recording...');
    await page.click('#btn-record-toggle', { force: true });
    
    const recordStartFrame = await page.evaluate(() => window.localFrameIndex);

    // 4. Simulate gameplay: Move right, then left
    await page.keyboard.down('ArrowRight');
    await waitForFrames(60);
    await page.keyboard.up('ArrowRight');

    await page.keyboard.down('ArrowLeft');
    await waitForFrames(40);
    await page.keyboard.up('ArrowLeft');

    await waitForFrames(20); // Let it sit idle for 20 frames

    const recordEndFrame = await page.evaluate(() => window.localFrameIndex);
    console.log(`Recording done. Frames captured: ${recordEndFrame - recordStartFrame}`);

    // 5. Stop recording and intercept file download
    const tempFilePath = path.join(process.cwd(), 'tests', 'temp_replay.fcr');
    
    const [ download ] = await Promise.all([
      page.waitForEvent('download'),
      page.click('#btn-record-toggle', { force: true })
    ]);
    
    await download.saveAs(tempFilePath);
    expect(fs.existsSync(tempFilePath)).toBeTruthy();
    console.log(`Saved replay file to ${tempFilePath}`);

    // 6. Capture the golden baseline canvas hash at the end of the recorded run
    const goldenHash = await page.evaluate(async () => {
      const canvas = document.getElementById("nes-canvas");
      const ctx = canvas.getContext("2d");
      const imgData = ctx.getImageData(0, 0, canvas.width, canvas.height);
      const buffer = imgData.data.buffer;
      const hashBuffer = await crypto.subtle.digest("SHA-256", buffer);
      const hashArray = Array.from(new Uint8Array(hashBuffer));
      return hashArray.map(b => b.toString(16).padStart(2, "0")).join("");
    });
    console.log(`Golden visual hash: ${goldenHash}`);

    // 7. Perform a fresh reload of the page to ensure completely clean state
    console.log('Reloading page to start playback verification...');
    await page.reload({ waitUntil: 'networkidle' });

    // Wait for boot button to be visible and ready
    await page.waitForSelector('#boot-btn', { state: 'visible' });

    // 8. Re-boot emulator
    await page.click('#boot-btn', { force: true });
    await expect(page.locator('#boot-overlay')).toHaveClass(/hidden/);

    // 9. Default ROM "novathesquirrel" auto-loads on reload, wait for it to start ticking
    await page.waitForFunction(() => window.localFrameIndex > 10, null, { timeout: 10000 });

    // 10. Upload the `.fcr` replay file via the hidden input
    console.log('Uploading .fcr replay file...');
    const fileInput = page.locator('#replay-file-input');
    await fileInput.setInputFiles(tempFilePath);

    // Wait for replay mode to be active
    await page.waitForFunction(() => {
      const btnSpeed = document.getElementById('btn-replay-speed');
      return btnSpeed && btnSpeed.style.display !== 'none';
    }, null, { timeout: 5000 });

    // 11. Fast forward at 8x speed to make the test finish quickly!
    await page.click('#btn-replay-speed', { force: true }); // cycles to 2x
    await page.click('#btn-replay-speed', { force: true }); // cycles to 4x
    await page.click('#btn-replay-speed', { force: true }); // cycles to 8x
    
    const currentSpeedText = await page.textContent('#btn-replay-speed');
    expect(currentSpeedText).toContain('8.0x');
    console.log('Fast forwarding replay at 8x speed...');

    // 12. Wait for replay to auto-finish (when replay UI speed indicator goes away)
    await page.waitForFunction(() => {
      const btnSpeed = document.getElementById('btn-replay-speed');
      return !btnSpeed || btnSpeed.style.display === 'none';
    }, null, { timeout: 15000 });
    
    console.log('Replay completed. Fetching actual frame checksum...');

    // 13. Capture the actual frame visual hash after playback completed
    const actualHash = await page.evaluate(async () => {
      const canvas = document.getElementById("nes-canvas");
      const ctx = canvas.getContext("2d");
      const imgData = ctx.getImageData(0, 0, canvas.width, canvas.height);
      const buffer = imgData.data.buffer;
      const hashBuffer = await crypto.subtle.digest("SHA-256", buffer);
      const hashArray = Array.from(new Uint8Array(hashBuffer));
      return hashArray.map(b => b.toString(16).padStart(2, "0")).join("");
    });
    console.log(`Playback visual hash: ${actualHash}`);

    // 14. Assert that the final frame buffer matches the golden baseline perfectly!
    expect(actualHash).toBe(goldenHash);
    console.log('SUCCESS: Replay was 100% bit-perfect match with golden recorded run!');

    // Cleanup temp file
    fs.unlinkSync(tempFilePath);
  });

});
