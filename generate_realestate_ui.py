import re

with open("moat_core/src/api/dashboard.html", "r") as f:
    content = f.read()

# Properties
prop_js = """
// Real Estate Module
async function fetchProperties() {
    const res = await fetch('/realestate/properties');
    if(res.ok) {
        const data = await res.json();
        const container = document.getElementById('properties-list');
        if(!container) return;
        container.innerHTML = data.map(p => `
            <div style="border:1px solid #444; padding:10px; margin:10px 0; border-radius:5px;">
                <p><strong>${p.id || p.address || 'Unknown Property'}</strong> - ${p.status || 'Active'}</p>
                <button onclick="deleteProperty('${p.id}')" style="background:#ef4444; color:white; border:none; padding:5px; cursor:pointer;">Delete</button>
            </div>
        `).join('');
    }
}
async function createProperty() {
    const address = prompt("Property Address:");
    if(!address) return;
    const res = await fetch('/realestate/properties', { method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify({id: 'prop_'+Date.now(), address})});
    if(res.ok) fetchProperties();
}
async function deleteProperty(id) {
    const res = await fetch(`/realestate/properties/${id}`, { method: 'DELETE' });
    if(res.ok) fetchProperties();
}
document.addEventListener('DOMContentLoaded', () => {
    const list = document.querySelector('.nav-list');
    if(list) {
        list.addEventListener('click', (e) => {
            const item = e.target.closest('.nav-item');
            if(item && item.dataset.view === 'properties') fetchProperties();
            if(item && item.dataset.view === 'offers') fetchOffers();
            if(item && item.dataset.view === 'deals') fetchDeals();
        });
    }
});
"""

prop_html = """<div id="view-properties" class="view-pane">
    <h2>Property Inventory</h2>
    <button onclick="createProperty()" style="background:#6366f1; color:white; border:none; padding:10px; cursor:pointer; margin-bottom:10px; border-radius:5px;">+ Add Property</button>
    <div id="properties-list"></div>
</div>"""

# Offers
offers_js = """
async function fetchOffers() {
    const res = await fetch('/realestate/offers');
    if(res.ok) {
        const data = await res.json();
        const container = document.getElementById('offers-list');
        if(!container) return;
        container.innerHTML = data.map(o => `
            <div style="border:1px solid #444; padding:10px; margin:10px 0; border-radius:5px;">
                <p><strong>Offer ${o.id || 'Unknown'}</strong> - Amount: ${o.amount || '0'}</p>
                <button onclick="deleteOffer('${o.id}')" style="background:#ef4444; color:white; border:none; padding:5px; cursor:pointer;">Delete</button>
            </div>
        `).join('');
    }
}
async function createOffer() {
    const amount = prompt("Offer Amount:");
    if(!amount) return;
    const res = await fetch('/realestate/offers', { method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify({id: 'offer_'+Date.now(), amount: parseFloat(amount)})});
    if(res.ok) fetchOffers();
}
async function deleteOffer(id) {
    const res = await fetch(`/realestate/offers/${id}`, { method: 'DELETE' });
    if(res.ok) fetchOffers();
}
"""

offers_html = """<div id="view-offers" class="view-pane">
    <h2>Offers Management</h2>
    <button onclick="createOffer()" style="background:#6366f1; color:white; border:none; padding:10px; cursor:pointer; margin-bottom:10px; border-radius:5px;">+ Create Offer</button>
    <div id="offers-list"></div>
</div>"""

# Deals
deals_js = """
async function fetchDeals() {
    const res = await fetch('/realestate/deals');
    if(res.ok) {
        const data = await res.json();
        const container = document.getElementById('deals-list');
        if(!container) return;
        container.innerHTML = data.map(d => `
            <div style="border:1px solid #444; padding:10px; margin:10px 0; border-radius:5px;">
                <p><strong>Deal ${d.id || 'Unknown'}</strong> - Value: ${d.value || '0'}</p>
                <button onclick="deleteDeal('${d.id}')" style="background:#ef4444; color:white; border:none; padding:5px; cursor:pointer;">Delete</button>
            </div>
        `).join('');
    }
}
async function createDeal() {
    const val = prompt("Deal Value:");
    if(!val) return;
    const res = await fetch('/realestate/deals', { method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify({id: 'deal_'+Date.now(), value: parseFloat(val)})});
    if(res.ok) fetchDeals();
}
async function deleteDeal(id) {
    const res = await fetch(`/realestate/deals/${id}`, { method: 'DELETE' });
    if(res.ok) fetchDeals();
}
"""

deals_html = """<div id="view-deals" class="view-pane">
    <h2>Deals Management</h2>
    <button onclick="createDeal()" style="background:#6366f1; color:white; border:none; padding:10px; cursor:pointer; margin-bottom:10px; border-radius:5px;">+ Create Deal</button>
    <div id="deals-list"></div>
</div>"""

# Substitutions
content = re.sub(r'<div id="view-properties" class="view-pane">.*?(?=<div id="view-)', prop_html + "\n            ", content, flags=re.DOTALL)
content = re.sub(r'<div id="view-offers" class="view-pane">.*?(?=<div id="view-)', offers_html + "\n            ", content, flags=re.DOTALL)
content = re.sub(r'<div id="view-deals" class="view-pane">.*?(?=<div id="view-)', deals_html + "\n            ", content, flags=re.DOTALL)

content = content.replace("</script>", prop_js + "\n" + offers_js + "\n" + deals_js + "\n</script>")

with open("moat_core/src/api/dashboard.html", "w") as f:
    f.write(content)
print("Updated Real Estate UI")
