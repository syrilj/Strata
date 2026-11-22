import { useState } from 'react'
import { Save, Check, Loader2, AlertCircle, Trash2 } from 'lucide-react'
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
  const { checkpoints, deleteCheckpoint } = useDashboardStore()
  const [confirmId, setConfirmId] = useState<string | null>(null)
  const [deletingId, setDeletingId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const recentCheckpoints = checkpoints.slice(0, 8)

  const handleDelete = async (id: string) => {
    setDeletingId(id)
    setError(null)
    try {
      await deleteCheckpoint(id)
      setConfirmId(null)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete checkpoint')
    } finally {
      setDeletingId(null)
    }
  }

  return (
    <div className="card overflow-hidden">
      <div className="flex items-baseline justify-between gap-3 px-5 pt-5 pb-3">
        <div>
          <p className="eyebrow mb-1">Snapshots</p>
          <h2 className="h-display text-[15px]">Recent checkpoints</h2>
        </div>
        <span className="text-[11px] text-zinc-500 tabular">{checkpoints.length} total</span>
      </div>

      {error && (
        <p className="mx-5 mb-2 text-[12px] text-red-300 bg-red-500/[0.08] border border-red-500/20 rounded-lg px-3 py-2">
          {error}
        </p>
      )}

      {recentCheckpoints.length === 0 ? (
        <p className="text-[13px] text-zinc-500 text-center px-5 pb-6 pt-2">No checkpoints yet</p>
      ) : (
        <div className="px-2.5 pb-3 space-y-0.5" role="list" aria-label="Checkpoint list">
          {recentCheckpoints.map((ckpt) => {
            const isConfirming = confirmId === ckpt.id
            const isDeleting = deletingId === ckpt.id

            return (
              <div
                key={ckpt.id}
                className={cn(
                  'flex items-center gap-3 px-2.5 py-2 rounded-lg transition-colors duration-[150ms] hover:bg-white/[0.03] group',
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
                    Epoch {ckpt.epoch} · {formatBytes(ckpt.size)}
                  </p>
                </div>

                <div className="flex items-center gap-1.5 shrink-0">
                  {isConfirming ? (
                    <>
                      <button
                        type="button"
                        onClick={() => handleDelete(ckpt.id)}
                        disabled={isDeleting}
                        className="px-2 py-1 rounded-md text-[11px] font-medium bg-red-500/15 text-red-300 border border-red-500/30 hover:bg-red-500/25 transition-colors disabled:opacity-50"
                        aria-label={`Confirm delete checkpoint ${ckpt.id}`}
                      >
                        {isDeleting ? <Loader2 className="w-3 h-3 animate-spin" /> : 'Delete'}
                      </button>
                      <button
                        type="button"
                        onClick={() => setConfirmId(null)}
                        disabled={isDeleting}
                        className="px-2 py-1 rounded-md text-[11px] font-medium text-zinc-400 hover:text-zinc-200 hover:bg-white/[0.06] transition-colors"
                      >
                        Cancel
                      </button>
                    </>
                  ) : (
                    <>
                      <span className="text-[11px] text-zinc-600 tabular">{formatRelativeTime(ckpt.createdAt)}</span>
                      <button
                        type="button"
                        onClick={() => setConfirmId(ckpt.id)}
                        className="p-1.5 rounded-md text-zinc-600 hover:text-red-300 hover:bg-red-500/[0.08] opacity-70 group-hover:opacity-100 focus-visible:opacity-100 transition-all duration-[150ms]"
                        title="Delete checkpoint"
                        aria-label={`Delete checkpoint ${ckpt.id}`}
                      >
                        <Trash2 className="w-3.5 h-3.5" aria-hidden="true" />
                      </button>
                    </>
                  )}
                </div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
