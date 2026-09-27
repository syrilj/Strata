import { useDashboardStore } from '../store'
import { formatTimestamp } from '../lib/utils'
import { useEffect, useState } from 'react'
import { RefreshCw } from 'lucide-react'

const VIEW_TITLES: Record<string, { title: string; subtitle: string }> = {
  dashboard: { title: 'Overview', subtitle: 'Live cluster health, throughput and training progress' },
  workers: { title: 'Workers', subtitle: 'GPU / CPU nodes, shards, epochs and heartbeats' },
  datasets: { title: 'Datasets', subtitle: 'Registered datasets, sharding and data preview' },
  tasks: { title: 'Tasks', subtitle: 'Launch, track and stop distributed training jobs' },
  activity: { title: 'Activity', subtitle: 'Checkpoints, barriers and recent events' },
  logs: { title: 'Logs', subtitle: 'System and task logs with live tail' },
  settings: { title: 'Settings', subtitle: 'Coordinator connection and runtime info' },
}

export function Header({ view = 'dashboard' }: { view?: string }) {
  const { coordinator, metrics, fetchLiveData, lastUpdated, lastError } = useDashboardStore()
  const [time, setTime] = useState(formatTimestamp(Date.now()))
  const [spinning, setSpinning] = useState(false)

  useEffect(() => {
    const interval = setInterval(() => {
      setTime(formatTimestamp(Date.now()))
    }, 1000)
    return () => clearInterval(interval)
  }, [])

  const meta = VIEW_TITLES[view] ?? VIEW_TITLES.dashboard

  const onRefresh = async () => {
    setSpinning(true)
    try {
      await fetchLiveData()
    } finally {
      setTimeout(() => setSpinning(false), 400)
    }
  }

  return (
    <header className="flex flex-wrap items-end justify-between gap-x-6 gap-y-4 mb-7">
      <div className="min-w-0 max-w-[560px]">
        <p className="eyebrow mb-1.5">Strata · {view}</p>
        <h1 className="h-display text-[26px] sm:text-[30px] leading-[1.1]">{meta.title}</h1>
        <p className="text-[14px] text-zinc-400 mt-1.5 leading-relaxed">
          {meta.subtitle}
        </p>
      </div>

      <div className="flex items-center gap-2.5 flex-wrap">
        <div className={`flex items-center gap-2 pl-3 pr-3.5 py-1.5 rounded-full border text-[13px] font-medium tabular ${coordinator.connected ? 'bg-emerald-500/[0.07] border-emerald-500/20 text-emerald-200' : 'bg-white/[0.03] border-white/10 text-zinc-400'}`}>
          <span
            className={`w-1.5 h-1.5 rounded-full ${coordinator.connected ? 'bg-emerald-400 animate-pulse-dot' : 'bg-zinc-600'}`}
            aria-hidden="true"
          />
          {coordinator.connected ? `${metrics.activeWorkers}/${metrics.totalWorkers} workers` : 'Disconnected'}
        </div>

        <button onClick={onRefresh} className="btn-ghost !px-3 pressable" aria-label="Refresh data">
          <RefreshCw className={`w-4 h-4 ${spinning ? 'animate-spin' : ''}`} aria-hidden="true" />
          <span className="hidden sm:inline text-[13px]">Refresh</span>
        </button>

        <div className="text-right leading-tight pl-1">
          <time className="block text-xs text-zinc-300 tabular font-mono" dateTime={new Date().toISOString()}>
            {time}
          </time>
          <span className="block text-[11px] text-zinc-500 tabular mt-0.5">
            {lastError ? 'update failed — retrying' : lastUpdated ? `updated ${new Date(lastUpdated).toLocaleTimeString()}` : 'connecting…'}
          </span>
        </div>
      </div>
    </header>
  )
}
