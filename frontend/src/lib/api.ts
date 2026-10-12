import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useToast } from '@/components/ui/use-toast';

export const fetcher = async (url: string) => {
  const res = await fetch(url);
  if (res.status === 401 || res.status === 403) {
    localStorage.removeItem('auth');
    localStorage.removeItem('role');
    if (window.location.pathname !== '/login') {
      window.location.assign('/login');
    }
    throw new Error('Unauthorized');
  }
  if (!res.ok) {
    let errorDetail: string;
    try {
      const errJson = await res.json();
      errorDetail = errJson.error || errJson.message || `HTTP ${res.status}`;
    } catch {
      errorDetail = res.statusText || `HTTP ${res.status}`;
    }
    throw new Error(errorDetail);
  }
  return res.json();
};

export function useData(endpoint: string, options?: { enabled?: boolean }) {
  return useQuery({
    queryKey: [endpoint],
    queryFn: () => fetcher(endpoint),
    enabled: options?.enabled ?? true,
  });
}

export function useMutateData(endpoint: string, successMessage: string) {
  const queryClient = useQueryClient();
  const { toast } = useToast();

  return useMutation({
    mutationFn: async (data: unknown) => {
      const res = await fetch(endpoint, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(data)
      });
      if (res.status === 401 || res.status === 403) {
        localStorage.removeItem('auth');
        localStorage.removeItem('role');
        if (window.location.pathname !== '/login') {
          window.location.assign('/login');
        }
        throw new Error('Unauthorized');
      }
      if (!res.ok) {
        let errorDetail: string;
        try {
          const errJson = await res.json();
          errorDetail = errJson.error || errJson.message || `HTTP ${res.status}`;
        } catch {
          errorDetail = res.statusText || `HTTP ${res.status}`;
        }
        throw new Error(errorDetail);
      }
      try {
        return await res.json();
      } catch {
        return null;
      }
    },
    onSuccess: () => {
      toast({ title: "Success", description: successMessage });
      queryClient.invalidateQueries();
    },
    onError: (error: Error) => {
      toast({
        title: "Error",
        description: error?.message || "Operation failed",
        variant: "destructive"
      });
    }
  });
}
