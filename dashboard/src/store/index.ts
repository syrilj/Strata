import { create } from 'zustand'
import type { Worker, Dataset, Checkpoint, SystemMetrics, LogEntry, CoordinatorStatus, BarrierStatus } from '../types'
import { api } from '../lib/api'

interface Task {
  id: string
  name: string
  type: string
  status: 'running' | 'completed' | 'failed' | 'pending'
  worker_ids: string[]
  dataset_id: string
  started_at: number
  completed_at?: number
  progress: number
  logs: string[]
}

interface DashboardState {
  // Connection state
  coordinator: CoordinatorStatus
  
  // Data
  workers: Worker[]
  datasets: Dataset[]
  checkpoints: Checkpoint[]
  barriers: BarrierStatus[]
  metrics: SystemMetrics
  logs: LogEntry[]
  tasks: Task[]
  
  // Actions
  setCoordinatorStatus: (status: Partial<CoordinatorStatus>) => void
  addLog: (entry: Omit<LogEntry, 'id' | 'timestamp'>) => void
  clearLogs: () => void
  setLogs: (logs: LogEntry[]) => void
  
  // Live mode - fetch from coordinator
  startLiveMode: () => void
  stopLiveMode: () => void
  fetchLiveData: () => Promise<void>
}

let pollingInterval: ReturnType<typeof setInterval> | null = null

export const useDashboardStore = create<DashboardState>((set, get) => ({
  // Initial state
  coordinator: {
    connected: false,
    address: 'localhost:50051',
    uptime: 0,
    version: '0.1.0',
  },
  workers: [],
  datasets: [],
  checkpoints: [],
  barriers: [],
  metrics: {
    checkpointThroughput: 0,
    coordinatorRps: 0,
    activeWorkers: 0,
    totalWorkers: 0,
    barrierLatencyP99: 0,
    shardAssignmentTime: 0,
  },
  logs: [],
  tasks: [],

  setCoordinatorStatus: (status) => {
    set((state) => ({
      coordinator: { ...state.coordinator, ...status },
    }))
  },

  addLog: (entry) => {
    const log: LogEntry = {
      ...entry,
      id: crypto.randomUUID(),
      timestamp: Date.now(),
    }
    set((state) => ({
      logs: [log, ...state.logs].slice(0, 100),
    }))
  },

  clearLogs: () => set({ logs: [] }),

  setLogs: (logs) => set({ logs }),

  // Live mode - fetch real data from coordinator
  fetchLiveData: async () => {
    try {
      const data = await api.getDashboardState()
      
      // Map API response to our types
      const workers: Worker[] = data.workers.map(w => ({
        id: w.id,
        ip: w.ip,
        port: w.port,
        status: w.status as 'active' | 'idle' | 'failed' | 'unknown',
        gpuCount: w.gpu_count,
        lastHeartbeat: w.last_heartbeat,
        assignedShards: w.assigned_shards,
        currentEpoch: w.current_epoch,
        currentStep: w.current_step,
        currentTask: w.current_task,
      }))
      
      const datasets: Dataset[] = data.datasets.map(d => ({
        id: d.id,
        name: d.name,
        totalSamples: d.total_samples,
        shardSize: d.shard_size,
        shardCount: d.shard_count,
        format: d.format,
        shuffle: d.shuffle,
        registeredAt: d.registered_at,
      }))
      
      const checkpoints: Checkpoint[] = data.checkpoints.map(c => ({
        id: c.id,
        step: c.step,
        epoch: c.epoch,
        size: c.size,
        path: c.path,
        createdAt: c.created_at,
        workerId: c.worker_id,
        status: c.status as 'completed' | 'in_progress' | 'failed',
      }))
      
      const barriers: BarrierStatus[] = data.barriers.map(b => ({
        id: b.id,
        name: b.name,
        arrived: b.arrived,
        total: b.total,
        status: b.status as 'waiting' | 'complete',
        createdAt: b.created_at,
      }))
      
      const metrics: SystemMetrics = {
        checkpointThroughput: data.metrics.checkpoint_throughput,
        coordinatorRps: data.metrics.coordinator_rps,
        activeWorkers: data.metrics.active_workers,
        totalWorkers: data.metrics.total_workers,
        barrierLatencyP99: data.metrics.barrier_latency_p99,
        shardAssignmentTime: data.metrics.shard_assignment_time,
      }

      const tasks: Task[] = (data.tasks || []).map(t => ({
        id: t.id,
        name: t.name,
        type: t.type,
        status: t.status,
        worker_ids: t.worker_ids,
        dataset_id: t.dataset_id,
        started_at: t.started_at,
        completed_at: t.completed_at,
        progress: t.progress,
        logs: t.logs,
      }))

      const systemLogs: LogEntry[] = (data.logs || []).map(l => ({
        id: l.id,
        timestamp: l.timestamp,
        level: l.level,
        message: l.message,
        source: l.source,
        taskId: l.task_id,
        workerId: l.worker_id,
      }))
      
      set({
        workers,
        datasets,
        checkpoints,
        barriers,
        metrics,
        tasks,
        logs: systemLogs,
        coordinator: {
          connected: true,
          address: data.coordinator.address,
          uptime: data.coordinator.uptime,
          version: data.coordinator.version,
        },
      })
    } catch (error) {
      // Log error for debugging but don't spam console in production
      if (import.meta.env.DEV) {
        console.error('Failed to fetch live data:', error)
      }
      set((state) => ({
        coordinator: { ...state.coordinator, connected: false },
      }))
    }
  },

  startLiveMode: () => {
    if (pollingInterval) return
    
    get().addLog({ level: 'info', message: 'Connecting to coordinator...', source: 'dashboard' })
    
    // Fetch immediately
    get().fetchLiveData()
    
    // Then poll every 2 seconds
    pollingInterval = setInterval(() => {
      get().fetchLiveData()
    }, 2000)
  },

  stopLiveMode: () => {
    if (pollingInterval) {
      clearInterval(pollingInterval)
      pollingInterval = null
    }
  },
}))
