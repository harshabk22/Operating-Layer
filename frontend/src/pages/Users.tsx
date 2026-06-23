import { useState } from 'react';
import { Button } from '@/components/ui/button';

interface User {
  username: string;
  role: string;
  email: string;
  status: 'active' | 'suspended';
}

const INITIAL_USERS: User[] = [
  { username: 'owner', role: 'Owner', email: 'owner@moatageex.io', status: 'active' },
  { username: 'operator', role: 'Operator', email: 'operator@moatageex.io', status: 'active' },
  { username: 'viewer', role: 'Viewer', email: 'viewer@moatageex.io', status: 'active' }
];

export default function Users() {
  const [users, setUsers] = useState<User[]>(INITIAL_USERS);
  const [showModal, setShowModal] = useState(false);
  const [formData, setFormData] = useState({ username: '', role: 'Viewer', email: '' });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setUsers([...users, { ...formData, status: 'active' }]);
    setShowModal(false);
    setFormData({ username: '', role: 'Viewer', email: '' });
  };

  return (
    <div className="space-y-6">
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">User Directory</h1>
          <p className="text-muted-foreground text-sm">Manage user accounts, authentication profiles, and role assignments.</p>
        </div>
        <Button onClick={() => setShowModal(true)}>Add User</Button>
      </div>

      <div className="border rounded-md bg-card">
        <div className="p-4 space-y-4">
          {users.map(user => (
            <div key={user.username} className="flex justify-between items-center p-4 border rounded-lg hover:bg-accent/50 transition-colors">
              <div>
                <p className="font-medium text-sm">{user.username}</p>
                <p className="text-xs text-muted-foreground">{user.email}</p>
              </div>
              <div className="flex items-center gap-4">
                <span className="text-xs px-2 py-1 rounded bg-secondary font-medium">{user.role}</span>
                <span className="text-xs px-2 py-1 rounded bg-green-500/10 text-green-500 font-medium capitalize">
                  {user.status}
                </span>
              </div>
            </div>
          ))}
        </div>
      </div>

      {showModal && (
        <div className="fixed inset-0 bg-background/80 flex items-center justify-center z-50">
          <form className="bg-card p-6 rounded-lg w-[400px] border shadow-xl space-y-4" onSubmit={handleSubmit}>
            <h2 className="text-xl font-bold">Add Platform User</h2>
            <div className="space-y-2">
              <label className="text-sm font-medium">Username</label>
              <input required className="w-full p-2 border rounded-md bg-background text-sm" value={formData.username} onChange={e => setFormData({...formData, username: e.target.value})} />
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium">Email Address</label>
              <input required type="email" className="w-full p-2 border rounded-md bg-background text-sm" value={formData.email} onChange={e => setFormData({...formData, email: e.target.value})} />
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium">Role Assignment</label>
              <select className="w-full p-2 border rounded-md bg-background text-sm" value={formData.role} onChange={e => setFormData({...formData, role: e.target.value})}>
                <option value="Viewer">Viewer</option>
                <option value="Operator">Operator</option>
                <option value="Owner">Owner</option>
              </select>
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button type="button" variant="outline" onClick={() => setShowModal(false)}>Cancel</Button>
              <Button type="submit">Create User</Button>
            </div>
          </form>
        </div>
      )}
    </div>
  );
}
