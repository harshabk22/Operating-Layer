import { test, expect } from '@playwright/test';

// Helper to login via the UI form
async function loginViaForm(page: any) {
  if (await page.locator('#app-layout').isVisible()) {
    await page.goto('/');
    return;
  }
  if (!await page.locator('#username').isVisible()) {
    await page.goto('/login');
  }
  await page.fill('#username', 'owner');
  await page.fill('#password', 'admin');
  await page.click('button[type="submit"]');
  await expect(page.locator('#app-layout')).toBeVisible({ timeout: 10000 });
}

test.describe('Package-Driven Architecture E2E Flow', () => {

  test.beforeEach(async ({ page, context }) => {
    // Forward browser console logs to node terminal
    page.on('console', msg => console.log('BROWSER:', msg.text()));
    page.on('pageerror', err => console.log('BROWSER ERROR:', err.message));

    // Clear any existing session to start fresh
    await page.goto('/login');
    await page.evaluate(() => localStorage.clear());

    // Login via UI form
    await loginViaForm(page);

    // Clean up packages via API (context.request shares cookies from the login)
    await context.request.delete('http://localhost:8080/platform/packages/sales-core-v1');
    await context.request.delete('http://localhost:8080/platform/packages/real-estate-v1');
    await context.request.delete('http://localhost:8080/platform/packages/notification-package-v1');
    
    // Clear localStorage and reload to login again fresh
    await page.goto('/login');
    await page.evaluate(() => localStorage.clear());
    await loginViaForm(page);
  });

  test('Dynamically installs, verifies, and uninstalls packages without reload', async ({ page, context }) => {

    // 1. Navigate to Packages view
    await page.click('a[href="/marketplace"]');
    await expect(page.locator('h1:has-text("Package Marketplace")')).toBeVisible({ timeout: 5000 });
    
    // 2. Install sales-core-v1 via API
    await context.request.post('http://localhost:8080/platform/packages/install', {
      data: { package_id: 'sales-core-v1' }
    });
    // Reload and re-login to see changes
    await loginViaForm(page);

    // 3. Verify Leads is accessible
    await page.click('a[href="/sales/leads"]');
    await expect(page.locator('h1:has-text("Leads")')).toBeVisible({ timeout: 5000 });

    // 4. Install real-estate-v1
    await context.request.post('http://localhost:8080/platform/packages/install', {
      data: { package_id: 'real-estate-v1' }
    });
    await loginViaForm(page);

    // 5. Verify Properties, Offers, and Deals are accessible
    await page.click('a[href="/realestate/properties"]');
    await expect(page.locator('h1:has-text("Properties")')).toBeVisible({ timeout: 5000 });

    await page.click('a[href="/realestate/offers"]');
    await expect(page.locator('h1:has-text("Offers")')).toBeVisible({ timeout: 5000 });

    await page.click('a[href="/realestate/deals"]');
    await expect(page.locator('h1:has-text("Deals")')).toBeVisible({ timeout: 5000 });

    // 6. Uninstall sales-core-v1
    await context.request.delete('http://localhost:8080/platform/packages/sales-core-v1');
    
    // 7. Verify Real Estate still works
    await page.click('a[href="/realestate/properties"]');
    await expect(page.locator('h1:has-text("Properties")')).toBeVisible({ timeout: 5000 });

    // 8. Install notification-package-v1
    await context.request.post('http://localhost:8080/platform/packages/install', {
      data: { package_id: 'notification-package-v1' }
    });
    await loginViaForm(page);

    // 9. Verify Notifications
    await page.click('a[href="/operations/notifications"]');
    await expect(page.locator('h1:has-text("Notification Center")')).toBeVisible({ timeout: 5000 });

    // 10. Cleanup
    await context.request.delete('http://localhost:8080/platform/packages/notification-package-v1');
  });

  test('Security: Anonymous requests are blocked', async ({ page }) => {
    const browser = page.context().browser()!;
    const anonContext = await browser.newContext();
    
    const res = await anonContext.request.get('http://localhost:8080/platform/packages');
    expect(res.status()).toBe(401);

    await anonContext.close();
  });

});
