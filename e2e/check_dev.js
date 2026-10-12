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

  const devUrl = process.env.DEV_URL || process.env.BASE_URL || 'http://localhost:3000';
  console.log(`Navigating to ${devUrl}...`);
  try {
    await page.goto(devUrl, { timeout: 10000 });
    await page.waitForTimeout(4000);
    
    const url = page.url();
    const content = await page.content();
    
    const report = {
      finalUrl: url,
      consoleLogs,
      errors,
      bodyHtml: content.substring(0, 1000)
    };
    
    const reportPath = path.join(__dirname, 'dev_report.json');
    fs.writeFileSync(reportPath, JSON.stringify(report, null, 2));
    console.log(`Dev report written successfully to ${reportPath}.`);
  } catch (err) {
    console.error('Error during navigation:', err);
  } finally {
    await browser.close();
  }
}

run();
