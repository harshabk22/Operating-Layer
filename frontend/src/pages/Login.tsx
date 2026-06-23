import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';

export default function Login() {
  const navigate = useNavigate();
  const [role, setRole] = useState('owner');
  const [error, setError] = useState('');

  const handleLogin = async () => {
    try {
      const password = role === 'owner' ? 'owner' : 'operator';
      const response = await fetch('/api/login', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ username: role, password }),
      });

      if (!response.ok) {
        throw new Error('Authentication failed');
      }

      const data = await response.json();
      localStorage.setItem('auth', 'true');
      localStorage.setItem('role', data.role.toLowerCase());
      navigate('/');
    } catch (err) {
      setError('Invalid credentials or server unavailable.');
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-background p-4">
      <Card className="w-[400px]">
        <CardHeader>
          <CardTitle className="text-2xl font-bold text-center">MoatAgeex Platform</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-sm text-muted-foreground text-center">Select a role to login:</p>
          <select 
            value={role} 
            onChange={(e) => setRole(e.target.value)}
            className="w-full p-2 border rounded bg-background text-foreground"
          >
            <option value="owner">Owner</option>
            <option value="operator">Operator</option>
            <option value="viewer">Viewer</option>
          </select>
          {error && <p className="text-sm text-red-500 text-center">{error}</p>}
          <Button className="w-full" onClick={handleLogin}>Enter Platform</Button>
        </CardContent>
      </Card>
    </div>
  );
}
