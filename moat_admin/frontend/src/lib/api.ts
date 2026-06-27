import axios, { AxiosError } from 'axios';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';

// Base API instance
export const apiClient = axios.create({
  baseURL: '/api/admin',
  headers: {
    'Content-Type': 'application/json',
  },
});

// Configure Axios response interceptors
apiClient.interceptors.response.use(
  (response) => response,
  (error: AxiosError) => {
    const status = error.response?.status;
    const data = error.response?.data as any;

    if (status === 401) {
      // Unauthorized: clear storage and redirect
      if (typeof window !== 'undefined') {
        localStorage.removeItem('moat_admin_user');
        window.location.href = '/login';
      }
    } else if (status === 403) {
      console.error('Forbidden: You do not have permissions for this action.');
      alert('Access Denied: You do not have the required role to perform this action.');
    } else if (status === 422) {
      console.error('Validation Error:', data?.errors || data || 'Unprocessable entity');
      alert(`Validation Error: ${JSON.stringify(data?.errors || data)}`);
    } else if (status === 500) {
      console.error('Internal Server Error:', data?.message || 'Server error');
      alert('Server Error: An internal error occurred. Please try again later.');
    }

    return Promise.reject(error);
  }
);

// Generic query helper hook
export function useAdminQuery<T>(key: string[], url: string, enabled = true) {
  return useQuery<T>({
    queryKey: key,
    queryFn: async () => {
      const response = await apiClient.get<T>(url);
      return response.data;
    },
    enabled,
  });
}

// Generic mutation helper hook
export function useAdminMutation<TVariables, TData>(
  url: string,
  method: 'POST' | 'PUT' | 'DELETE' | 'PATCH' = 'POST',
  onSuccessCallback?: (data: TData) => void
) {
  const queryClient = useQueryClient();

  return useMutation<TData, Error, TVariables>({
    mutationFn: async (variables: TVariables) => {
      const isOverride = variables && typeof variables === 'object' && 'url' in variables;
      const config = isOverride
        ? {
            url: (variables as any).url,
            method: (variables as any).method || method,
            data: (variables as any).data,
          }
        : {
            url,
            method,
            data: variables,
          };
      const response = await apiClient<TData>(config);
      return response.data;
    },
    onSuccess: (data) => {
      queryClient.invalidateQueries();
      if (onSuccessCallback) {
        onSuccessCallback(data);
      }
    },
  });
}
