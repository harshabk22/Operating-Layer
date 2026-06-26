const { chromium } = require('@playwright/test');
const fs = require('fs');

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

  console.log('Navigating to http://localhost:8080/login...');
  try {
    await page.goto('http://localhost:8080/login', { timeout: 10000 });
    
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
    
    fs.writeFileSync('/Users/harshavardhanbk/Desktop/MoatAgeex/e2e/login_flow_report.json', JSON.stringify(report, null, 2));
    console.log('Login flow report written successfully.');
  } catch (err) {
    console.error('Error during login flow:', err);
  } finally {
    await browser.close();
  }
}

run();
