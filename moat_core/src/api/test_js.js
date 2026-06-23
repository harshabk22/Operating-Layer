const fs = require('fs');
const content = fs.readFileSync('/Users/harshavardhanbk/Desktop/MoatAgeex/moat_core/src/api/dashboard.html', 'utf-8');
const js = content.split('<script>')[1].split('</script>')[0];
fs.writeFileSync('temp.js', js);
