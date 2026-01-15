import { Save, Check, Loader2, AlertCircle } from 'lucide-react'
import { useDashboardStore } from '../store'
import { formatBytes, formatRelativeTime, cn } from '../lib/utils'
import type { Checkpoint } from '../types'

function CheckpointStatusIcon({ status }: { status: Checkpoint['status'] }) {
  switch (status) {
    case 'completed':
      return <Check className="w-3 h-3 text-emerald-400" aria-label="Completed" />
    case 'in_progress':
      return <Loader2 className="w-3 h-3 text-blue-400 animate-spin" aria-label="In progress" />
    case 'failed':
      return <AlertCircle className="w-3 h-3 text-red-400" aria-label="Failed" />
  }
}

export function CheckpointList() {
  const { checkpoints } = useDashboardStore()
  
  const recentCheckpoints = checkpoints.slice(0, 8)
  
  return (
    <div className="card p-5">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-sm font-medium text-white">Recent Checkpoints</h2>
        <span className="text-xs text-zinc-500">{checkpoints.length} total</span>
      </div>
      
      {recentCheckpoints.length === 0 ? (
        <p className="text-sm text-zinc-500 text-center py-4">No checkpoints yet</p>
      ) : (
        <div className="space-y-2" role="list" aria-label="Checkpoint list">
          {recentCheckpoints.map((ckpt) => (
            <div
              key={ckpt.id}
              className={cn(
                'flex items-center gap-3 p-2 rounded-lg',
                ckpt.status === 'in_progress' ? 'bg-blue-500/10' : 'bg-zinc-800/30'
              )}
              role="listitem"
            >
              <div className="w-6 h-6 rounded bg-zinc-800 flex items-center justify-center flex-shrink-0">
                <Save className="w-3 h-3 text-zinc-400" aria-hidden="true" />
              </div>
              
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="text-xs font-medium text-zinc-300">Step {ckpt.step}</span>
                  <CheckpointStatusIcon status={ckpt.status} />
                </div>
                <p className="text-xs text-zinc-600 truncate">
                  Epoch {ckpt.epoch} • {formatBytes(ckpt.size * 1024 * 1024)}
                </p>
              </div>
              
              <span className="text-xs text-zinc-600">{formatRelativeTime(ckpt.createdAt)}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
