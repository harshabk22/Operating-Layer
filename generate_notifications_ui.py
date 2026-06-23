import re

with open("moat_core/src/api/dashboard.html", "r") as f:
    content = f.read()

# Notifications Nav Item
nav_item = """
                <li class="nav-item" data-view="notifications">
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="margin-right: 0.5rem;"><path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"></path><path d="M13.73 21a2 2 0 0 1-3.46 0"></path></svg>
                    Notifications
                </li>
"""
content = re.sub(r'(<li class="nav-item" data-view="authorizations">)', nav_item + r'\1', content)

# Notifications UI pane
notif_js = """
// Notifications Module
async function fetchNotifications() {
    const res = await fetch('/operations/notifications');
    if(res.ok) {
        const data = await res.json();
        const container = document.getElementById('notifications-list');
        if(!container) return;
        container.innerHTML = data.map(n => `
            <div style="border:1px solid #444; padding:10px; margin:10px 0; border-radius:5px;">
                <p><strong>To: ${n.to || 'Unknown'}</strong> - ${n.status || 'Sent'}</p>
                <p>${n.message || 'No content'}</p>
            </div>
        `).join('');
    }
}
async function sendNotification() {
    const to = prompt("Recipient Email/Phone:");
    const msg = prompt("Message:");
    if(!to || !msg) return;
    const res = await fetch('/operations/notifications', { method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify({id: 'notif_'+Date.now(), to, message: msg})});
    if(res.ok) fetchNotifications();
}
document.addEventListener('DOMContentLoaded', () => {
    const list = document.querySelector('.nav-list');
    if(list) {
        list.addEventListener('click', (e) => {
            const item = e.target.closest('.nav-item');
            if(item && item.dataset.view === 'notifications') fetchNotifications();
        });
    }
});
"""

notif_html = """
            <div id="view-notifications" class="view-pane">
                <h2>Notifications</h2>
                <button onclick="sendNotification()" style="background:#6366f1; color:white; border:none; padding:10px; cursor:pointer; margin-bottom:10px; border-radius:5px;">+ Send Notification</button>
                <div id="notifications-list"></div>
            </div>
"""
content = re.sub(r'(<div id="view-authorizations" class="view-pane">)', notif_html + r'\1', content)
content = content.replace("</script>", notif_js + "\n</script>")

with open("moat_core/src/api/dashboard.html", "w") as f:
    f.write(content)
print("Updated Notifications UI")
