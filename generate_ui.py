import re

with open("moat_core/src/api/dashboard.html", "r") as f:
    content = f.read()

# JS logic to fetch and display Leads
leads_js = """
// Sales Module
async function fetchLeads() {
    const res = await fetch('/sales/leads');
    if(res.ok) {
        const data = await res.json();
        const container = document.getElementById('leads-list');
        if(!container) return;
        container.innerHTML = data.map(l => `
            <div style="border:1px solid #444; padding:10px; margin:10px 0; border-radius:5px;">
                <p><strong>${l.id || l.email}</strong> - ${l.status || 'New'}</p>
                <button onclick="qualifyLead('${l.id}')" style="background:#10b981; color:white; border:none; padding:5px; cursor:pointer;">Qualify</button>
                <button onclick="deleteLead('${l.id}')" style="background:#ef4444; color:white; border:none; padding:5px; cursor:pointer;">Delete</button>
            </div>
        `).join('');
    }
}
async function createLead() {
    const email = prompt("Lead Email:");
    if(!email) return;
    const res = await fetch('/sales/leads', { method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify({id: 'lead_'+Date.now(), email})});
    if(res.ok) fetchLeads();
}
async function qualifyLead(id) {
    const res = await fetch(`/sales/leads/${id}/qualify`, { method: 'POST' });
    if(res.ok) fetchLeads();
}
async function deleteLead(id) {
    const res = await fetch(`/sales/leads/${id}`, { method: 'DELETE' });
    if(res.ok) fetchLeads();
}
document.addEventListener('DOMContentLoaded', () => {
    // Hook into tab switching to fetch data
    const list = document.querySelector('.nav-list');
    if(list) {
        list.addEventListener('click', (e) => {
            const item = e.target.closest('.nav-item');
            if(item) {
                if(item.dataset.view === 'leads') fetchLeads();
            }
        });
    }
});
"""

# HTML replacement for Leads
leads_html = """<div id="view-leads" class="view-pane">
    <h2>Leads Management</h2>
    <button onclick="createLead()" style="background:#6366f1; color:white; border:none; padding:10px; cursor:pointer; margin-bottom:10px; border-radius:5px;">+ Create Lead</button>
    <div id="leads-list"></div>
</div>"""

content = re.sub(r'<div id="view-leads" class="view-pane">.*?</div>(?!</div>)', leads_html, content, flags=re.DOTALL)
content = content.replace("</script>", leads_js + "\n</script>")

with open("moat_core/src/api/dashboard.html", "w") as f:
    f.write(content)
print("Updated UI")
