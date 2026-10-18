import { expect } from '@playwright/test';

// Boot the desktop emulator on a page that is already at '/' (after
// goto or reload). The boot overlay auto-hides when the AudioContext
// can start without a user gesture (e.g. headless Chromium), so only
// click the boot button if the overlay is still shown — a force-click
// on the already-hidden button would land on the canvas underneath
// and pause the game.
export async function bootEmulator(page) {
  const overlayHidden = await page.locator('#boot-overlay').evaluate(
    (el) => el.classList.contains('hidden')
  );
  if (!overlayHidden) {
    await page.click('#boot-btn', { force: true });
  }
  await expect(page.locator('#boot-overlay')).toHaveClass(/hidden/);
  await page.waitForFunction(() => window.localFrameIndex > 10, null, { timeout: 15000 });
  // Remove keyboard focus from the boot button so Space/Enter don't re-click it.
  await page.evaluate(() => {
    if (document.activeElement) document.activeElement.blur();
  });
}
