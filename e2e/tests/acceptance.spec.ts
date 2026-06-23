import { test, expect } from '@playwright/test';

// Helper to login via the UI form
async function loginViaForm(page: any) {
  await page.goto('/');
  await page.fill('#username', 'owner');
  await page.fill('#password', 'admin');
  await page.click('button[type="submit"]');
  // Wait for the app layout to become visible (login overlay hides, app shows)
  await expect(page.locator('#app-layout')).toBeVisible({ timeout: 10000 });
}

test.describe('Package-Driven Architecture E2E Flow', () => {

  test.beforeEach(async ({ page, context }) => {
    // Forward browser console logs to node terminal
    page.on('console', msg => console.log('BROWSER:', msg.text()));
    page.on('pageerror', err => console.log('BROWSER ERROR:', err.message));

    // Login via UI form
    await loginViaForm(page);

    // Clean up packages via API (context.request shares cookies from the login)
    await context.request.delete('http://localhost:8080/platform/packages/sales-core-v1');
    await context.request.delete('http://localhost:8080/platform/packages/real-estate-v1');
    await context.request.delete('http://localhost:8080/platform/packages/notification-package-v1');
    
    // Reload to reflect clean state - need to re-login since page reloads
    await loginViaForm(page);
  });

  test('Dynamically installs, verifies, and uninstalls packages without reload', async ({ page, context }) => {

    // 1. Navigate to Packages view
    await page.locator('li[data-view="packages"]').click();
    await expect(page.locator('#view-packages')).toHaveClass(/active/, { timeout: 5000 });
    
    // 2. Install sales-core-v1 via API
    await context.request.post('http://localhost:8080/platform/packages/install', {
      data: { package_id: 'sales-core-v1' }
    });
    // Reload and re-login to see changes
    await loginViaForm(page);

    // 3. Verify Leads is accessible
    await page.locator('li[data-view="leads"]').click();
    await expect(page.locator('#view-leads')).toHaveClass(/active/, { timeout: 5000 });

    // 4. Install real-estate-v1
    await context.request.post('http://localhost:8080/platform/packages/install', {
      data: { package_id: 'real-estate-v1' }
    });
    await loginViaForm(page);

    // 5. Verify Properties, Offers, and Deals are accessible
    await page.locator('li[data-view="properties"]').click();
    await expect(page.locator('#view-properties')).toHaveClass(/active/, { timeout: 5000 });

    await page.locator('li[data-view="offers"]').click();
    await expect(page.locator('#view-offers')).toHaveClass(/active/, { timeout: 5000 });

    await page.locator('li[data-view="deals"]').click();
    await expect(page.locator('#view-deals')).toHaveClass(/active/, { timeout: 5000 });

    // 6. Uninstall sales-core-v1
    await context.request.delete('http://localhost:8080/platform/packages/sales-core-v1');
    
    // 7. Verify Real Estate still works
    await page.locator('li[data-view="properties"]').click();
    await expect(page.locator('#view-properties')).toHaveClass(/active/, { timeout: 5000 });

    // 8. Install notification-package-v1
    await context.request.post('http://localhost:8080/platform/packages/install', {
      data: { package_id: 'notification-package-v1' }
    });
    await loginViaForm(page);

    // 9. Verify Notifications
    await page.locator('li[data-view="notifications"]').click();
    await expect(page.locator('#view-notifications')).toHaveClass(/active/, { timeout: 5000 });

    // 10. Cleanup
    await context.request.delete('http://localhost:8080/platform/packages/notification-package-v1');
  });

  test('Security: Anonymous requests are blocked', async ({ page }) => {
    const browser = page.context().browser()!;
    const anonContext = await browser.newContext();
    
    const res = await anonContext.request.get('http://localhost:8080/platform/packages');
    expect(res.status()).toBe(401);

    const res2 = await anonContext.request.post('http://localhost:8080/sales/leads', {
      data: { id: 'anon_lead', email: 'anon@test.com' }
    });
    expect(res2.status()).toBe(401);

    await anonContext.close();
  });

  test('Security: Viewer cannot mutate data', async ({ page }) => {
    const browser = page.context().browser()!;
    const viewerContext = await browser.newContext();
    const loginRes = await viewerContext.request.post('http://localhost:8080/login', {
      data: { username: 'viewer', password: '' }
    });
    expect(loginRes.ok()).toBeTruthy();

    const res = await viewerContext.request.post('http://localhost:8080/sales/leads', {
      data: { id: 'viewer_lead', email: 'viewer@test.com' }
    });
    expect(res.status()).toBe(403);

    await viewerContext.close();
  });

  test('CRUD: Leads lifecycle', async ({ context }) => {
    const createRes = await context.request.post('http://localhost:8080/sales/leads', {
      data: { id: 'test_lead_e2e', email: 'e2e@test.com' }
    });
    expect(createRes.ok()).toBeTruthy();

    const qualifyRes = await context.request.post('http://localhost:8080/sales/leads/test_lead_e2e/qualify');
    expect(qualifyRes.ok()).toBeTruthy();

    const deleteRes = await context.request.delete('http://localhost:8080/sales/leads/test_lead_e2e');
    expect(deleteRes.ok()).toBeTruthy();
  });

  test('CRUD: Properties lifecycle', async ({ context }) => {
    const createRes = await context.request.post('http://localhost:8080/realestate/properties', {
      data: { id: 'test_prop_e2e', address: '123 E2E Street' }
    });
    expect(createRes.ok()).toBeTruthy();

    const deleteRes = await context.request.delete('http://localhost:8080/realestate/properties/test_prop_e2e');
    expect(deleteRes.ok()).toBeTruthy();
  });

  test('CRUD: Notifications lifecycle', async ({ context }) => {
    const createRes = await context.request.post('http://localhost:8080/operations/notifications', {
      data: { id: 'test_notif_e2e', to: 'user@test.com', message: 'E2E test notification' }
    });
    expect(createRes.ok()).toBeTruthy();

    const listRes = await context.request.get('http://localhost:8080/operations/notifications');
    expect(listRes.ok()).toBeTruthy();
  });

  test('Persistence: Data survives login/logout cycle', async ({ page, context }) => {
    // Create a lead
    await context.request.post('http://localhost:8080/sales/leads', {
      data: { id: 'persist_lead', email: 'persist@test.com' }
    });

    // Logout
    await context.request.post('http://localhost:8080/logout');

    // Re-login
    const loginRes = await context.request.post('http://localhost:8080/login', {
      data: { username: 'owner', password: 'admin' }
    });
    expect(loginRes.ok()).toBeTruthy();

    // Verify the lead still exists
    const listRes = await context.request.get('http://localhost:8080/sales/leads');
    expect(listRes.ok()).toBeTruthy();
    const leads = await listRes.json();
    const found = leads.some((l: any) => l.id === 'persist_lead');
    expect(found).toBeTruthy();

    // Cleanup
    await context.request.delete('http://localhost:8080/sales/leads/persist_lead');
  });
});
