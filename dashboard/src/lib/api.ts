// API client for coordinator HTTP endpoints

const API_BASE_URL = import.meta.env.VITE_API_URL || '/api'

export interface ApiWorker {
  id: string
  ip: string
  port: number
  status: string
  gpu_count: number
  last_heartbeat: number
  assigned_shards: number
  current_epoch: number
  current_step: number
  current_task: string
}

export interface ApiDataset {
  id: string
  name: string
  total_samples: number
  shard_size: number
  shard_count: number
  format: string
  shuffle: boolean
  registered_at: number
}

export interface ApiCheckpoint {
  id: string
  step: number
  epoch: number
  size: number
  path: string
  created_at: number
  worker_id: string
  status: string
}

export interface ApiBarrier {
  id: string
  name: string
  arrived: number
  total: number
  status: string
  created_at: number
}

export interface ApiMetrics {
  checkpoint_throughput: number
  coordinator_rps: number
  active_workers: number
  total_workers: number
  barrier_latency_p99: number
  shard_assignment_time: number
}

export interface ApiStatus {
  connected: boolean
  address: string
  uptime: number
  version: string
}

export interface ApiTask {
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

export interface ApiLogEntry {
  id: string
  timestamp: number
  level: 'info' | 'warn' | 'error' | 'debug'
  message: string
  source: string
  task_id?: string
  worker_id?: string
}

export interface ApiDashboardState {
  coordinator: ApiStatus
  workers: ApiWorker[]
  datasets: ApiDataset[]
  checkpoints: ApiCheckpoint[]
  barriers: ApiBarrier[]
  metrics: ApiMetrics
  tasks: ApiTask[]
  logs: ApiLogEntry[]
}

class CoordinatorApi {
  private baseUrl: string

  constructor(baseUrl: string = API_BASE_URL) {
    this.baseUrl = baseUrl
  }

  private async fetch<T>(endpoint: string): Promise<T> {
    const response = await fetch(`${this.baseUrl}${endpoint}`)
    if (!response.ok) {
      throw new Error(`API error: ${response.status} ${response.statusText}`)
    }
    return response.json()
  }

  async health(): Promise<{ status: string }> {
    return this.fetch('/health')
  }

  async getStatus(): Promise<ApiStatus> {
    return this.fetch('/status')
  }

  async getWorkers(): Promise<ApiWorker[]> {
    return this.fetch('/workers')
  }

  async getDatasets(): Promise<ApiDataset[]> {
    return this.fetch('/datasets')
  }

  async getCheckpoints(): Promise<ApiCheckpoint[]> {
    return this.fetch('/checkpoints')
  }

  async getBarriers(): Promise<ApiBarrier[]> {
    return this.fetch('/barriers')
  }

  async getMetrics(): Promise<ApiMetrics> {
    return this.fetch('/metrics')
  }

  async getDashboardState(): Promise<ApiDashboardState> {
    return this.fetch('/dashboard')
  }

  // Task management endpoints
  async getTasks(): Promise<ApiTask[]> {
    return this.fetch('/tasks')
  }

  async startTask(taskConfig: {
    name: string
    type: string
    dataset_id: string
    worker_count: number
    config: Record<string, any>
  }): Promise<{ task_id: string }> {
    const response = await fetch(`${this.baseUrl}/tasks`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(taskConfig)
    })
    if (!response.ok) {
      throw new Error(`Failed to start task: ${response.status} ${response.statusText}`)
    }
    return response.json()
  }

  async stopTask(taskId: string): Promise<{ success: boolean }> {
    const response = await fetch(`${this.baseUrl}/tasks/${taskId}/stop`, {
      method: 'POST'
    })
    if (!response.ok) {
      throw new Error(`Failed to stop task: ${response.status} ${response.statusText}`)
    }
    return response.json()
  }

  async getTaskLogs(taskId: string): Promise<ApiLogEntry[]> {
    return this.fetch(`/tasks/${taskId}/logs`)
  }

  async getLogs(limit: number = 100): Promise<ApiLogEntry[]> {
    return this.fetch(`/logs?limit=${limit}`)
  }
}

export const api = new CoordinatorApi()
