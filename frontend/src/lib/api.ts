import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useToast } from '@/components/ui/use-toast';

export const fetcher = async (url: string) => {
  const res = await fetch(url);
  if (res.status === 401 || res.status === 403) {
    localStorage.removeItem('auth');
    localStorage.removeItem('role');
    window.location.href = '/login';
    throw new Error('Unauthorized');
  }
  if (!res.ok) throw new Error('Network error');
  return res.json();
};

export function useData(endpoint: string) {
  return useQuery({ queryKey: [endpoint], queryFn: () => fetcher(endpoint) });
}

export function useMutateData(endpoint: string, successMessage: string) {
  const queryClient = useQueryClient();
  const { toast } = useToast();

  return useMutation({
    mutationFn: async (data: any) => {
      const res = await fetch(endpoint, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(data)
      });
      if (res.status === 401 || res.status === 403) {
        localStorage.removeItem('auth');
        localStorage.removeItem('role');
        window.location.href = '/login';
        throw new Error('Unauthorized');
      }
      if (!res.ok) throw new Error('Failed');
      return res.json();
    },
    onSuccess: () => {
      toast({ title: "Success", description: successMessage });
      queryClient.invalidateQueries();
    },
    onError: () => {
      toast({ title: "Error", description: "Operation failed", variant: "destructive" });
    }
  });
}
