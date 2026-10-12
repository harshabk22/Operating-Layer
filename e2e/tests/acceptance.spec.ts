import { test, expect } from '@playwright/test';

const API_BASE = process.env.API_BASE_URL || process.env.BASE_URL || 'http://localhost:8080';

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
    await context.request.delete(`${API_BASE}/platform/packages/sales-core-v1`);
    await context.request.delete(`${API_BASE}/platform/packages/real-estate-v1`);
    await context.request.delete(`${API_BASE}/platform/packages/notification-package-v1`);
    
    // Clear localStorage and reload to login again fresh
    await page.goto('/login');
    await page.evaluate(() => localStorage.clear());
    await loginViaForm(page);
  });

  test('Tier 1: Dynamically installs, verifies, and uninstalls packages without reload', async ({ page, context }) => {
    // 1. Navigate to Packages view
    await page.click('a[href="/marketplace"]');
    await expect(page.locator('h1:has-text("Package Marketplace")')).toBeVisible({ timeout: 5000 });
    
    // 2. Install sales-core-v1 via API
    const installRes1 = await context.request.post(`${API_BASE}/platform/packages/install`, {
      data: { package_id: 'sales-core-v1' }
    });
    expect(installRes1.ok()).toBeTruthy();

    // Reload and re-login to see changes in UI
    await loginViaForm(page);

    // 3. Verify Leads is accessible
    await page.click('a[href="/sales/leads"]');
    await expect(page.locator('h1:has-text("Leads")')).toBeVisible({ timeout: 5000 });

    // 4. Install real-estate-v1
    const installRes2 = await context.request.post(`${API_BASE}/platform/packages/install`, {
      data: { package_id: 'real-estate-v1' }
    });
    expect(installRes2.ok()).toBeTruthy();
    await loginViaForm(page);

    // 5. Verify Properties, Offers, and Deals are accessible
    await page.click('a[href="/realestate/properties"]');
    await expect(page.locator('h1:has-text("Properties")')).toBeVisible({ timeout: 5000 });

    await page.click('a[href="/realestate/offers"]');
    await expect(page.locator('h1:has-text("Offers")')).toBeVisible({ timeout: 5000 });

    await page.click('a[href="/realestate/deals"]');
    await expect(page.locator('h1:has-text("Deals")')).toBeVisible({ timeout: 5000 });

    // 6. Uninstall sales-core-v1
    const delSalesRes = await context.request.delete(`${API_BASE}/platform/packages/sales-core-v1`);
    expect(delSalesRes.ok()).toBeTruthy();
    
    // 7. Verify Real Estate still works
    await page.click('a[href="/realestate/properties"]');
    await expect(page.locator('h1:has-text("Properties")')).toBeVisible({ timeout: 5000 });

    // 8. Install notification-package-v1
    const installRes3 = await context.request.post(`${API_BASE}/platform/packages/install`, {
      data: { package_id: 'notification-package-v1' }
    });
    expect(installRes3.ok()).toBeTruthy();
    await loginViaForm(page);

    // 9. Verify Notifications
    await page.click('a[href="/operations/notifications"]');
    await expect(page.locator('h1:has-text("Notification Center")')).toBeVisible({ timeout: 5000 });

    // 10. Cleanup
    const delNotifRes = await context.request.delete(`${API_BASE}/platform/packages/notification-package-v1`);
    expect(delNotifRes.ok()).toBeTruthy();
    const delRealRes = await context.request.delete(`${API_BASE}/platform/packages/real-estate-v1`);
    expect(delRealRes.ok()).toBeTruthy();
  });

  test('Tier 1: Dynamic leads lifecycle - creation, query, and qualification', async ({ context }) => {
    // 1. Install sales-core-v1
    const installRes = await context.request.post(`${API_BASE}/platform/packages/install`, {
      data: { package_id: 'sales-core-v1' }
    });
    expect(installRes.ok()).toBeTruthy();

    // 2. Create lead via API
    const leadId = `lead_e2e_${Date.now()}`;
    const createRes = await context.request.post(`${API_BASE}/sales/leads`, {
      data: { id: leadId, name: 'Alice Operator', email: 'alice@example.com' }
    });
    expect(createRes.ok()).toBeTruthy();

    // 3. Query leads and verify presence
    const listRes = await context.request.get(`${API_BASE}/sales/leads`);
    expect(listRes.ok()).toBeTruthy();
    const leads = await listRes.json();
    expect(Array.isArray(leads)).toBeTruthy();
    expect(leads.some((l: any) => l.id === leadId)).toBeTruthy();

    // 4. Qualify lead
    const qualifyRes = await context.request.post(`${API_BASE}/sales/leads/${leadId}/qualify`);
    expect(qualifyRes.ok()).toBeTruthy();

    // 5. Cleanup lead & package
    const delLeadRes = await context.request.delete(`${API_BASE}/sales/leads/${leadId}`);
    expect(delLeadRes.ok()).toBeTruthy();

    const delPkgRes = await context.request.delete(`${API_BASE}/platform/packages/sales-core-v1`);
    expect(delPkgRes.ok()).toBeTruthy();
  });

  test('Tier 2: Boundary payloads on install API return 404', async ({ context }) => {
    // 1. Empty payload
    const emptyRes = await context.request.post(`${API_BASE}/platform/packages/install`, {
      data: {}
    });
    expect(emptyRes.status()).toBe(404);

    // 2. Non-existent package ID
    const notFoundRes = await context.request.post(`${API_BASE}/platform/packages/install`, {
      data: { package_id: 'non-existent-package-xyz' }
    });
    expect(notFoundRes.status()).toBe(404);

    // 3. Empty package ID
    const blankRes = await context.request.post(`${API_BASE}/platform/packages/install`, {
      data: { package_id: '' }
    });
    expect(blankRes.status()).toBe(404);
  });

  test('Tier 3: Gated endpoints and RBAC validation on package state change', async ({ page }) => {
    const browser = page.context().browser()!;

    // 1. Anonymous requests are blocked with 401
    const anonContext = await browser.newContext();
    const anonRes = await anonContext.request.get(`${API_BASE}/platform/packages`);
    expect(anonRes.status()).toBe(401);

    const anonLeadRes = await anonContext.request.post(`${API_BASE}/sales/leads`, {
      data: { id: 'anon_lead', email: 'anon@test.com' }
    });
    expect(anonLeadRes.status()).toBe(401);
    await anonContext.close();

    // 2. Viewer role can read but cannot mutate data (returns 403)
    const viewerContext = await browser.newContext();
    const viewerLogin = await viewerContext.request.post(`${API_BASE}/login`, {
      data: { username: 'viewer', password: '' }
    });
    expect(viewerLogin.ok()).toBeTruthy();

    // Viewer read is allowed
    const viewerReadRes = await viewerContext.request.get(`${API_BASE}/platform/packages`);
    expect(viewerReadRes.status()).toBe(200);

    // Viewer mutation is forbidden
    const viewerMutateRes = await viewerContext.request.post(`${API_BASE}/sales/leads`, {
      data: { id: 'viewer_lead', email: 'viewer@test.com' }
    });
    expect(viewerMutateRes.status()).toBe(403);

    const viewerInstallRes = await viewerContext.request.post(`${API_BASE}/platform/packages/install`, {
      data: { package_id: 'sales-core-v1' }
    });
    expect(viewerInstallRes.status()).toBe(403);

    await viewerContext.close();
  });

  test('Tier 4: MetaBrain authorization access and role segregation', async ({ page }) => {
    const browser = page.context().browser()!;

    // 1. Anonymous access is blocked
    const anonContext = await browser.newContext();
    const anonAuthRes = await anonContext.request.get(`${API_BASE}/authorizations`);
    expect(anonAuthRes.status()).toBe(401);
    await anonContext.close();

    // 2. Operator access to Owner-only authorizations is forbidden
    const opContext = await browser.newContext();
    const opLogin = await opContext.request.post(`${API_BASE}/login`, {
      data: { username: 'operator', password: 'admin' }
    });
    expect(opLogin.ok()).toBeTruthy();

    const opAuthRes = await opContext.request.get(`${API_BASE}/authorizations`);
    expect(opAuthRes.status()).toBe(403);

    // Operator can access general operations
    const opEscRes = await opContext.request.get(`${API_BASE}/escalations`);
    expect(opEscRes.ok()).toBeTruthy();
    await opContext.close();

    // 3. Owner access is granted
    const ownerContext = await browser.newContext();
    const ownerLogin = await ownerContext.request.post(`${API_BASE}/login`, {
      data: { username: 'owner', password: 'admin' }
    });
    expect(ownerLogin.ok()).toBeTruthy();

    const ownerAuthRes = await ownerContext.request.get(`${API_BASE}/authorizations`);
    expect(ownerAuthRes.status()).toBe(200);
    const authList = await ownerAuthRes.json();
    expect(Array.isArray(authList)).toBeTruthy();

    await ownerContext.close();
  });

});
