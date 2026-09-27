import { useEffect, useState } from 'react'
import { LineChart, Line, XAxis, YAxis, ResponsiveContainer, Tooltip, CartesianGrid } from 'recharts'
import { useDashboardStore } from '../store'

interface DataPoint {
  time: string
  throughput: number
  rps: number
}

function ChartTooltip({ active, payload, label }: any) {
  if (!active || !payload?.length) return null
  return (
    <div className="rounded-lg border border-white/10 bg-[#17171b] px-3 py-2 shadow-pop">
      <p className="text-[11px] font-mono text-zinc-500 tabular mb-1">{label}</p>
      {payload.map((p: any) => (
        <p key={p.dataKey} className="text-[12px] tabular text-zinc-200">
          <span className="inline-block w-2 h-2 rounded-full mr-1.5" style={{ background: p.stroke }} />
          {p.name}: <span className="font-semibold font-mono">{p.value}</span>
        </p>
      ))}
    </div>
  )
}

export function ThroughputChart() {
  const { metrics } = useDashboardStore()
  const [data, setData] = useState<DataPoint[]>([])

  useEffect(() => {
    const now = new Date()
    const time = now.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' })

    setData((prev) => {
      const newData = [
        ...prev,
        {
          time,
          throughput: metrics.checkpointThroughput,
          rps: metrics.coordinatorRps / 100,
        },
      ]
      return newData.slice(-30)
    })
  }, [metrics.checkpointThroughput, metrics.coordinatorRps])

  return (
    <div className="card overflow-hidden">
      <div className="flex items-baseline justify-between gap-3 px-6 pt-6 pb-1">
        <div>
          <p className="eyebrow mb-1">Telemetry</p>
          <h2 className="h-display text-[17px]">Performance</h2>
        </div>
        <div className="flex items-center gap-4 shrink-0">
          <span className="flex items-center gap-1.5 text-[12px] text-zinc-400">
            <span className="w-2 h-2 rounded-full bg-emerald-400" aria-hidden="true" />
            Throughput
          </span>
          <span className="flex items-center gap-1.5 text-[12px] text-zinc-400">
            <span className="w-2 h-2 rounded-full bg-zinc-500" aria-hidden="true" />
            RPS ÷100
          </span>
        </div>
      </div>

      {data.length < 2 ? (
        <div className="mx-6 mb-6 mt-3 h-[180px] rounded-lg bg-white/[0.02] border border-dashed border-white/10 flex items-center justify-center text-zinc-500 text-[13px]">
          Collecting telemetry…
        </div>
      ) : (
        <div className="px-2 pb-2 pt-2 h-[200px]" role="img" aria-label="Performance chart showing throughput and requests per second">
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={data} margin={{ top: 8, right: 16, bottom: 0, left: 0 }}>
              <CartesianGrid stroke="rgba(255,255,255,0.05)" vertical={false} />
              <XAxis
                dataKey="time"
                tick={{ fill: '#63636b', fontSize: 10, fontFamily: 'JetBrains Mono, monospace' }}
                axisLine={false}
                tickLine={false}
                minTickGap={48}
              />
              <YAxis
                tick={{ fill: '#63636b', fontSize: 10, fontFamily: 'JetBrains Mono, monospace' }}
                axisLine={false}
                tickLine={false}
                width={36}
              />
              <Tooltip content={<ChartTooltip />} cursor={{ stroke: 'rgba(255,255,255,0.15)' }} />
              <Line
                type="monotone"
                dataKey="throughput"
                stroke="#34d399"
                strokeWidth={1.75}
                dot={false}
                activeDot={{ r: 3, fill: '#34d399', stroke: '#0a0a0b', strokeWidth: 2 }}
                name="Throughput (MB/s)"
                isAnimationActive={false}
              />
              <Line
                type="monotone"
                dataKey="rps"
                stroke="#8b8b93"
                strokeWidth={1.5}
                strokeDasharray="4 3"
                dot={false}
                activeDot={{ r: 3, fill: '#8b8b93', stroke: '#0a0a0b', strokeWidth: 2 }}
                name="RPS (÷100)"
                isAnimationActive={false}
              />
            </LineChart>
          </ResponsiveContainer>
        </div>
      )}
    </div>
  )
}
