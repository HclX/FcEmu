import { test, expect, devices } from '@playwright/test';

// Desktop-only test — uses default browser viewport (no mobile emulation)
test('Desktop user remains on desktop /index.html without redirect', async ({ page }) => {
  await page.goto('/');
  // Should not contain mobile.html in the URL
  expect(page.url()).not.toContain('mobile.html');
  // The desktop boot button should be visible
  await expect(page.locator('#boot-btn')).toBeVisible();
});
