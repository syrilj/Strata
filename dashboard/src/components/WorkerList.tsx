import { Server, AlertCircle, Clock, TrendingUp, Cpu } from 'lucide-react'
import { useDashboardStore } from '../store'
import { cn } from '../lib/utils'
import { EmptyState } from './State'
import type { Worker } from '../types'

function WorkerStatusBadge({ status }: { status: Worker['status'] }) {
  const config = {
    active: { icon: TrendingUp, classes: 'bg-emerald-500/[0.08] text-emerald-300 border-emerald-500/20', label: 'Training' },
    idle: { icon: Clock, classes: 'bg-white/[0.04] text-zinc-300 border-white/10', label: 'Idle' },
    failed: { icon: AlertCircle, classes: 'bg-red-500/[0.08] text-red-300 border-red-500/20', label: 'Failed' },
    unknown: { icon: AlertCircle, classes: 'bg-white/[0.04] text-zinc-500 border-white/10', label: 'Unknown' },
  }

  const { icon: Icon, classes, label } = config[status]

  return (
    <span className={cn('inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11.5px] font-medium border', classes)}>
      <Icon className="w-3 h-3" aria-hidden="true" />
      {label}
    </span>
  )
}

function parseTaskInfo(task: string): { stock?: string; action?: string } {
  const match = task.match(/^(\w+)_([A-Z]+)_?/)
  if (match) {
    return { action: match[1], stock: match[2] }
  }
  return {}
}

export function WorkerList() {
  const { workers, fetchLiveData } = useDashboardStore()

  if (workers.length === 0) {
    return (
      <div className="card p-6 sm:p-7">
        <div className="flex items-baseline justify-between gap-3">
          <div>
            <p className="eyebrow mb-1">Compute</p>
            <h2 className="h-display text-[17px]">Training workers</h2>
          </div>
          <span className="text-xs text-zinc-500 tabular shrink-0">0 connected</span>
        </div>
        <EmptyState
          icon={<Server className="w-5 h-5" aria-hidden="true" />}
          title="No workers connected"
          hint="Start the coordinator and workers with docker-compose up --build, then refresh. Demo data appears when DEMO_MODE=true."
          action={<button onClick={fetchLiveData} className="btn-ghost text-[13px] pressable">Retry</button>}
        />
      </div>
    )
  }

  return (
    <div className="card overflow-hidden">
      <div className="flex items-baseline justify-between gap-3 px-6 pt-6 pb-4">
        <div>
          <p className="eyebrow mb-1">Compute</p>
          <h2 className="h-display text-[17px]">Training workers</h2>
        </div>
        <span className="text-xs text-zinc-500 tabular shrink-0">{workers.length} connected</span>
      </div>

      <div className="row-divider border-t border-white/[0.06]" role="list" aria-label="Worker list">
        {workers.map((worker) => {
          const taskInfo = parseTaskInfo(worker.currentTask || '')

          return (
            <div
              key={worker.id}
              className="px-6 py-4 hover:bg-white/[0.02] transition-colors duration-[150ms] ease-out group"
              role="listitem"
            >
              <div className="flex items-center justify-between gap-3">
                <div className="flex items-center gap-3 min-w-0">
                  <div className="w-8 h-8 rounded-lg bg-white/[0.04] border border-white/[0.08] flex items-center justify-center shrink-0">
                    {worker.gpuCount > 0
                      ? <Server className="w-4 h-4 text-zinc-200" aria-hidden="true" />
                      : <Cpu className="w-4 h-4 text-zinc-400" aria-hidden="true" />}
                  </div>
                  <div className="min-w-0 flex items-center gap-2">
                    <span className="text-[13.5px] font-semibold text-zinc-100 font-mono truncate tracking-[-0.005em]">{worker.id}</span>
                    {taskInfo.stock && (
                      <span className="px-1.5 py-0.5 rounded-md bg-white/[0.05] text-zinc-300 border border-white/10 text-[11px] font-mono shrink-0">
                        {taskInfo.stock}
                      </span>
                    )}
                  </div>
                </div>
                <WorkerStatusBadge status={worker.status} />
              </div>

              <div className="grid grid-cols-3 gap-4 mt-3 ml-11">
                <div>
                  <p className="text-[11px] text-zinc-500 font-medium">Epoch</p>
                  <p className="text-[13px] text-zinc-200 font-semibold tabular mt-0.5">{worker.currentEpoch}</p>
                </div>
                <div>
                  <p className="text-[11px] text-zinc-500 font-medium">Step</p>
                  <p className="text-[13px] text-zinc-200 font-semibold tabular mt-0.5">{worker.currentStep}</p>
                </div>
                <div>
                  <p className="text-[11px] text-zinc-500 font-medium">GPUs · Shards</p>
                  <p className="text-[13px] text-zinc-200 font-semibold tabular mt-0.5">{worker.gpuCount} · {worker.assignedShards}</p>
                </div>
              </div>

              {worker.currentTask && (
                <p className="mt-2 ml-11 text-[11.5px] text-zinc-600 truncate font-mono" title={worker.currentTask}>
                  {worker.currentTask}
                </p>
              )}
            </div>
          )
        })}
      </div>
    </div>
  )
}
