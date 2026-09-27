import { Check, Loader2 } from 'lucide-react'
import { useDashboardStore } from '../store'
import { cn } from '../lib/utils'

export function BarrierStatus() {
  const { barriers } = useDashboardStore()

  if (barriers.length === 0) {
    return null
  }

  return (
    <div className="card p-5">
      <p className="eyebrow mb-1">Coordination</p>
      <h2 className="h-display text-[15px] mb-4">Barrier sync</h2>

      <div className="space-y-4" role="list" aria-label="Barrier synchronization status">
        {barriers.map((barrier) => {
          const progress = (barrier.arrived / barrier.total) * 100
          const isComplete = barrier.status === 'complete'

          return (
            <div key={barrier.id} className="space-y-2" role="listitem">
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-2 min-w-0">
                  {isComplete ? (
                    <Check className="w-3.5 h-3.5 text-emerald-400 shrink-0" aria-hidden="true" />
                  ) : (
                    <Loader2 className="w-3.5 h-3.5 text-zinc-300 animate-spin shrink-0" aria-hidden="true" />
                  )}
                  <span className="text-[13px] font-medium text-zinc-200 truncate">{barrier.name}</span>
                </div>
                <span className="text-[11.5px] text-zinc-500 tabular shrink-0 font-mono">
                  {barrier.arrived}/{barrier.total}
                </span>
              </div>

              <div className="h-1 bg-white/[0.07] rounded-full overflow-hidden">
                <div
                  className={cn(
                    'h-full rounded-full transition-all duration-300 ease-out',
                    isComplete ? 'bg-emerald-400' : 'bg-zinc-300'
                  )}
                  style={{ width: `${progress}%` }}
                  role="progressbar"
                  aria-valuenow={barrier.arrived}
                  aria-valuemin={0}
                  aria-valuemax={barrier.total}
                  aria-label={`${barrier.name} progress`}
                />
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}
