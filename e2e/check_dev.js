const { chromium } = require('@playwright/test');
const fs = require('fs');

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
    errors.push(`Request failed: ${req.method()} ${req.url()} - ${req.failure().errorText}`);
  });

  console.log('Navigating to http://localhost:3000...');
  try {
    await page.goto('http://localhost:3000', { timeout: 10000 });
    await page.waitForTimeout(4000);
    
    const url = page.url();
    const content = await page.content();
    
    const report = {
      finalUrl: url,
      consoleLogs,
      errors,
      bodyHtml: content.substring(0, 1000)
    };
    
    fs.writeFileSync('/Users/harshavardhanbk/Desktop/MoatAgeex/e2e/dev_report.json', JSON.stringify(report, null, 2));
    console.log('Dev report written successfully.');
  } catch (err) {
    console.error('Error during navigation:', err);
  } finally {
    browser.close();
  }
}

run();
