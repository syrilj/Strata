import { create } from 'zustand'
import type {
  BarrierStatus,
  Checkpoint,
  CoordinatorStatus,
  Dataset,
  LogEntry,
  RegisterDatasetRequest,
  SystemMetrics,
  Task,
  ThroughputDataPoint,
  Worker,
} from '../types'
import { api } from '../lib/api'

interface DashboardState {
  // Connection state
  coordinator: CoordinatorStatus
  lastError: string | null
  lastUpdated: number | null
  retryCount: number

  // Data
  workers: Worker[]
  datasets: Dataset[]
  checkpoints: Checkpoint[]
  barriers: BarrierStatus[]
  metrics: SystemMetrics
  logs: LogEntry[]
  tasks: Task[]
  throughputHistory: ThroughputDataPoint[]

  // UI / Log state
  logsClearedAt: number | null
  logsAutoRefresh: boolean
  selectedDatasetId: string | null

  // Actions
  setCoordinatorStatus: (status: Partial<CoordinatorStatus>) => void
  addLog: (entry: Omit<LogEntry, 'id' | 'timestamp'>) => void
  clearLogs: () => void
  setLogs: (logs: LogEntry[]) => void
  setLogsAutoRefresh: (enabled: boolean) => void
  refreshLogs: () => Promise<void>
  setSelectedDatasetId: (datasetId: string | null) => void

  // Mutations
  registerDataset: (req: RegisterDatasetRequest) => Promise<Dataset>
  deleteDataset: (id: string) => Promise<void>
  deregisterWorker: (id: string) => Promise<void>
  deleteCheckpoint: (id: string) => Promise<void>

  // Live mode - fetch from coordinator
  startLiveMode: () => void
  stopLiveMode: () => void
  fetchLiveData: () => Promise<void>
}

let pollingInterval: ReturnType<typeof setInterval> | null = null
let backoffMs = 2000
let visibilityHandlerAttached = false

const MAX_THROUGHPUT_POINTS = 60

function filterClearedLogs(logs: LogEntry[], clearedAt: number | null): LogEntry[] {
  if (clearedAt === null) return logs
  return logs.filter((log) => log.timestamp > clearedAt)
}

export const useDashboardStore = create<DashboardState>((set, get) => ({
  // Initial state
  coordinator: {
    connected: false,
    address: 'localhost:50051',
    uptime: 0,
    version: '0.1.0',
  },
  lastError: null,
  lastUpdated: null,
  retryCount: 0,
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
  throughputHistory: [],
  logsClearedAt: null,
  logsAutoRefresh: true,
  selectedDatasetId: null,

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

  clearLogs: () => set({ logs: [], logsClearedAt: Date.now() }),

  setLogs: (logs) =>
    set((state) => ({
      logs: filterClearedLogs(logs, state.logsClearedAt),
    })),

  setLogsAutoRefresh: (enabled) => {
    set({ logsAutoRefresh: enabled })
    if (enabled) {
      void get().refreshLogs()
    }
  },

  refreshLogs: async () => {
    try {
      const fetched = await api.getLogs({ limit: 500 })
      const clearedAt = get().logsClearedAt
      set({ logs: filterClearedLogs(fetched, clearedAt) })
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error('Failed to refresh logs:', error)
      }
    }
  },

  setSelectedDatasetId: (datasetId) => set({ selectedDatasetId: datasetId }),

  registerDataset: async (req) => {
    const dataset = await api.registerDataset(req)
    if (dataset.id) {
      set({ selectedDatasetId: dataset.id })
    }
    await get().fetchLiveData()
    return dataset
  },

  deleteDataset: async (id) => {
    await api.deleteDataset(id)
    if (get().selectedDatasetId === id) {
      set({ selectedDatasetId: null })
    }
    await get().fetchLiveData()
  },

  deregisterWorker: async (id) => {
    await api.deregisterWorker(id)
    await get().fetchLiveData()
  },

  deleteCheckpoint: async (id) => {
    await api.deleteCheckpoint(id)
    await get().fetchLiveData()
  },

  // Live mode - fetch real data from coordinator
  fetchLiveData: async () => {
    try {
      const data = await api.getDashboard()
      const now = Date.now()

      let history = get().throughputHistory
      if (history.length === 0) {
        try {
          const seeded = await api.getMetricsHistory()
          if (Array.isArray(seeded) && seeded.length > 0) {
            history = seeded.slice(-MAX_THROUGHPUT_POINTS)
          }
        } catch {
          // Silently ignore if /metrics/history is unavailable
        }
      }

      const nextPoint: ThroughputDataPoint = {
        timestamp: now,
        checkpoints: data.metrics.checkpointThroughput,
        requests: data.metrics.coordinatorRps,
      }
      const updatedHistory = [...history, nextPoint].slice(-MAX_THROUGHPUT_POINTS)

      const { logsAutoRefresh, logsClearedAt, logs: existingLogs, selectedDatasetId } = get()
      const nextLogs = logsAutoRefresh
        ? filterClearedLogs(data.logs, logsClearedAt)
        : existingLogs

      const nextSelectedDatasetId =
        selectedDatasetId && data.datasets.some((d) => d.id === selectedDatasetId)
          ? selectedDatasetId
          : (data.datasets[0]?.id ?? null)

      set({
        workers: data.workers,
        datasets: data.datasets,
        checkpoints: data.checkpoints,
        barriers: data.barriers,
        metrics: data.metrics,
        tasks: data.tasks,
        logs: nextLogs,
        throughputHistory: updatedHistory,
        selectedDatasetId: nextSelectedDatasetId,
        lastError: null,
        lastUpdated: now,
        retryCount: 0,
        coordinator: {
          ...data.coordinator,
          connected: true,
        },
      })
      backoffMs = 2000
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to reach coordinator'
      if (import.meta.env.DEV) {
        console.error('Failed to fetch live data:', error)
      }
      const retryCount = get().retryCount + 1
      backoffMs = Math.min(backoffMs * 1.5, 15000)
      set((state) => ({
        retryCount,
        lastError: message,
        coordinator: { ...state.coordinator, connected: false },
      }))
      if (pollingInterval && typeof document !== 'undefined' && !document.hidden) {
        clearInterval(pollingInterval)
        pollingInterval = setInterval(() => {
          if (!document.hidden) void get().fetchLiveData()
        }, backoffMs)
      }
    }
  },

  startLiveMode: () => {
    if (pollingInterval) return
    backoffMs = 2000

    get().addLog({ level: 'info', message: 'Connecting to coordinator...', source: 'dashboard' })

    // Fetch immediately
    void get().fetchLiveData()

    if (!visibilityHandlerAttached && typeof document !== 'undefined') {
      visibilityHandlerAttached = true
      document.addEventListener('visibilitychange', () => {
        if (document.hidden && pollingInterval) {
          clearInterval(pollingInterval)
          pollingInterval = null
        } else if (!document.hidden && !pollingInterval) {
          void get().fetchLiveData()
          pollingInterval = setInterval(() => {
            if (!document.hidden) void get().fetchLiveData()
          }, backoffMs)
        }
      })
    }

    // Then poll every 2 seconds
    pollingInterval = setInterval(() => {
      if (typeof document === 'undefined' || !document.hidden) {
        void get().fetchLiveData()
      }
    }, 2000)
  },

  stopLiveMode: () => {
    if (pollingInterval) {
      clearInterval(pollingInterval)
      pollingInterval = null
    }
  },
}))
