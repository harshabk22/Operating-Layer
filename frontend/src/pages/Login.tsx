import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';

export default function Login() {
  const navigate = useNavigate();
  const [username, setUsername] = useState('owner');
  const [password, setPassword] = useState('admin');
  const [error, setError] = useState('');

  const handleLogin = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      const response = await fetch('/api/login', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ username, password }),
      });

      if (!response.ok) {
        throw new Error('Authentication failed');
      }

      const data = await response.json();
      localStorage.setItem('auth', 'true');
      localStorage.setItem('role', data.role.toLowerCase());
      navigate('/');
    } catch {
      setError('Invalid credentials or server unavailable.');
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-background p-4">
      <Card className="w-[400px]">
        <CardHeader>
          <CardTitle className="text-2xl font-bold text-center">MoatAgeex Platform</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleLogin} className="space-y-4">
            <div className="space-y-1">
              <label htmlFor="username" className="text-xs font-semibold text-muted-foreground uppercase">Username</label>
              <input
                id="username"
                type="text"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                className="w-full p-2 border rounded bg-background text-foreground text-sm"
                placeholder="Enter username"
                required
              />
            </div>
            <div className="space-y-1">
              <label htmlFor="password" className="text-xs font-semibold text-muted-foreground uppercase">Password</label>
              <input
                id="password"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className="w-full p-2 border rounded bg-background text-foreground text-sm"
                placeholder="Enter password"
                required
              />
            </div>
            {error && <p className="text-sm text-red-500 text-center">{error}</p>}
            <Button type="submit" className="w-full">Enter Platform</Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
