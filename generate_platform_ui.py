import re

with open("moat_core/src/api/dashboard.html", "r") as f:
    content = f.read()

# Nav Items
nav_items = """
                <li class="nav-item" data-view="packages">
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="margin-right: 0.5rem;"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/></svg>
                    Packages
                </li>
                <li class="nav-item" data-view="tenants">
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="margin-right: 0.5rem;"><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/></svg>
                    Tenants
                </li>
"""
content = re.sub(r'(<li class="nav-item" data-view="settings">)', nav_items + r'\1', content)

# JS
platform_js = """
// Platform Module
async function fetchPackages() {
    const res = await fetch('/platform/packages');
    if(res.ok) {
        const data = await res.json();
        const container = document.getElementById('packages-list');
        if(!container) return;
        container.innerHTML = data.map(p => `
            <div style="border:1px solid #444; padding:10px; margin:10px 0; border-radius:5px;">
                <p><strong>${p.id || p.name || 'Unknown'}</strong> - Version ${p.version || '1.0'}</p>
                <button onclick="installPackage('${p.id}')" style="background:#10b981; color:white; border:none; padding:5px; cursor:pointer;">Reinstall</button>
                <button onclick="deletePackage('${p.id}')" style="background:#ef4444; color:white; border:none; padding:5px; cursor:pointer;">Remove</button>
            </div>
        `).join('');
    }
}
async function installPackage(id) {
    let pkg_id = id;
    if(!pkg_id) { pkg_id = prompt("Package ID to Install:"); }
    if(!pkg_id) return;
    const res = await fetch('/platform/packages/install', { method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify({package_id: pkg_id})});
    if(res.ok) fetchPackages();
}
async function deletePackage(id) {
    const res = await fetch(`/platform/packages/${id}`, { method: 'DELETE' });
    if(res.ok) fetchPackages();
}

async function fetchTenants() {
    const res = await fetch('/platform/tenants');
    if(res.ok) {
        const data = await res.json();
        const container = document.getElementById('tenants-list');
        if(!container) return;
        container.innerHTML = data.map(t => `
            <div style="border:1px solid #444; padding:10px; margin:10px 0; border-radius:5px;">
                <p><strong>Tenant ${t.id || 'Unknown'}</strong> - Domain: ${t.domain || 'N/A'}</p>
                <button onclick="deleteTenant('${t.id}')" style="background:#ef4444; color:white; border:none; padding:5px; cursor:pointer;">Delete</button>
            </div>
        `).join('');
    }
}
async function deployTenant() {
    const tenantId = prompt("Tenant ID to Deploy:");
    if(!tenantId) return;
    const res = await fetch('/platform/tenants', { method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify({tenantId: tenantId})});
    if(res.ok) fetchTenants();
}
async function deleteTenant(id) {
    const res = await fetch(`/platform/tenants/${id}`, { method: 'DELETE' });
    if(res.ok) fetchTenants();
}

document.addEventListener('DOMContentLoaded', () => {
    const list = document.querySelector('.nav-list');
    if(list) {
        list.addEventListener('click', (e) => {
            const item = e.target.closest('.nav-item');
            if(item && item.dataset.view === 'packages') fetchPackages();
            if(item && item.dataset.view === 'tenants') fetchTenants();
        });
    }
});
"""

platform_html = """
            <div id="view-packages" class="view-pane">
                <h2>Packages Management</h2>
                <button onclick="installPackage()" style="background:#6366f1; color:white; border:none; padding:10px; cursor:pointer; margin-bottom:10px; border-radius:5px;">+ Install Package</button>
                <div id="packages-list"></div>
            </div>
            <div id="view-tenants" class="view-pane">
                <h2>Tenants Management</h2>
                <button onclick="deployTenant()" style="background:#6366f1; color:white; border:none; padding:10px; cursor:pointer; margin-bottom:10px; border-radius:5px;">+ Deploy Tenant</button>
                <div id="tenants-list"></div>
            </div>
"""
content = re.sub(r'(<div id="view-settings" class="view-pane">)', platform_html + r'\1', content)
content = content.replace("</script>", platform_js + "\n</script>")

with open("moat_core/src/api/dashboard.html", "w") as f:
    f.write(content)
print("Updated Platform UI")
