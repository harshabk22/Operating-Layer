import { Component, type ErrorInfo, type ReactNode } from 'react';
import { AlertCircle, RefreshCw } from 'lucide-react';
import { Button } from './ui/button';

interface Props {
  children: ReactNode;
  fallbackTitle?: string;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  public override state: State = {
    hasError: false,
    error: null,
  };

  public static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  public override componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('Uncaught error in component tree:', error, errorInfo);
  }

  private handleReset = () => {
    this.setState({ hasError: false, error: null });
  };

  public override render() {
    if (this.state.hasError) {
      return (
        <div className="min-h-[400px] flex flex-col items-center justify-center p-8 text-center space-y-4 bg-card border rounded-lg m-4">
          <div className="p-4 bg-destructive/10 text-destructive rounded-full border border-destructive/20 shadow-inner">
            <AlertCircle className="h-8 w-8" />
          </div>
          <h2 className="text-xl font-bold tracking-tight">
            {this.props.fallbackTitle || 'Something went wrong'}
          </h2>
          <p className="text-sm text-muted-foreground max-w-md leading-relaxed">
            {this.state.error?.message || 'An unexpected rendering error occurred in this view.'}
          </p>
          <div className="flex gap-3">
            <Button variant="outline" onClick={this.handleReset} className="gap-2">
              <RefreshCw className="h-4 w-4" /> Try Again
            </Button>
            <Button onClick={() => window.location.assign('/')}>
              Return to Dashboard
            </Button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
