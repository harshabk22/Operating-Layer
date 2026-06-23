let name = "John O'Brien";
let s = `<div onclick="fn('${name.replace(/'/g, "\\\\'")}')">`;
console.log(s);
