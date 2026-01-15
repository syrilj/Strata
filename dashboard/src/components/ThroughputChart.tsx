import { useEffect, useState } from 'react'
import { LineChart, Line, XAxis, YAxis, ResponsiveContainer, Tooltip } from 'recharts'
import { useDashboardStore } from '../store'

interface DataPoint {
  time: string
  throughput: number
  rps: number
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
      // Keep last 30 data points
      return newData.slice(-30)
    })
  }, [metrics.checkpointThroughput, metrics.coordinatorRps])
  
  if (data.length < 2) {
    return (
      <div className="card p-5">
        <h2 className="text-sm font-medium text-white mb-4">Performance</h2>
        <div className="h-32 flex items-center justify-center text-zinc-500 text-sm">
          Collecting data...
        </div>
      </div>
    )
  }
  
  return (
    <div className="card p-5">
      <h2 className="text-sm font-medium text-white mb-4">Performance</h2>
      
      <div className="h-32" role="img" aria-label="Performance chart showing throughput and requests per second">
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={data}>
            <XAxis
              dataKey="time"
              tick={{ fill: '#71717a', fontSize: 10 }}
              axisLine={{ stroke: '#27272a' }}
              tickLine={false}
            />
            <YAxis
              tick={{ fill: '#71717a', fontSize: 10 }}
              axisLine={{ stroke: '#27272a' }}
              tickLine={false}
              width={30}
            />
            <Tooltip
              contentStyle={{
                backgroundColor: '#18181b',
                border: '1px solid #27272a',
                borderRadius: '8px',
                fontSize: '12px',
              }}
              labelStyle={{ color: '#a1a1aa' }}
            />
            <Line
              type="monotone"
              dataKey="throughput"
              stroke="#10b981"
              strokeWidth={2}
              dot={false}
              name="Throughput (MB/s)"
            />
            <Line
              type="monotone"
              dataKey="rps"
              stroke="#3b82f6"
              strokeWidth={2}
              dot={false}
              name="RPS (x100)"
            />
          </LineChart>
        </ResponsiveContainer>
      </div>
      
      <div className="flex items-center justify-center gap-6 mt-3">
        <div className="flex items-center gap-2">
          <div className="w-3 h-0.5 bg-emerald-500 rounded" aria-hidden="true" />
          <span className="text-xs text-zinc-500">Throughput</span>
        </div>
        <div className="flex items-center gap-2">
          <div className="w-3 h-0.5 bg-blue-500 rounded" aria-hidden="true" />
          <span className="text-xs text-zinc-500">RPS</span>
        </div>
      </div>
    </div>
  )
}
