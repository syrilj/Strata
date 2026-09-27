import { useEffect, useState } from 'react'
import { useDashboardStore } from './store'
import {
  Sidebar,
  Header,
  MetricsCards,
  WorkerList,
  CheckpointList,
  DatasetList,
  ActivityLog,
  BarrierStatus,
  ThroughputChart,
  ErrorBanner,
} from './components'
import { TaskManager } from './components/TaskManager'
import { SystemLogs } from './components/SystemLogs'
import { DataPreview } from './components/DataPreview'

function DashboardView() {
  const { lastError, fetchLiveData } = useDashboardStore()
  return (
    <>
      {lastError && (
        <div className="mb-4">
          <ErrorBanner message={`${lastError}. Retrying automatically — start the coordinator with 'cargo run -p coordinator' or 'docker-compose up'.`} onRetry={fetchLiveData} />
        </div>
      )}
      <MetricsCards />
      
      <div className="grid lg:grid-cols-3 gap-4 lg:gap-6">
        <div className="lg:col-span-2 space-y-4 lg:space-y-6 min-w-0">
          <WorkerList />
          <ThroughputChart />
        </div>
        
        <div className="space-y-4 lg:space-y-6 lg:sticky lg:top-24 self-start min-w-0">
          <DatasetList />
          <BarrierStatus />
          <CheckpointList />
        </div>
      </div>
      
      <div className="mt-4 lg:mt-6">
        <ActivityLog />
      </div>
    </>
  )
}

function WorkersView() {
  return (
    <div className="space-y-6">
      <MetricsCards />
      <WorkerList />
    </div>
  )
}

function DatasetsView() {
  const { datasets } = useDashboardStore()
  const firstDataset = datasets[0]?.id || 'imagenet-1k'
  
  return (
    <div className="space-y-6">
      <div className="grid lg:grid-cols-2 gap-6">
        <DatasetList />
        <BarrierStatus />
      </div>
      <DataPreview datasetId={firstDataset} />
    </div>
  )
}

function TasksView() {
  return (
    <div className="space-y-6">
      <TaskManager />
    </div>
  )
}

function LogsView() {
  return (
    <div className="space-y-6">
      <SystemLogs />
    </div>
  )
}

function ActivityView() {
  return (
    <div className="space-y-6">
      <div className="grid lg:grid-cols-2 gap-6">
        <CheckpointList />
        <ThroughputChart />
      </div>
      <ActivityLog />
    </div>
  )
}

function SettingsView() {
  const { coordinator, fetchLiveData } = useDashboardStore()
  const apiUrl = (import.meta as unknown as { env?: Record<string, string> }).env?.VITE_API_URL || 'http://localhost:51051/api'
  
  return (
    <div className="max-w-xl space-y-4">
      <div className="card p-6 sm:p-7">
        <p className="eyebrow mb-1">Connection</p>
        <h2 className="h-display text-[17px]">Coordinator</h2>
        <p className="text-[13px] text-zinc-500 mt-1 mb-5">gRPC + HTTP API used by this dashboard.</p>
        
        <div className="space-y-4">
          <div>
            <label className="label">Coordinator Address (gRPC)</label>
            <input
              type="text"
              value={coordinator.address}
              readOnly
              className="input font-mono"
            />
          </div>
          
          <div>
            <label className="label">HTTP API URL</label>
            <input
              type="text"
              value={apiUrl}
              readOnly
              className="input font-mono"
            />
          </div>
          
          <div className={`p-3.5 rounded-lg border ${coordinator.connected ? 'bg-emerald-500/[0.06] border-emerald-500/20' : 'bg-red-500/[0.06] border-red-500/20'}`}>
            <p className={`text-[13.5px] font-semibold tracking-[-0.01em] flex items-center gap-2 ${coordinator.connected ? 'text-emerald-200' : 'text-red-200'}`}>
              <span className={`w-1.5 h-1.5 rounded-full ${coordinator.connected ? 'bg-emerald-400 animate-pulse-dot' : 'bg-red-400'}`} aria-hidden="true" />
              {coordinator.connected ? 'Connected to coordinator' : 'Disconnected — start the coordinator'}
            </p>
            <p className="text-xs text-zinc-400 mt-1 tabular">
              {coordinator.connected
                ? `Uptime: ${Math.floor(coordinator.uptime / 60)}m ${coordinator.uptime % 60}s • v${coordinator.version}`
                : 'Run: cargo run -p coordinator  or  docker-compose up --build'}
            </p>
            {!coordinator.connected && (
              <button onClick={fetchLiveData} className="btn-ghost mt-3 !py-1.5 text-xs">Retry connection</button>
            )}
          </div>
        </div>
      </div>
      
      <div className="card p-6 sm:p-7">
        <p className="eyebrow mb-1">About</p>
        <h2 className="h-display text-[17px] mb-4">Strata</h2>
        <div className="space-y-2 text-sm">
          <p className="text-zinc-300">
            <span className="text-zinc-500">Version:</span> <span className="font-mono">{coordinator.version}</span>
          </p>
          <p className="text-zinc-300">
            <span className="text-zinc-500">Runtime:</span> Rust + Tokio • Axum HTTP • Tonic gRPC
          </p>
          <p className="text-zinc-300">
            <span className="text-zinc-500">Protocol:</span> gRPC / HTTP/2 • Dashboard polls <span className="font-mono">/api/dashboard</span> every 2s
          </p>
          <p className="text-zinc-300">
            <span className="text-zinc-500">Repo:</span> <span className="font-mono">github.com/syrilj/Strata</span>
          </p>
        </div>
      </div>
    </div>
  )
}

export default function App() {
  const [activeTab, setActiveTab] = useState('dashboard')
  const { startLiveMode, stopLiveMode } = useDashboardStore()
  
  // Start live mode on mount
  useEffect(() => {
    startLiveMode()
    return () => stopLiveMode()
  }, [startLiveMode, stopLiveMode])
  
  const renderView = () => {
    switch (activeTab) {
      case 'workers':
        return <WorkersView />
      case 'datasets':
        return <DatasetsView />
      case 'tasks':
        return <TasksView />
      case 'activity':
        return <ActivityView />
      case 'logs':
        return <LogsView />
      case 'settings':
        return <SettingsView />
      default:
        return <DashboardView />
    }
  }
  
  return (
    <div className="flex min-h-screen text-zinc-100 font-sans antialiased bg-[#0a0a0b]">
      <Sidebar activeTab={activeTab} onTabChange={setActiveTab} />
      
      <div className="flex-1 min-w-0">
        <div className="sticky top-0 z-30 bg-[#0a0a0b]/90 backdrop-blur-md border-b border-white/[0.07]">
          <div className="max-w-[1200px] mx-auto px-4 sm:px-6 lg:px-10 pt-6 pb-1">
            <Header view={activeTab} />
          </div>
        </div>
        <main className="max-w-[1200px] mx-auto px-4 sm:px-6 lg:px-10 py-8">
          {renderView()}
          <footer className="mt-12 pb-4 pt-5 border-t border-white/[0.06] flex items-center justify-between text-[11px] text-zinc-600">
            <span className="tracking-[-0.005em]">Strata — Distributed Training Control Plane · MIT</span>
            <span className="font-mono">github.com/syrilj/Strata</span>
          </footer>
        </main>
      </div>
    </div>
  )
}
