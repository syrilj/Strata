import { Server, AlertCircle, Clock, TrendingUp } from 'lucide-react'
import { useDashboardStore } from '../store'
import { cn } from '../lib/utils'
import type { Worker } from '../types'

function WorkerStatusBadge({ status }: { status: Worker['status'] }) {
  const config = {
    active: { icon: TrendingUp, color: 'text-emerald-400', bg: 'bg-emerald-500/20', label: 'training' },
    idle: { icon: Clock, color: 'text-amber-400', bg: 'bg-amber-500/20', label: 'idle' },
    failed: { icon: AlertCircle, color: 'text-red-400', bg: 'bg-red-500/20', label: 'failed' },
    unknown: { icon: AlertCircle, color: 'text-zinc-400', bg: 'bg-zinc-500/20', label: 'unknown' },
  }
  
  const { icon: Icon, color, bg, label } = config[status]
  
  return (
    <span className={cn('flex items-center gap-1 px-2 py-0.5 rounded text-xs', bg, color)}>
      <Icon className="w-3 h-3" aria-hidden="true" />
      {label}
    </span>
  )
}

function parseTaskInfo(task: string): { stock?: string; action?: string } {
  // Parse task strings like "training_AAPL_epoch0" or "checkpoint_GOOGL_epoch1"
  const match = task.match(/^(\w+)_([A-Z]+)_?/)
  if (match) {
    return { action: match[1], stock: match[2] }
  }
  return {}
}

export function WorkerList() {
  const { workers } = useDashboardStore()
  
  if (workers.length === 0) {
    return (
      <div className="card p-6">
        <h2 className="text-sm font-medium text-white mb-4">Workers</h2>
        <p className="text-sm text-zinc-500 text-center py-8">No workers connected</p>
      </div>
    )
  }
  
  return (
    <div className="card p-6">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-sm font-medium text-white">Training Workers</h2>
        <span className="text-xs text-zinc-500">{workers.length} connected</span>
      </div>
      
      <div className="space-y-3" role="list" aria-label="Worker list">
        {workers.map((worker) => {
          const taskInfo = parseTaskInfo(worker.currentTask || '')
          
          return (
            <div
              key={worker.id}
              className="p-4 rounded-lg bg-zinc-800/30 hover:bg-zinc-800/50 transition-colors"
              role="listitem"
            >
              <div className="flex items-center justify-between mb-2">
                <div className="flex items-center gap-3">
                  <div className="w-8 h-8 rounded-lg bg-zinc-800 flex items-center justify-center">
                    <Server className="w-4 h-4 text-zinc-400" aria-hidden="true" />
                  </div>
                  <div>
                    <span className="text-sm font-medium text-zinc-200">{worker.id}</span>
                    {taskInfo.stock && (
                      <span className="ml-2 px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-400 text-xs">
                        {taskInfo.stock}
                      </span>
                    )}
                  </div>
                </div>
                <WorkerStatusBadge status={worker.status} />
              </div>
              
              <div className="grid grid-cols-3 gap-4 text-xs">
                <div>
                  <span className="text-zinc-500">Epoch</span>
                  <p className="text-zinc-300 font-medium">{worker.currentEpoch}</p>
                </div>
                <div>
                  <span className="text-zinc-500">Step</span>
                  <p className="text-zinc-300 font-medium">{worker.currentStep}</p>
                </div>
                <div>
                  <span className="text-zinc-500">GPUs</span>
                  <p className="text-zinc-300 font-medium">{worker.gpuCount}</p>
                </div>
              </div>
              
              {worker.currentTask && (
                <div className="mt-2 text-xs text-zinc-500 truncate">
                  Task: {worker.currentTask}
                </div>
              )}
            </div>
          )
        })}
      </div>
    </div>
  )
}
