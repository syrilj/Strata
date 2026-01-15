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
      <h2 className="text-sm font-medium text-white mb-4">Barrier Sync</h2>
      
      <div className="space-y-3" role="list" aria-label="Barrier synchronization status">
        {barriers.map((barrier) => {
          const progress = (barrier.arrived / barrier.total) * 100
          const isComplete = barrier.status === 'complete'
          
          return (
            <div key={barrier.id} className="space-y-2" role="listitem">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  {isComplete ? (
                    <Check className="w-4 h-4 text-emerald-400" aria-hidden="true" />
                  ) : (
                    <Loader2 className="w-4 h-4 text-blue-400 animate-spin" aria-hidden="true" />
                  )}
                  <span className="text-sm text-zinc-300">{barrier.name}</span>
                </div>
                <span className="text-xs text-zinc-500">
                  {barrier.arrived}/{barrier.total}
                </span>
              </div>
              
              <div className="h-1.5 bg-zinc-800 rounded-full overflow-hidden">
                <div
                  className={cn(
                    'h-full rounded-full transition-all duration-500',
                    isComplete ? 'bg-emerald-500' : 'bg-blue-500'
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
