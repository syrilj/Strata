// API client for coordinator HTTP endpoints

import type {
  BarrierStatus,
  Checkpoint,
  CoordinatorStatus,
  CreateTaskRequest,
  DashboardStatePayload,
  DataSample,
  Dataset,
  DatasetPreviewResponse,
  DatasetShardInfo,
  DatasetShardsResponse,
  GpuUsage,
  LogEntry,
  RegisterDatasetRequest,
  SystemMetrics,
  Task,
  ThroughputDataPoint,
  Worker,
  WorkerResources,
  WorkerStatus,
} from '../types'

const STORAGE_KEY = 'strata_api_url'
const DEFAULT_BASE_URL = (import.meta.env.VITE_API_URL || '/api').replace(/\/+$/, '') || '/api'

type RawRecord = Record<string, unknown>

function asString(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : value != null ? String(value) : fallback
}

function asNumber(value: unknown, fallback = 0): number {
  if (typeof value === 'number' && Number.isFinite(value)) return value
  if (typeof value === 'string' && value.trim() !== '') {
    const parsed = Number(value)
    if (Number.isFinite(parsed)) return parsed
  }
  return fallback
}

function asOptionalNumber(value: unknown): number | undefined {
  if (value === undefined || value === null) return undefined
  const parsed = asNumber(value, Number.NaN)
  return Number.isFinite(parsed) ? parsed : undefined
}

function asBoolean(value: unknown, fallback = false): boolean {
  return typeof value === 'boolean' ? value : fallback
}

function asStringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return []
  return value.map((item) => asString(item))
}

function normalizeUrl(url: string): string {
  const trimmed = url.trim().replace(/\/+$/, '')
  return trimmed || DEFAULT_BASE_URL
}

function readStoredBaseUrl(): string {
  try {
    if (typeof window !== 'undefined' && window.localStorage) {
      const stored = window.localStorage.getItem(STORAGE_KEY)
      if (stored && stored.trim()) {
        return normalizeUrl(stored)
      }
    }
  } catch {
    // Ignore storage access errors
  }
  return DEFAULT_BASE_URL
}

const VALID_WORKER_STATUSES: WorkerStatus[] = [
  'active',
  'idle',
  'failed',
  'unknown',
  'training',
  'loading_data',
  'checkpointing',
  'recovering',
]

function mapWorkerStatus(status: unknown): WorkerStatus {
  const normalized = asString(status, 'unknown').toLowerCase() as WorkerStatus
  return VALID_WORKER_STATUSES.includes(normalized) ? normalized : 'unknown'
}

function mapGpuUsage(raw: RawRecord): GpuUsage {
  return {
    gpuId: asNumber(raw.gpuId ?? raw.gpu_id),
    utilizationPercent: asNumber(raw.utilizationPercent ?? raw.utilization_percent),
    memoryUsedBytes: asNumber(raw.memoryUsedBytes ?? raw.memory_used_bytes),
    memoryTotalBytes: asNumber(raw.memoryTotalBytes ?? raw.memory_total_bytes),
    temperatureCelsius: asNumber(raw.temperatureCelsius ?? raw.temperature_celsius),
  }
}

function mapWorkerResources(raw: unknown): WorkerResources | undefined {
  if (!raw || typeof raw !== 'object') return undefined
  const r = raw as RawRecord
  const gpuRaw = r.gpuUsage ?? r.gpu_usage
  return {
    cpuPercent: asNumber(r.cpuPercent ?? r.cpu_percent),
    memoryUsedBytes: asNumber(r.memoryUsedBytes ?? r.memory_used_bytes),
    diskReadBytes: asNumber(r.diskReadBytes ?? r.disk_read_bytes),
    diskWriteBytes: asNumber(r.diskWriteBytes ?? r.disk_write_bytes),
    networkRxBytes: asNumber(r.networkRxBytes ?? r.network_rx_bytes),
    networkTxBytes: asNumber(r.networkTxBytes ?? r.network_tx_bytes),
    gpuUsage: Array.isArray(gpuRaw) ? gpuRaw.map((g) => mapGpuUsage((g ?? {}) as RawRecord)) : [],
  }
}

export function mapWorker(raw: RawRecord): Worker {
  return {
    id: asString(raw.id),
    ip: asString(raw.ip),
    port: asNumber(raw.port),
    status: mapWorkerStatus(raw.status),
    gpuCount: asNumber(raw.gpuCount ?? raw.gpu_count),
    lastHeartbeat: asNumber(raw.lastHeartbeat ?? raw.last_heartbeat),
    assignedShards: asNumber(raw.assignedShards ?? raw.assigned_shards),
    currentEpoch: asNumber(raw.currentEpoch ?? raw.current_epoch),
    currentStep: asNumber(raw.currentStep ?? raw.current_step),
    currentTask: asString(raw.currentTask ?? raw.current_task),
    resources: mapWorkerResources(raw.resources),
  }
}

export function mapDataset(raw: RawRecord): Dataset {
  const pathVal = raw.path
  return {
    id: asString(raw.id),
    name: asString(raw.name),
    path: typeof pathVal === 'string' ? pathVal : undefined,
    totalSamples: asNumber(raw.totalSamples ?? raw.total_samples),
    shardSize: asNumber(raw.shardSize ?? raw.shard_size),
    shardCount: asNumber(raw.shardCount ?? raw.shard_count),
    format: asString(raw.format),
    shuffle: asBoolean(raw.shuffle),
    registeredAt: asNumber(raw.registeredAt ?? raw.registered_at),
  }
}

export function mapCheckpoint(raw: RawRecord): Checkpoint {
  const statusStr = asString(raw.status, 'completed')
  const status: Checkpoint['status'] =
    statusStr === 'in_progress' || statusStr === 'failed' ? statusStr : 'completed'
  return {
    id: asString(raw.id),
    step: asNumber(raw.step),
    epoch: asNumber(raw.epoch),
    size: asNumber(raw.size),
    path: asString(raw.path),
    createdAt: asNumber(raw.createdAt ?? raw.created_at),
    workerId: asString(raw.workerId ?? raw.worker_id),
    status,
  }
}

export function mapBarrier(raw: RawRecord): BarrierStatus {
  const statusStr = asString(raw.status, 'waiting')
  return {
    id: asString(raw.id),
    name: asString(raw.name),
    arrived: asNumber(raw.arrived),
    total: asNumber(raw.total),
    status: statusStr === 'complete' ? 'complete' : 'waiting',
    createdAt: asNumber(raw.createdAt ?? raw.created_at),
  }
}

export function mapMetrics(raw: RawRecord): SystemMetrics {
  return {
    checkpointThroughput: asNumber(raw.checkpointThroughput ?? raw.checkpoint_throughput),
    coordinatorRps: asNumber(raw.coordinatorRps ?? raw.coordinator_rps),
    activeWorkers: asNumber(raw.activeWorkers ?? raw.active_workers),
    totalWorkers: asNumber(raw.totalWorkers ?? raw.total_workers),
    barrierLatencyP99: asNumber(raw.barrierLatencyP99 ?? raw.barrier_latency_p99),
    shardAssignmentTime: asNumber(raw.shardAssignmentTime ?? raw.shard_assignment_time),
  }
}

export function mapCoordinatorStatus(raw: RawRecord): CoordinatorStatus {
  const httpAddressRaw = raw.httpAddress ?? raw.http_address
  const storageBackendRaw = raw.storageBackend ?? raw.storage_backend
  const demoModeRaw = raw.demoMode ?? raw.demo_mode
  return {
    connected: asBoolean(raw.connected, true),
    address: asString(raw.address, 'localhost:50051'),
    uptime: asNumber(raw.uptime),
    version: asString(raw.version, '0.1.0'),
    httpAddress: typeof httpAddressRaw === 'string' ? httpAddressRaw : undefined,
    storageBackend: typeof storageBackendRaw === 'string' ? storageBackendRaw : undefined,
    demoMode: typeof demoModeRaw === 'boolean' ? demoModeRaw : undefined,
  }
}

export function mapTask(raw: RawRecord): Task {
  const statusStr = asString(raw.status, 'pending')
  const status: Task['status'] =
    statusStr === 'running' ||
    statusStr === 'completed' ||
    statusStr === 'failed' ||
    statusStr === 'pending' ||
    statusStr === 'stopped'
      ? statusStr
      : 'pending'
  return {
    id: asString(raw.id),
    name: asString(raw.name),
    type: asString(raw.type),
    status,
    workerIds: asStringArray(raw.workerIds ?? raw.worker_ids),
    datasetId: asString(raw.datasetId ?? raw.dataset_id),
    startedAt: asNumber(raw.startedAt ?? raw.started_at),
    completedAt: asOptionalNumber(raw.completedAt ?? raw.completed_at),
    progress: asNumber(raw.progress),
    logs: asStringArray(raw.logs),
  }
}

export function mapLogEntry(raw: RawRecord): LogEntry {
  const levelStr = asString(raw.level, 'info')
  const level: LogEntry['level'] =
    levelStr === 'warn' || levelStr === 'error' || levelStr === 'debug' ? levelStr : 'info'
  const taskIdRaw = raw.taskId ?? raw.task_id
  const workerIdRaw = raw.workerId ?? raw.worker_id
  return {
    id: asString(raw.id),
    timestamp: asNumber(raw.timestamp, Date.now()),
    level,
    message: asString(raw.message),
    source: asString(raw.source),
    taskId: typeof taskIdRaw === 'string' ? taskIdRaw : undefined,
    workerId: typeof workerIdRaw === 'string' ? workerIdRaw : undefined,
  }
}

export function mapDataSample(raw: RawRecord): DataSample {
  const featuresRaw = raw.features
  const features =
    featuresRaw && typeof featuresRaw === 'object' && !Array.isArray(featuresRaw)
      ? (featuresRaw as Record<string, unknown>)
      : {}
  const workerIdRaw = raw.workerId ?? raw.worker_id
  return {
    id: asString(raw.id),
    features,
    label: raw.label,
    shardId: asNumber(raw.shardId ?? raw.shard_id),
    workerId: typeof workerIdRaw === 'string' ? workerIdRaw : undefined,
  }
}

export function mapDatasetPreview(raw: RawRecord, fallbackDatasetId: string): DatasetPreviewResponse {
  const samplesRaw = Array.isArray(raw.samples) ? raw.samples : []
  const samples = samplesRaw.map((s) => mapDataSample((s ?? {}) as RawRecord))
  const explicitCols = asStringArray(raw.columns)
  const inferredCols =
    explicitCols.length > 0
      ? explicitCols
      : Array.from(
          new Set(samples.flatMap((sample) => Object.keys(sample.features)))
        )

  return {
    datasetId: asString(raw.datasetId ?? raw.dataset_id, fallbackDatasetId),
    columns: inferredCols,
    samples,
    totalSamples: asNumber(raw.totalSamples ?? raw.total_samples, samples.length),
    format: asString(raw.format),
    note: typeof raw.note === 'string' ? raw.note : undefined,
  }
}

export function mapDatasetShards(raw: RawRecord, fallbackDatasetId: string): DatasetShardsResponse {
  const shardsRaw = Array.isArray(raw.shards) ? raw.shards : []
  const shards: DatasetShardInfo[] = shardsRaw.map((s) => {
    const item = (s ?? {}) as RawRecord
    const workerRaw = item.workerId ?? item.worker_id
    return {
      shardId: asNumber(item.shardId ?? item.shard_id),
      workerId: typeof workerRaw === 'string' ? workerRaw : undefined,
      sampleCount: asNumber(item.sampleCount ?? item.sample_count),
      status: typeof item.status === 'string' ? item.status : undefined,
    }
  })

  return {
    datasetId: asString(raw.datasetId ?? raw.dataset_id, fallbackDatasetId),
    totalShards: asNumber(raw.totalShards ?? raw.total_shards, shards.length),
    shardSize: asNumber(raw.shardSize ?? raw.shard_size),
    shards,
  }
}

export function mapThroughputPoint(raw: RawRecord): ThroughputDataPoint {
  return {
    timestamp: asNumber(raw.timestamp),
    checkpoints: asNumber(raw.checkpoints ?? raw.checkpointThroughput ?? raw.checkpoint_throughput),
    requests: asNumber(raw.requests ?? raw.coordinatorRps ?? raw.coordinator_rps),
  }
}

export function mapDashboardState(raw: RawRecord): DashboardStatePayload {
  const workersRaw = Array.isArray(raw.workers) ? raw.workers : []
  const datasetsRaw = Array.isArray(raw.datasets) ? raw.datasets : []
  const checkpointsRaw = Array.isArray(raw.checkpoints) ? raw.checkpoints : []
  const barriersRaw = Array.isArray(raw.barriers) ? raw.barriers : []
  const tasksRaw = Array.isArray(raw.tasks) ? raw.tasks : []
  const logsRaw = Array.isArray(raw.logs) ? raw.logs : []

  return {
    coordinator: mapCoordinatorStatus((raw.coordinator ?? {}) as RawRecord),
    workers: workersRaw.map((w) => mapWorker((w ?? {}) as RawRecord)),
    datasets: datasetsRaw.map((d) => mapDataset((d ?? {}) as RawRecord)),
    checkpoints: checkpointsRaw.map((c) => mapCheckpoint((c ?? {}) as RawRecord)),
    barriers: barriersRaw.map((b) => mapBarrier((b ?? {}) as RawRecord)),
    metrics: mapMetrics((raw.metrics ?? {}) as RawRecord),
    tasks: tasksRaw.map((t) => mapTask((t ?? {}) as RawRecord)),
    logs: logsRaw.map((l) => mapLogEntry((l ?? {}) as RawRecord)),
  }
}

class CoordinatorApi {
  private baseUrl: string

  constructor(baseUrl: string = readStoredBaseUrl()) {
    this.baseUrl = normalizeUrl(baseUrl)
  }

  getBaseUrl(): string {
    return this.baseUrl
  }

  setBaseUrl(url: string): string {
    const normalized = normalizeUrl(url)
    this.baseUrl = normalized
    try {
      if (typeof window !== 'undefined' && window.localStorage) {
        window.localStorage.setItem(STORAGE_KEY, normalized)
      }
    } catch {
      // Ignore storage access errors
    }
    return this.baseUrl
  }

  resetBaseUrl(): string {
    this.baseUrl = DEFAULT_BASE_URL
    try {
      if (typeof window !== 'undefined' && window.localStorage) {
        window.localStorage.removeItem(STORAGE_KEY)
      }
    } catch {
      // Ignore storage access errors
    }
    return this.baseUrl
  }

  private async request<T>(endpoint: string, init?: RequestInit): Promise<T> {
    const response = await fetch(`${this.baseUrl}${endpoint}`, init)
    if (!response.ok) {
      let detail = `${response.status} ${response.statusText}`.trim()
      try {
        const text = await response.text()
        if (text) {
          detail = `${detail}: ${text}`
        }
      } catch {
        // Ignore body parse errors
      }
      throw new Error(`API error: ${detail}`)
    }
    if (response.status === 204) {
      return undefined as T
    }
    const text = await response.text()
    if (!text) {
      return undefined as T
    }
    return JSON.parse(text) as T
  }

  async getHealth(): Promise<{ status: string }> {
    return this.request('/health')
  }

  async health(): Promise<{ status: string }> {
    return this.getHealth()
  }

  async getStatus(): Promise<CoordinatorStatus> {
    const raw = await this.request<RawRecord>('/status')
    return mapCoordinatorStatus(raw)
  }

  async getDashboard(): Promise<DashboardStatePayload> {
    const raw = await this.request<RawRecord>('/dashboard')
    return mapDashboardState(raw)
  }

  async getDashboardState(): Promise<DashboardStatePayload> {
    return this.getDashboard()
  }

  async getWorkers(): Promise<Worker[]> {
    const raw = await this.request<RawRecord[]>('/workers')
    return Array.isArray(raw) ? raw.map(mapWorker) : []
  }

  async getWorker(id: string): Promise<Worker> {
    const raw = await this.request<RawRecord>(`/workers/${encodeURIComponent(id)}`)
    return mapWorker(raw)
  }

  async deregisterWorker(id: string): Promise<void> {
    await this.request(`/workers/${encodeURIComponent(id)}`, {
      method: 'DELETE',
    })
  }

  async getDatasets(): Promise<Dataset[]> {
    const raw = await this.request<RawRecord[]>('/datasets')
    return Array.isArray(raw) ? raw.map(mapDataset) : []
  }

  async registerDataset(req: RegisterDatasetRequest): Promise<Dataset> {
    const raw = await this.request<RawRecord>('/datasets', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req),
    })
    return mapDataset(raw ?? {})
  }

  async deleteDataset(id: string): Promise<void> {
    await this.request(`/datasets/${encodeURIComponent(id)}`, {
      method: 'DELETE',
    })
  }

  async getDatasetPreview(id: string): Promise<DatasetPreviewResponse> {
    const raw = await this.request<RawRecord>(`/datasets/${encodeURIComponent(id)}/preview`)
    return mapDatasetPreview(raw ?? {}, id)
  }

  async getDatasetShards(id: string): Promise<DatasetShardsResponse> {
    const raw = await this.request<RawRecord>(`/datasets/${encodeURIComponent(id)}/shards`)
    return mapDatasetShards(raw ?? {}, id)
  }

  async getCheckpoints(): Promise<Checkpoint[]> {
    const raw = await this.request<RawRecord[]>('/checkpoints')
    return Array.isArray(raw) ? raw.map(mapCheckpoint) : []
  }

  async getCheckpoint(id: string): Promise<Checkpoint> {
    const raw = await this.request<RawRecord>(`/checkpoints/${encodeURIComponent(id)}`)
    return mapCheckpoint(raw)
  }

  async deleteCheckpoint(id: string): Promise<void> {
    await this.request(`/checkpoints/${encodeURIComponent(id)}`, {
      method: 'DELETE',
    })
  }

  async getBarriers(): Promise<BarrierStatus[]> {
    const raw = await this.request<RawRecord[]>('/barriers')
    return Array.isArray(raw) ? raw.map(mapBarrier) : []
  }

  async getMetrics(): Promise<SystemMetrics> {
    const raw = await this.request<RawRecord>('/metrics')
    return mapMetrics(raw)
  }

  async getMetricsHistory(): Promise<ThroughputDataPoint[]> {
    const raw = await this.request<RawRecord[]>('/metrics/history')
    return Array.isArray(raw) ? raw.map(mapThroughputPoint) : []
  }

  async getLogs(params?: {
    limit?: number
    level?: string
    source?: string
    task_id?: string
  }): Promise<LogEntry[]> {
    const search = new URLSearchParams()
    if (params?.limit !== undefined) search.set('limit', String(params.limit))
    if (params?.level) search.set('level', params.level)
    if (params?.source) search.set('source', params.source)
    if (params?.task_id) search.set('task_id', params.task_id)
    const query = search.toString()
    const raw = await this.request<RawRecord[]>(`/logs${query ? `?${query}` : ''}`)
    return Array.isArray(raw) ? raw.map(mapLogEntry) : []
  }

  async getTasks(): Promise<Task[]> {
    const raw = await this.request<RawRecord[]>('/tasks')
    return Array.isArray(raw) ? raw.map(mapTask) : []
  }

  async createTask(req: CreateTaskRequest): Promise<{ task_id: string }> {
    return this.request<{ task_id: string }>('/tasks', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req),
    })
  }

  async startTask(req: CreateTaskRequest): Promise<{ task_id: string }> {
    return this.createTask(req)
  }

  async stopTask(taskId: string): Promise<{ success: boolean }> {
    return this.request<{ success: boolean }>(`/tasks/${encodeURIComponent(taskId)}/stop`, {
      method: 'POST',
    })
  }

  async getTaskLogs(taskId: string): Promise<LogEntry[]> {
    const raw = await this.request<RawRecord[]>(`/tasks/${encodeURIComponent(taskId)}/logs`)
    return Array.isArray(raw) ? raw.map(mapLogEntry) : []
  }
}

export const api = new CoordinatorApi()
export { CoordinatorApi, DEFAULT_BASE_URL, STORAGE_KEY }
