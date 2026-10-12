import { test, expect } from '@playwright/test';
import * as cp from 'child_process';
import * as path from 'path';

const API_BASE = process.env.API_BASE_URL || process.env.BASE_URL || 'http://localhost:8080';

// Helper to ensure demo conversation data exists in SQLite for Inbox testing
function seedInboxThread() {
  try {
    const dbPath = path.resolve(__dirname, '../../moat_core/business.db');
    const sql = `
      INSERT OR IGNORE INTO business_external_contacts (id, name, email, phone, created_at, updated_at) 
      VALUES ('contact_demo', 'Alice Demo', 'alice@demo.io', '+1234567890', strftime('%s', 'now'), strftime('%s', 'now'));
      INSERT OR IGNORE INTO business_interaction_threads (id, contact_id, status, opportunity_status, created_at, updated_at, control_mode, assigned_operator, is_paused) 
      VALUES ('thread_demo', 'contact_demo', 'active', 'open', strftime('%s', 'now'), strftime('%s', 'now'), 'ai', NULL, 0);
      INSERT OR IGNORE INTO business_message_records (id, thread_id, sender, content, timestamp) 
      VALUES ('msg_demo_1', 'thread_demo', 'customer', 'Hello, interested in commercial real estate offerings', strftime('%s', 'now'));
    `;
    cp.execFileSync('sqlite3', [dbPath, sql]);
  } catch (err) {
    console.warn('Seeding thread note:', err);
  }
}

// Helper to login via the UI form
async function loginViaForm(page: any) {
  await page.goto('/login');
  await page.evaluate(() => localStorage.clear());
  await page.goto('/login');
  await page.fill('#username', 'owner');
  await page.fill('#password', 'admin');
  await page.click('button[type="submit"]');
  await expect(page.locator('#app-layout')).toBeVisible({ timeout: 15000 });
}

// Helper to ensure all business packages are installed
async function ensurePackagesInstalled(request: any) {
  const pkgs = ['sales-core-v1', 'real-estate-v1', 'notification-package-v1'];
  for (const pkg of pkgs) {
    await request.post(`${API_BASE}/platform/packages/install`, {
      data: { package_id: pkg }
    });
  }
}

test.describe('MoatAgeex UI Exhaustive Feature Verification', () => {

  test.beforeAll(() => {
    seedInboxThread();
  });

  // ---------------------------------------------------------------------------
  // 1. Authentication flow
  // ---------------------------------------------------------------------------
  test('1. Authentication flow: Login with owner/admin, verify session cookie and dashboard mount', async ({ page, context }) => {
    // Navigate to login
    await page.goto('/login');
    await page.evaluate(() => localStorage.clear());
    await page.goto('/login');

    // Verify form elements exist
    await expect(page.locator('#username')).toBeVisible();
    await expect(page.locator('#password')).toBeVisible();
    await expect(page.locator('button[type="submit"]')).toBeVisible();

    // Fill credentials and submit
    await page.fill('#username', 'owner');
    await page.fill('#password', 'admin');
    await page.click('button[type="submit"]');

    // Verify app-layout mount and dashboard heading
    await expect(page.locator('#app-layout')).toBeVisible({ timeout: 15000 });
    await expect(page.locator('h1:has-text("Dashboard Overview")')).toBeVisible({ timeout: 10000 });

    // Verify session cookie was set
    const cookies = await context.cookies();
    const sessionCookie = cookies.find(c => c.name === 'session_id');
    expect(sessionCookie).toBeDefined();
    expect(sessionCookie?.value.length).toBeGreaterThan(0);

    // Verify localStorage role and auth state
    const authFlag = await page.evaluate(() => localStorage.getItem('auth'));
    const roleFlag = await page.evaluate(() => localStorage.getItem('role'));
    expect(authFlag).toBe('true');
    expect(roleFlag).toBe('owner');
  });

  // ---------------------------------------------------------------------------
  // 2. Dashboard KPIs
  // ---------------------------------------------------------------------------
  test('2. Dashboard: Verify KPIs load from /api/dashboard/kpis without errors', async ({ page, context }) => {
    await loginViaForm(page);

    // Verify direct API response
    const kpiRes = await context.request.get(`${API_BASE}/api/dashboard/kpis`);
    expect(kpiRes.ok()).toBeTruthy();
    const kpiData = await kpiRes.json();
    expect(kpiData).toHaveProperty('active_now');

    // Verify UI KPI cards
    await page.goto('/');
    await expect(page.locator('h1:has-text("Dashboard Overview")')).toBeVisible({ timeout: 10000 });
    await expect(page.locator('text=Installed Packages')).toBeVisible();
    await expect(page.locator('text=Active Now')).toBeVisible();

    // Ensure error message is not rendered
    await expect(page.locator('text=Failed to load metrics')).not.toBeVisible();
  });

  // ---------------------------------------------------------------------------
  // 3. Package Marketplace
  // ---------------------------------------------------------------------------
  test('3. Package Marketplace: View cards, install packages, and verify dynamic sidebar links', async ({ page, context }) => {
    await loginViaForm(page);

    // Navigate to Marketplace
    await page.click('a[href="/marketplace"]');
    await expect(page.locator('h1:has-text("Package Marketplace")')).toBeVisible({ timeout: 10000 });

    // View package cards
    await expect(page.locator('div:has-text("sales-core-v1")').first()).toBeVisible();
    await expect(page.locator('div:has-text("real-estate-v1")').first()).toBeVisible();
    await expect(page.locator('div:has-text("notification-package-v1")').first()).toBeVisible();

    // Verify "Package Details" modal opens and inspect tabs
    const detailsButtons = page.locator('button:has-text("Package Details")');
    if (await detailsButtons.count() > 0) {
      await detailsButtons.first().click();
      await expect(page.locator('h2')).toBeVisible();
      // Click Capabilities tab
      await page.click('button:has-text("capabilities")');
      await expect(page.locator('text=Injected React Routes')).toBeVisible();
      // Click Registry tab
      await page.click('button:has-text("registry")');
      await expect(page.locator('text=Declared Event Sourcing Schema')).toBeVisible();
      // Close modal
      await page.click('button:has-text("Close Window")');
    }

    // Install all 3 packages
    await ensurePackagesInstalled(context.request);

    // Reload page to reflect dynamic routes and sidebar links
    await page.goto('/');
    await expect(page.locator('#app-layout')).toBeVisible({ timeout: 10000 });

    // Verify dynamic sidebar navigation links appear
    await expect(page.locator('a[href="/sales/leads"]')).toBeVisible({ timeout: 10000 });
    await expect(page.locator('a[href="/sales/opportunities"]')).toBeVisible({ timeout: 10000 });
    await expect(page.locator('a[href="/realestate/properties"]')).toBeVisible({ timeout: 10000 });
    await expect(page.locator('a[href="/realestate/offers"]')).toBeVisible({ timeout: 10000 });
    await expect(page.locator('a[href="/realestate/deals"]')).toBeVisible({ timeout: 10000 });
    await expect(page.locator('a[href="/operations/notifications"]')).toBeVisible({ timeout: 10000 });
  });

  // ---------------------------------------------------------------------------
  // 4. Dynamic Feature Pages - Sales
  // ---------------------------------------------------------------------------
  test('4a. Sales: Manage leads, qualify lead, and check opportunities', async ({ page, context }) => {
    await ensurePackagesInstalled(context.request);
    await loginViaForm(page);

    // Navigate to Leads via dynamic sidebar link
    await page.click('a[href="/sales/leads"]');
    await expect(page.locator('h1:has-text("Leads")')).toBeVisible({ timeout: 10000 });

    // Create a new lead
    const testLeadName = `Lead Jane ${Date.now()}`;
    await page.click('button:has-text("New Lead")');
    await expect(page.locator('h2:has-text("Create Lead")')).toBeVisible();
    await page.fill('input[placeholder="Name"]', testLeadName);
    await page.fill('input[placeholder="Email"]', 'jane.lead@example.com');
    await page.click('button[type="submit"]:has-text("Save")');

    // Verify lead appears in list
    await expect(page.locator(`text=${testLeadName}`)).toBeVisible({ timeout: 10000 });

    // Qualify the lead
    const leadRow = page.locator('div.border.rounded-lg').filter({ hasText: testLeadName });
    const qualifyBtn = leadRow.locator('button:has-text("Qualify")');
    await expect(qualifyBtn).toBeVisible({ timeout: 5000 });
    await qualifyBtn.click();
    await expect(page.getByText('Action completed successfully').first()).toBeVisible({ timeout: 5000 });

    // Navigate to Opportunities via sidebar link
    await page.click('a[href="/sales/opportunities"]');
    await expect(page.locator('h1:has-text("Opportunities")')).toBeVisible({ timeout: 10000 });

    // Create a new opportunity
    const testOppTitle = `Enterprise Contract ${Date.now()}`;
    const oppAmount = `${Math.floor(10000 + Math.random() * 89999)}`;
    await page.click('button:has-text("New Opportunity")');
    await expect(page.locator('h2:has-text("Create Opportunity")')).toBeVisible();
    await page.fill('input[name="title"]', testOppTitle);
    await page.fill('input[name="amount"]', oppAmount);
    await page.click('button[type="submit"]:has-text("Create")');

    // Verify opportunity appears in list
    const oppRow = page.locator('div.border.rounded-lg').filter({ hasText: testOppTitle });
    await expect(oppRow).toBeVisible({ timeout: 10000 });
    await expect(oppRow).toContainText(`$${oppAmount}`);
  });

  // ---------------------------------------------------------------------------
  // 4b. Dynamic Feature Pages - Real Estate
  // ---------------------------------------------------------------------------
  test('4b. Real Estate: Create property, verify details view, check offers and deals', async ({ page, context }) => {
    await ensurePackagesInstalled(context.request);
    await loginViaForm(page);

    // Navigate to Properties via sidebar
    await page.click('a[href="/realestate/properties"]');
    await expect(page.locator('h1:has-text("Properties")')).toBeVisible({ timeout: 10000 });

    // Create a new property
    const propTitle = `Luxury Penthouse ${Date.now()}`;
    const propAddress = '450 Ocean Drive, Miami FL';
    await page.click('button:has-text("New Property")');
    await expect(page.locator('h2:has-text("Create Property")')).toBeVisible();
    await page.fill('input[name="title"]', propTitle);
    await page.fill('input[name="address"]', propAddress);
    await page.click('button[type="submit"]:has-text("Save")');

    // Verify property appears in list
    await expect(page.locator(`text=${propTitle}`)).toBeVisible({ timeout: 10000 });

    // View property details
    const propRow = page.locator('div.border.rounded-lg').filter({ hasText: propTitle });
    await propRow.locator('button:has-text("View")').click();
    await expect(page.locator('h2:has-text("Property Details")')).toBeVisible();
    await expect(page.locator('p:has-text("Address:")')).toContainText(propAddress);
    await page.click('button:has-text("Close")');

    // Navigate to Offers via sidebar
    await page.click('a[href="/realestate/offers"]');
    await expect(page.locator('h1:has-text("Offers")')).toBeVisible({ timeout: 10000 });

    // Create a new offer
    const offerBuyer = `Arthur Pendelton ${Date.now()}`;
    await page.click('button:has-text("New Offer")');
    await expect(page.locator('h2:has-text("Create Offer")')).toBeVisible();
    // Select property
    const propSelect = page.locator('select');
    await propSelect.selectOption({ index: 1 });
    await page.fill('input[placeholder="Buyer Name"]', offerBuyer);
    await page.fill('input[placeholder="Offer Amount"]', '1750000');
    await page.click('button[type="submit"]:has-text("Save")');

    // Verify offer appears
    const offerRow = page.locator('div.border.rounded-lg').filter({ hasText: offerBuyer });
    await expect(offerRow).toBeVisible({ timeout: 10000 });
    await expect(offerRow).toContainText('$1750000');

    // Navigate to Deals via sidebar
    await page.click('a[href="/realestate/deals"]');
    await expect(page.locator('h1:has-text("Deals")')).toBeVisible({ timeout: 10000 });

    // Create a new deal
    const dealSeller = `Marina Group ${Date.now()}`;
    await page.click('button:has-text("New Deal")');
    await expect(page.locator('h2:has-text("Create Deal")')).toBeVisible();
    const dealSelect = page.locator('select');
    await dealSelect.selectOption({ index: 1 });
    await page.fill('input[placeholder="Buyer Name"]', offerBuyer);
    await page.fill('input[placeholder="Seller Name"]', dealSeller);
    await page.fill('input[placeholder="Final Price"]', '1700000');
    await page.click('button[type="submit"]:has-text("Save")');

    // Verify deal appears
    const dealRow = page.locator('div.border.rounded-lg').filter({ hasText: dealSeller });
    await expect(dealRow).toBeVisible({ timeout: 10000 });
    await expect(dealRow).toContainText('$1700000');
  });

  // ---------------------------------------------------------------------------
  // 4c. Dynamic Feature Pages - Notifications
  // ---------------------------------------------------------------------------
  test('4c. Notifications: Send test notification, check templates and history', async ({ page, context }) => {
    await ensurePackagesInstalled(context.request);
    await loginViaForm(page);

    // Navigate to Notification Center (both /notifications and /operations/notifications work)
    await page.goto('/notifications');
    await expect(page.locator('h1:has-text("Notification Center")')).toBeVisible({ timeout: 10000 });

    // Send a test notification
    const testMsg = `Critical security alert payload ${Date.now()}`;
    await page.fill('input[name="recipient"]', 'admin@moatageex.io');
    await page.fill('textarea[name="message"]', testMsg);
    await page.click('button[type="submit"]:has-text("Send Notification")');

    // Verify dispatch success toast
    await expect(page.getByText('Notification sent successfully').first()).toBeVisible({ timeout: 10000 });

    // Navigate to Templates
    await page.goto('/notifications/templates');
    await expect(page.locator('h1:has-text("Notification Templates")')).toBeVisible({ timeout: 10000 });

    // Create a new template
    const tmplName = `System Alert Tmpl ${Date.now()}`;
    await page.click('button:has-text("New Template")');
    await expect(page.locator('h2:has-text("Create Template")')).toBeVisible();
    await page.locator('input').nth(0).fill(tmplName);
    await page.locator('input').nth(1).fill('Notice: System Event Triggered');
    await page.locator('textarea').fill('This is a pre-defined system notification template.');
    await page.click('button[type="submit"]:has-text("Save Template")');

    // Verify template is rendered
    await expect(page.locator(`text=${tmplName}`)).toBeVisible({ timeout: 10000 });

    // Navigate to History
    await page.goto('/notifications/history');
    await expect(page.locator('h1:has-text("Notification History")')).toBeVisible({ timeout: 10000 });

    // Verify history contains records
    await expect(page.locator('.font-mono').first()).toBeVisible({ timeout: 10000 });
  });

  // ---------------------------------------------------------------------------
  // 5. Governance - Inbox
  // ---------------------------------------------------------------------------
  test('5a. Inbox: Verify conversations sidebar, active thread, and action buttons (Pause, Takeover)', async ({ page }) => {
    await loginViaForm(page);

    // Navigate to Inbox
    await page.goto('/inbox');
    await expect(page.locator('h2:has-text("Conversations")')).toBeVisible({ timeout: 10000 });

    // Wait for conversations list to render
    const threadItem = page.locator('text=Thread thread_demo').or(page.locator('div[class*="cursor-pointer"]:has-text("Thread")')).first();
    await expect(threadItem).toBeVisible({ timeout: 10000 });
    await threadItem.click();

    // Verify chat header mounts with action buttons
    const pauseBtn = page.locator('button:has-text("Pause AI"), button:has-text("Resume AI")').first();
    const takeoverBtn = page.locator('button:has-text("Takeover (Human)"), button:has-text("Release (AI)")').first();

    await expect(pauseBtn).toBeVisible({ timeout: 10000 });
    await expect(takeoverBtn).toBeVisible({ timeout: 10000 });

    // Test toggle pause
    await pauseBtn.click();
    await expect(page.getByText('AI automated response').first()).toBeVisible({ timeout: 10000 });

    // Test toggle takeover
    await takeoverBtn.click();
    await expect(page.getByText('takeover').or(page.getByText('released')).first()).toBeVisible({ timeout: 10000 });
  });

  // ---------------------------------------------------------------------------
  // 5. Governance - Approvals
  // ---------------------------------------------------------------------------
  test('5b. Approvals: Navigate to /approvals, check Pending and History tabs', async ({ page }) => {
    await loginViaForm(page);

    await page.goto('/approvals');
    await expect(page.locator('h1:has-text("Approvals & Operations Hub")')).toBeVisible({ timeout: 10000 });

    // Verify Pending Approvals tab
    const pendingTab = page.locator('button:has-text("Pending Approvals")');
    await expect(pendingTab).toBeVisible();

    // Switch to Approvals History tab
    const historyTab = page.locator('button:has-text("Approvals History")');
    await expect(historyTab).toBeVisible();
    await historyTab.click();

    // Verify history content loads
    await expect(page.locator('h3:has-text("History Clear"), h4:has-text("Client Information")').first()).toBeVisible({ timeout: 10000 });

    // Switch back to Pending tab
    await pendingTab.click();
    await expect(page.locator('h3:has-text("Inbox Zero"), h4:has-text("Client / User Profile")').first()).toBeVisible({ timeout: 10000 });
  });

  // ---------------------------------------------------------------------------
  // 5. Platform - Calendar
  // ---------------------------------------------------------------------------
  test('5c. Calendar: Navigate to /calendar, verify scheduled slots / empty state display', async ({ page }) => {
    await loginViaForm(page);

    await page.goto('/calendar');
    await expect(page.locator('h1:has-text("Calendar & Engagements")')).toBeVisible({ timeout: 10000 });

    // Verify empty state or booking cards
    const emptyState = page.locator('text=No Scheduled Appointments');
    const bookingCard = page.locator('text=Booking ID:');
    await expect(emptyState.or(bookingCard)).toBeVisible({ timeout: 10000 });
  });

  // ---------------------------------------------------------------------------
  // 5. Platform - Package Studio
  // ---------------------------------------------------------------------------
  test('5d. Package Studio: Navigate to /studio, verify all tabs and action triggers', async ({ page }) => {
    await loginViaForm(page);

    await page.goto('/studio');
    await expect(page.locator('h1:has-text("Package Studio")')).toBeVisible({ timeout: 10000 });

    // Tab 1: Builder
    await page.click('button:has-text("Builder")');
    await expect(page.locator('h2:has-text("Package Builder")')).toBeVisible();
    await page.click('button:has-text("Build Package via SDK")');
    await expect(page.locator('pre:has-text("Connecting to Runtime SDK")')).toBeVisible({ timeout: 10000 });

    // Tab 2: Validator
    await page.click('button:has-text("Validator")');
    await expect(page.locator('h2:has-text("Validator")')).toBeVisible();
    await page.click('button:has-text("Run Validation")');
    await expect(page.locator('pre:has-text("Running package validation")')).toBeVisible({ timeout: 10000 });

    // Tab 3: Simulator
    await page.click('button:has-text("Simulator")');
    await expect(page.locator('h2:has-text("Simulator")')).toBeVisible();
    await page.click('button:has-text("Run Simulator")');
    await expect(page.locator('pre:has-text("Running simulation")')).toBeVisible({ timeout: 10000 });

    // Tab 4: Publisher
    await page.click('button:has-text("Publisher")');
    await expect(page.locator('h2:has-text("Publisher")')).toBeVisible();
    await page.click('button:has-text("Publish Package")');
    await expect(page.locator('pre:has-text("Publishing to Marketplace")')).toBeVisible({ timeout: 10000 });
  });

  // ---------------------------------------------------------------------------
  // 5. Governance - Users
  // ---------------------------------------------------------------------------
  test('5e. Users: Verify default users from /api/system/users, create new user and verify rendered', async ({ page }) => {
    await loginViaForm(page);

    await page.goto('/users');
    await expect(page.locator('h1:has-text("User Directory")')).toBeVisible({ timeout: 10000 });

    // Verify default users from system
    await expect(page.locator('p.font-medium:has-text("owner")')).toBeVisible({ timeout: 10000 });
    await expect(page.locator('p.font-medium:has-text("operator")')).toBeVisible({ timeout: 10000 });
    await expect(page.locator('p.font-medium:has-text("viewer")')).toBeVisible({ timeout: 10000 });

    // Create a new user
    const newUsername = `auditor_${Date.now()}`;
    await page.click('button:has-text("Add User")');
    await expect(page.locator('h2:has-text("Add Platform User")')).toBeVisible();
    await page.locator('input').nth(0).fill(newUsername);
    await page.locator('input').nth(1).fill(`${newUsername}@moatageex.io`);
    await page.locator('select').selectOption('Operator');
    await page.click('button[type="submit"]:has-text("Create User")');

    // Verify newly created user is rendered in list
    await expect(page.locator(`p.font-medium:has-text("${newUsername}")`)).toBeVisible({ timeout: 10000 });
  });

  // ---------------------------------------------------------------------------
  // 5. Governance - Roles
  // ---------------------------------------------------------------------------
  test('5f. Roles: Verify Owner, Operator, Viewer and dynamic permissions', async ({ page }) => {
    await loginViaForm(page);

    await page.goto('/roles');
    await expect(page.locator('h1:has-text("Role Management")')).toBeVisible({ timeout: 10000 });

    // Verify role cards
    await expect(page.locator('div:has-text("Owner")').first()).toBeVisible();
    await expect(page.locator('div:has-text("Operator")').first()).toBeVisible();
    await expect(page.locator('div:has-text("Viewer")').first()).toBeVisible();

    // Verify dynamic package permissions section
    await expect(page.locator('text=Dynamic Package Permissions').first()).toBeVisible();
  });

  // ---------------------------------------------------------------------------
  // 5. Governance - Connections
  // ---------------------------------------------------------------------------
  test('5g. Connections: Verify WhatsApp Personal Account and Google Workspace cards', async ({ page }) => {
    await loginViaForm(page);

    await page.goto('/integrations');
    await expect(page.locator('h1:has-text("Workspace Connections")')).toBeVisible({ timeout: 10000 });

    // Verify WhatsApp card
    await expect(page.locator('div:has-text("WhatsApp Personal Account")').first()).toBeVisible();

    // Verify Google Workspace card
    await expect(page.locator('div:has-text("Google Workspace Integration")').first()).toBeVisible();
  });

  // ---------------------------------------------------------------------------
  // 5. Platform - Settings
  // ---------------------------------------------------------------------------
  test('5h. Settings: Verify General tab, AI Provider tab, Integrations tab, and save action', async ({ page }) => {
    await loginViaForm(page);

    await page.goto('/settings');
    await expect(page.locator('h1:has-text("System Settings")')).toBeVisible({ timeout: 10000 });

    // Tab 1: General
    await page.click('button:has-text("General")');
    await expect(page.locator('label:has-text("Tenant Display Name")')).toBeVisible();
    await page.click('button:has-text("Save Changes")');
    await expect(page.getByText('Settings saved successfully').first()).toBeVisible({ timeout: 10000 });

    // Tab 2: AI Provider
    await page.click('button:has-text("AI Provider")');
    await expect(page.locator('label:has-text("AI Provider")')).toBeVisible();
    await page.click('button:has-text("Save AI Configuration")');
    await expect(page.getByText('Settings saved successfully').first()).toBeVisible({ timeout: 10000 });

    // Tab 3: Integrations
    await page.click('button:has-text("Integrations")');
    await expect(page.locator('p.text-sm:has-text("Google Calendar")')).toBeVisible();
    await expect(page.locator('p.text-sm:has-text("WhatsApp")')).toBeVisible();
  });

  // ---------------------------------------------------------------------------
  // 5. Platform - Observability
  // ---------------------------------------------------------------------------
  test('5i. Observability: Verify Health, System Overview, Reliability, Jobs, Outbox, Audit, Events and Rebuild Projections', async ({ page }) => {
    await loginViaForm(page);

    await page.goto('/observability');
    await expect(page.locator('h1:has-text("Observability")')).toBeVisible({ timeout: 10000 });

    // Health tab
    await page.click('button:has-text("Health")');
    await expect(page.locator('text=System Status:')).toBeVisible();
    await expect(page.locator('text=System Overview')).toBeVisible();

    // Jobs tab
    await page.click('button:has-text("Jobs")');
    await expect(page.locator('h2:has-text("Background Jobs")')).toBeVisible();

    // Outbox tab
    await page.click('button:has-text("Outbox")');
    await expect(page.locator('h2:has-text("Outbox Messages")')).toBeVisible();

    // Audit Trail tab
    await page.click('button:has-text("Audit Trail")');
    await expect(page.locator('h2:has-text("Audit Trail")')).toBeVisible();

    // Events tab
    await page.click('button:has-text("Events")');
    await expect(page.locator('h2:has-text("Live Event Stream")')).toBeVisible();

    // Trigger Rebuild All Projections
    await page.click('button:has-text("Rebuild All Projections")');
    await expect(page.getByText('Projections rebuilt successfully').first()).toBeVisible({ timeout: 10000 });
  });

});
