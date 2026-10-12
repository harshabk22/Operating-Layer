import { useState } from 'react';
import { useData, useMutateData } from '@/lib/api';
import { Button } from '@/components/ui/button';

interface User {
  username: string;
  role: string;
  email: string;
  status: 'active' | 'suspended';
}

export default function Users() {
  const { data, isLoading, error } = useData('/api/system/users');
  const createMutation = useMutateData('/api/system/users', 'User created successfully');
  const [showModal, setShowModal] = useState(false);
  const [formData, setFormData] = useState({ username: '', role: 'Viewer', email: '' });

  const users: User[] = Array.isArray(data) ? data : (data ? Object.values(data) : []);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!formData.username.trim()) return;
    createMutation.mutate(
      {
        username: formData.username.trim(),
        role: formData.role,
        email: formData.email.trim(),
        status: 'active'
      },
      {
        onSuccess: () => {
          setShowModal(false);
          setFormData({ username: '', role: 'Viewer', email: '' });
        }
      }
    );
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
        {isLoading ? (
          <div className="p-8 text-center text-muted-foreground">Loading users...</div>
        ) : error ? (
          <div className="p-8 text-center text-destructive">Failed to load users.</div>
        ) : users.length === 0 ? (
          <div className="p-8 text-center text-muted-foreground">No users found.</div>
        ) : (
          <div className="p-4 space-y-4">
            {users.map(user => (
              <div key={user.username} className="flex justify-between items-center p-4 border rounded-lg hover:bg-accent/50 transition-colors">
                <div>
                  <p className="font-medium text-sm">{user.username}</p>
                  <p className="text-xs text-muted-foreground">{user.email || 'No email provided'}</p>
                </div>
                <div className="flex items-center gap-4">
                  <span className="text-xs px-2 py-1 rounded bg-secondary font-medium">{user.role}</span>
                  <span className={`text-xs px-2 py-1 rounded font-medium capitalize ${user.status === 'suspended' ? 'bg-destructive/10 text-destructive' : 'bg-green-500/10 text-green-500'}`}>
                    {user.status || 'active'}
                  </span>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {showModal && (
        <div className="fixed inset-0 bg-background/80 flex items-center justify-center z-50">
          <form className="bg-card p-6 rounded-lg w-[400px] border shadow-xl space-y-4" onSubmit={handleSubmit}>
            <h2 className="text-xl font-bold">Add Platform User</h2>
            <div className="space-y-2">
              <label className="text-sm font-medium">Username</label>
              <input
                required
                type="text"
                placeholder="Enter username"
                className="w-full p-2 border rounded-md bg-background text-sm"
                value={formData.username}
                onChange={e => setFormData({ ...formData, username: e.target.value })}
              />
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium">Email Address</label>
              <input
                required
                type="email"
                placeholder="Enter email address"
                className="w-full p-2 border rounded-md bg-background text-sm"
                value={formData.email}
                onChange={e => setFormData({ ...formData, email: e.target.value })}
              />
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium">Role Assignment</label>
              <select
                className="w-full p-2 border rounded-md bg-background text-sm"
                value={formData.role}
                onChange={e => setFormData({ ...formData, role: e.target.value })}
              >
                <option value="Viewer">Viewer</option>
                <option value="Operator">Operator</option>
                <option value="Owner">Owner</option>
              </select>
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button type="button" variant="outline" onClick={() => setShowModal(false)}>
                Cancel
              </Button>
              <Button type="submit" disabled={createMutation.isPending}>
                {createMutation.isPending ? 'Creating...' : 'Create User'}
              </Button>
            </div>
          </form>
        </div>
      )}
    </div>
  );
}
