import type { ReactNode } from 'react';
import { AlertTriangle, RefreshCw } from 'lucide-react';

export function EmptyState({
  icon,
  title,
  hint,
  action,
}: {
  icon: ReactNode;
  title: string;
  hint?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center text-center py-10 px-6">
      <div className="w-11 h-11 rounded-[10px] bg-white/[0.04] border border-white/[0.08] flex items-center justify-center mb-3.5 text-zinc-300">
        {icon}
      </div>
      <p className="text-[14px] font-semibold text-zinc-100 tracking-[-0.01em]">{title}</p>
      {hint && <p className="text-[13px] text-zinc-500 mt-1.5 max-w-sm leading-relaxed">{hint}</p>}
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}

export function SkeletonCard() {
  return (
    <div className="card p-5 space-y-3" aria-hidden="true">
      <div className="skeleton h-3 w-24" />
      <div className="skeleton h-7 w-32" />
      <div className="skeleton h-3 w-full" />
    </div>
  );
}

export function SkeletonTable({ rows = 4 }: { rows?: number }) {
  return (
    <div className="space-y-2" aria-hidden="true" aria-label="Loading">
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="skeleton h-10 w-full" />
      ))}
    </div>
  );
}

export function ErrorBanner({
  message,
  onRetry,
}: {
  message: string;
  onRetry?: () => void;
}) {
  return (
    <div
      role="alert"
      className="flex items-start gap-3 px-4 py-3.5 rounded-[10px] bg-red-500/[0.06] border border-red-500/20 text-sm"
    >
      <AlertTriangle className="w-4 h-4 text-red-400 mt-0.5 shrink-0" aria-hidden="true" />
      <div className="flex-1 min-w-0">
        <p className="text-red-200 font-semibold text-[13.5px] tracking-[-0.01em]">Connection issue</p>
        <p className="text-red-200/60 text-[12.5px] mt-0.5 leading-relaxed">{message}</p>
      </div>
      {onRetry && (
        <button onClick={onRetry} className="btn-ghost !py-1.5 !px-3 text-xs shrink-0 pressable">
          <RefreshCw className="w-3.5 h-3.5" aria-hidden="true" />
          Retry
        </button>
      )}
    </div>
  );
}
