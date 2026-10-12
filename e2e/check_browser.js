const { chromium } = require('@playwright/test');
const fs = require('fs');
const path = require('path');

async function run() {
  const browser = await chromium.launch();
  const page = await browser.newPage();
  
  const consoleLogs = [];
  const errors = [];
  
  page.on('console', msg => {
    consoleLogs.push(`[${msg.type()}] ${msg.text()}`);
  });
  
  page.on('pageerror', err => {
    errors.push(err.stack || err.message);
  });
  
  page.on('requestfailed', req => {
    errors.push(`Request failed: ${req.method()} ${req.url()} - ${req.failure()?.errorText || 'Unknown error'}`);
  });

  const baseUrl = process.env.BASE_URL || 'http://localhost:8080';
  console.log(`Navigating to ${baseUrl}...`);
  try {
    await page.goto(baseUrl, { timeout: 10000 });
    // Wait a bit for React to render and API requests to execute
    await page.waitForTimeout(4000);
    
    const url = page.url();
    const content = await page.content();
    
    const report = {
      finalUrl: url,
      consoleLogs,
      errors,
      bodyHtml: content.substring(0, 1000) // first 1000 chars of HTML
    };
    
    const reportPath = path.join(__dirname, 'browser_report.json');
    fs.writeFileSync(reportPath, JSON.stringify(report, null, 2));
    console.log(`Report written successfully to ${reportPath}.`);
  } catch (err) {
    console.error('Error during navigation:', err);
  } finally {
    await browser.close();
  }
}

run();
