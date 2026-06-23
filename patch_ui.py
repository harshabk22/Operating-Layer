import re

with open("moat_core/src/api/dashboard.html", "r") as f:
    content = f.read()

# Replace Leads
leads_html = """<div id="view-leads" class="view-pane">
    <div class="panel-header">
        <h2>Sales Leads</h2>
        <button class="btn btn-primary" onclick="createLead()">+ New Lead</button>
    </div>
    <div id="leads-list" class="data-list">
        <!-- Leads will be injected here -->
    </div>
</div>
<script>
async function createLead() {
    const email = prompt("Enter lead email:");
    if(!email) return;
    const res = await fetch('/sales/leads', {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify({email: email})
    });
    if(res.ok) alert("Lead created!");
    else alert("Error creating lead");
}
</script>"""

content = re.sub(r'<div id="view-leads" class="view-pane">.*?</div></div>', leads_html, content, flags=re.DOTALL)

with open("moat_core/src/api/dashboard.html", "w") as f:
    f.write(content)
print("Patched leads")
