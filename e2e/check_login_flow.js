const { chromium } = require('@playwright/test');
const fs = require('fs');
const path = require('path');

async function run() {
  const browser = await chromium.launch();
  const page = await browser.newPage();
  
  const consoleLogs = [];
  const errors = [];
  const networkRequests = [];
  
  page.on('console', msg => {
    consoleLogs.push(`[${msg.type()}] ${msg.text()}`);
  });
  
  page.on('pageerror', err => {
    errors.push(err.stack || err.message);
  });
  
  page.on('request', req => {
    networkRequests.push({
      url: req.url(),
      method: req.method(),
      headers: req.headers()
    });
  });

  page.on('response', async res => {
    console.log(`Response: ${res.status()} ${res.url()}`);
    if (res.url().includes('/api/')) {
      try {
        const text = await res.text();
        console.log(`  Body: ${text.substring(0, 200)}`);
      } catch (e) {
        console.log(`  Body: (could not read body)`);
      }
    }
  });

  const baseUrl = process.env.BASE_URL || 'http://localhost:8080';
  console.log(`Navigating to ${baseUrl}/login...`);
  try {
    await page.goto(`${baseUrl}/login`, { timeout: 10000 });
    
    // Log in
    console.log('Entering login credentials...');
    await page.fill('#username', 'owner');
    await page.fill('#password', 'admin');
    await page.click('button[type="submit"]');
    
    console.log('Waiting 5 seconds...');
    await page.waitForTimeout(5000);
    
    const url = page.url();
    const content = await page.content();
    
    const report = {
      finalUrl: url,
      consoleLogs,
      errors,
      networkRequests,
      bodyHtml: content.substring(0, 1000)
    };
    
    const reportPath = path.join(__dirname, 'login_flow_report.json');
    fs.writeFileSync(reportPath, JSON.stringify(report, null, 2));
    console.log(`Login flow report written successfully to ${reportPath}.`);
  } catch (err) {
    console.error('Error during login flow:', err);
  } finally {
    await browser.close();
  }
}

run();
