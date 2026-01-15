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
} from './components'
import { TaskManager } from './components/TaskManager'
import { SystemLogs } from './components/SystemLogs'
import { DataPreview } from './components/DataPreview'

function DashboardView() {
  return (
    <>
      <MetricsCards />
      
      <div className="grid lg:grid-cols-3 gap-6">
        <div className="lg:col-span-2 space-y-6">
          <WorkerList />
          <ThroughputChart />
        </div>
        
        <div className="space-y-6">
          <DatasetList />
          <BarrierStatus />
          <CheckpointList />
        </div>
      </div>
      
      <ActivityLog />
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
  const { coordinator } = useDashboardStore()
  
  return (
    <div className="max-w-xl space-y-6">
      <div className="card p-6">
        <h2 className="text-sm font-medium text-white mb-4">Connection</h2>
        
        <div className="space-y-4">
          <div>
            <label className="block text-xs text-zinc-500 mb-1">Coordinator Address (gRPC)</label>
            <input
              type="text"
              value={coordinator.address}
              readOnly
              className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded-lg text-sm text-zinc-300"
            />
          </div>
          
          <div>
            <label className="block text-xs text-zinc-500 mb-1">HTTP API URL</label>
            <input
              type="text"
              value="http://localhost:51051/api"
              readOnly
              className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded-lg text-sm text-zinc-300"
            />
          </div>
          
          <div className={`p-3 rounded-lg ${coordinator.connected ? 'bg-emerald-500/10 border border-emerald-500/20' : 'bg-red-500/10 border border-red-500/20'}`}>
            <p className={`text-sm ${coordinator.connected ? 'text-emerald-400' : 'text-red-400'}`}>
              {coordinator.connected ? '● Connected to coordinator' : '○ Disconnected - Make sure coordinator is running'}
            </p>
            {coordinator.connected && (
              <p className="text-xs text-zinc-500 mt-1">
                Uptime: {Math.floor(coordinator.uptime / 60)}m {coordinator.uptime % 60}s
              </p>
            )}
          </div>
        </div>
      </div>
      
      <div className="card p-6">
        <h2 className="text-sm font-medium text-white mb-4">About</h2>
        <div className="space-y-2 text-sm">
          <p className="text-zinc-400">
            <span className="text-zinc-500">Version:</span> {coordinator.version}
          </p>
          <p className="text-zinc-400">
            <span className="text-zinc-500">Runtime:</span> Rust + Tokio
          </p>
          <p className="text-zinc-400">
            <span className="text-zinc-500">Protocol:</span> gRPC / HTTP/2
          </p>
        </div>
      </div>
    </div>
  )
}

export default function App() {
  const [activeTab, setActiveTab] = useState('dashboard')
  const { startLiveMode } = useDashboardStore()
  
  // Start live mode on mount
  useEffect(() => {
    startLiveMode()
  }, [startLiveMode])
  
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
    <div className="flex min-h-screen text-zinc-100 font-sans antialiased">
      <Sidebar activeTab={activeTab} onTabChange={setActiveTab} />
      
      <main className="flex-1 p-8">
        <Header />
        {renderView()}
      </main>
    </div>
  )
}
