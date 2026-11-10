import { Component, type ErrorInfo, type ReactNode } from "react";
import { describeThrown, errorDetails, reportError } from "../../lib/errorReport";
import { FallbackScreen } from "./FallbackScreen";

type State = { caught: { message: string; details: string } | null };

/**
 * Catches an error while drawing the window, logs it once, and shows the
 * fallback screen instead of a blank window (React 19 unmounts the root on an
 * uncaught render error).
 */
export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { caught: null };

  static getDerivedStateFromError(error: unknown): State {
    const described = describeThrown(error);
    return { caught: { message: described.message, details: errorDetails(described, null) } };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    const described = describeThrown(error);
    const componentStack = info.componentStack ?? null;
    reportError({ ...described, componentStack, source: "render" });
    this.setState({
      caught: { message: described.message, details: errorDetails(described, componentStack) },
    });
  }

  render() {
    const { caught } = this.state;
    if (caught) return <FallbackScreen message={caught.message} details={caught.details} />;
    return this.props.children;
  }
}
