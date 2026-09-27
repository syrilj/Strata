import { useDashboardStore } from '../store'

function Stat({ label, value, unit, sub, first = false }: { label: string; value: string; unit?: string; sub: string; first?: boolean }) {
  return (
    <div className={`px-5 sm:px-6 py-5 min-w-0 ${first ? '' : 'border-l border-white/[0.07]'}`}>
      <p className="eyebrow">{label}</p>
      <p className="mt-2 text-[26px] leading-none font-semibold text-white tracking-[-0.02em] tabular">
        {value}
        {unit && <span className="text-[13px] text-zinc-500 font-medium ml-1.5 tracking-normal">{unit}</span>}
      </p>
      <p className="text-[12px] text-zinc-500 mt-1.5">{sub}</p>
    </div>
  )
}

export function MetricsCards() {
  const { metrics } = useDashboardStore()

  return (
    <section
      className="card mb-8 grid grid-cols-2 xl:grid-cols-4 divide-y sm:divide-y-0 overflow-hidden"
      role="region"
      aria-label="System metrics"
    >
      <div className="col-span-2 xl:col-span-1 border-b sm:border-b xl:border-b-0 border-white/[0.07]">
        <Stat
          first
          label="Checkpoint throughput"
          value={String(metrics.checkpointThroughput)}
          unit="MB/s"
          sub="Local NVMe · live"
        />
      </div>
      <div className="border-b sm:border-b xl:border-b-0 border-white/[0.07]">
        <Stat
          label="Coordinator"
          value={`${(metrics.coordinatorRps / 1000).toFixed(1)}K`}
          unit="req/s"
          sub="gRPC capacity"
        />
      </div>
      <div className="border-b sm:border-b xl:border-b-0 border-white/[0.07]">
        <Stat
          label="Workers"
          value={`${metrics.activeWorkers}/${metrics.totalWorkers}`}
          sub="Active / total"
        />
      </div>
      <div className="col-span-2 xl:col-span-1">
        <Stat
          label="Barrier sync p99"
          value={`<${metrics.barrierLatencyP99}`}
          unit="ms"
          sub="Sync latency"
        />
      </div>
    </section>
  )
}
