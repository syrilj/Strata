import { Save, Check, Loader2, AlertCircle } from 'lucide-react'
import { useDashboardStore } from '../store'
import { formatBytes, formatRelativeTime, cn } from '../lib/utils'
import type { Checkpoint } from '../types'

function CheckpointStatusIcon({ status }: { status: Checkpoint['status'] }) {
  switch (status) {
    case 'completed':
      return <Check className="w-3 h-3 text-emerald-400" aria-label="Completed" />
    case 'in_progress':
      return <Loader2 className="w-3 h-3 text-zinc-300 animate-spin" aria-label="In progress" />
    case 'failed':
      return <AlertCircle className="w-3 h-3 text-red-400" aria-label="Failed" />
  }
}

export function CheckpointList() {
  const { checkpoints } = useDashboardStore()

  const recentCheckpoints = checkpoints.slice(0, 8)

  return (
    <div className="card overflow-hidden">
      <div className="flex items-baseline justify-between gap-3 px-5 pt-5 pb-3">
        <div>
          <p className="eyebrow mb-1">Snapshots</p>
          <h2 className="h-display text-[15px]">Recent checkpoints</h2>
        </div>
        <span className="text-[11px] text-zinc-500 tabular">{checkpoints.length} total</span>
      </div>

      {recentCheckpoints.length === 0 ? (
        <p className="text-[13px] text-zinc-500 text-center px-5 pb-6 pt-2">No checkpoints yet</p>
      ) : (
        <div className="px-2.5 pb-3 space-y-0.5" role="list" aria-label="Checkpoint list">
          {recentCheckpoints.map((ckpt) => (
            <div
              key={ckpt.id}
              className={cn(
                'flex items-center gap-3 px-2.5 py-2 rounded-lg transition-colors duration-[150ms] hover:bg-white/[0.03]',
                ckpt.status === 'in_progress' && 'bg-white/[0.03]'
              )}
              role="listitem"
            >
              <div className="w-7 h-7 rounded-lg bg-white/[0.04] border border-white/[0.07] flex items-center justify-center flex-shrink-0">
                <Save className="w-3.5 h-3.5 text-zinc-400" aria-hidden="true" />
              </div>

              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-1.5">
                  <span className="text-[12.5px] font-semibold text-zinc-200 tabular font-mono">Step {ckpt.step}</span>
                  <CheckpointStatusIcon status={ckpt.status} />
                </div>
                <p className="text-[11.5px] text-zinc-500 truncate tabular">
                  Epoch {ckpt.epoch} · {formatBytes(ckpt.size * 1024 * 1024)}
                </p>
              </div>

              <span className="text-[11px] text-zinc-600 tabular shrink-0">{formatRelativeTime(ckpt.createdAt)}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
