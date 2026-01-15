import { HardDrive, RadioTower, Users, Timer } from 'lucide-react'
import { useDashboardStore } from '../store'

interface MetricCardProps {
  icon: React.ReactNode
  label: string
  value: string | number
  unit?: string
  sublabel?: string
}

function MetricCard({ icon, label, value, unit, sublabel }: MetricCardProps) {
  return (
    <div className="card p-5">
      <div className="flex items-center gap-2 mb-3">
        {icon}
        <span className="text-xs text-zinc-500 uppercase tracking-wide">{label}</span>
      </div>
      <p className="text-2xl font-semibold text-white">
        {value}
        {unit && <span className="text-sm text-zinc-500 font-normal ml-1">{unit}</span>}
      </p>
      {sublabel && <p className="text-xs text-zinc-600 mt-1">{sublabel}</p>}
    </div>
  )
}

export function MetricsCards() {
  const { metrics } = useDashboardStore()
  
  return (
    <div className="grid grid-cols-4 gap-4 mb-6" role="region" aria-label="System metrics">
      <MetricCard
        icon={<HardDrive className="w-4 h-4 text-zinc-500" aria-hidden="true" />}
        label="Checkpoint Throughput"
        value={metrics.checkpointThroughput}
        unit="MB/s"
        sublabel="Local NVMe"
      />
      <MetricCard
        icon={<RadioTower className="w-4 h-4 text-zinc-500" aria-hidden="true" />}
        label="Coordinator"
        value={`${(metrics.coordinatorRps / 1000).toFixed(1)}K+`}
        unit="req/s"
        sublabel="gRPC capacity"
      />
      <MetricCard
        icon={<Users className="w-4 h-4 text-zinc-500" aria-hidden="true" />}
        label="Workers"
        value={`${metrics.activeWorkers}/${metrics.totalWorkers}`}
        sublabel="Active / Total"
      />
      <MetricCard
        icon={<Timer className="w-4 h-4 text-zinc-500" aria-hidden="true" />}
        label="Barrier Sync"
        value={`<${metrics.barrierLatencyP99}`}
        unit="ms"
        sublabel="p99 latency"
      />
    </div>
  )
}
