import { useEffect, useState } from 'react'
import { Play, Square, Eye, Clock, CheckCircle, XCircle, AlertCircle, X, Ban } from 'lucide-react'
import { useDashboardStore } from '../store'
import { api } from '../lib/api'
import { cn } from '../lib/utils'
import type { CreateTaskRequest, Dataset, LogEntry, Task, Worker } from '../types'

export function TaskManager() {
  const { tasks, datasets, workers, fetchLiveData } = useDashboardStore()
  const [showStartDialog, setShowStartDialog] = useState(false)
  const [selectedTask, setSelectedTask] = useState<Task | null>(null)
  const [showLogs, setShowLogs] = useState(false)
  const [actionError, setActionError] = useState<string | null>(null)

  const handleStartTask = async (taskConfig: CreateTaskRequest) => {
    setActionError(null)
    await api.createTask(taskConfig)
    setShowStartDialog(false)
    await fetchLiveData()
  }

  const handleStopTask = async (taskId: string) => {
    setActionError(null)
    try {
      await api.stopTask(taskId)
      await fetchLiveData()
    } catch (error) {
      const msg = error instanceof Error ? error.message : 'Unknown error'
      setActionError(`Failed to stop task: ${msg}`)
    }
  }

  const getStatusIcon = (status: Task['status']) => {
    switch (status) {
      case 'running':
        return <Play className="w-4 h-4 text-emerald-400" aria-hidden="true" />
      case 'completed':
        return <CheckCircle className="w-4 h-4 text-emerald-400" aria-hidden="true" />
      case 'failed':
        return <XCircle className="w-4 h-4 text-red-400" aria-hidden="true" />
      case 'stopped':
        return <Ban className="w-4 h-4 text-zinc-400" aria-hidden="true" />
      case 'pending':
        return <Clock className="w-4 h-4 text-amber-300" aria-hidden="true" />
      default:
        return <AlertCircle className="w-4 h-4 text-zinc-500" aria-hidden="true" />
    }
  }

  const getStatusPill = (status: Task['status']) => {
    switch (status) {
      case 'running':
        return 'bg-emerald-500/[0.08] text-emerald-300 border-emerald-500/20'
      case 'completed':
        return 'bg-white/[0.05] text-zinc-200 border-white/10'
      case 'failed':
        return 'bg-red-500/[0.08] text-red-300 border-red-500/20'
      case 'stopped':
        return 'bg-zinc-500/[0.10] text-zinc-400 border-white/10'
      default:
        return 'bg-amber-500/[0.08] text-amber-200 border-amber-500/20'
    }
  }

  const formatDuration = (startTime: number, endTime?: number) => {
    const duration = Math.max(0, (endTime || Date.now()) - startTime)
    const minutes = Math.floor(duration / 60000)
    const seconds = Math.floor((duration % 60000) / 1000)
    return `${minutes}m ${seconds}s`
  }

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <p className="eyebrow mb-1">Orchestration</p>
          <h2 className="h-display text-[20px]">Training tasks</h2>
          <p className="text-[13px] text-zinc-500 mt-1">
            {tasks.length} total · {tasks.filter((t) => t.status === 'running').length} running
          </p>
        </div>
        <button
          onClick={() => setShowStartDialog(true)}
          className="btn-primary pressable"
        >
          <Play className="w-4 h-4" aria-hidden="true" />
          Start task
        </button>
      </div>

      {actionError && (
        <div className="px-4 py-3 rounded-[10px] bg-red-500/[0.06] border border-red-500/20 text-[12.5px] text-red-200">
          {actionError}
        </div>
      )}

      <div className="card overflow-hidden">
        {tasks.length === 0 ? (
          <div className="flex flex-col items-center justify-center text-center py-12 px-6">
            <div className="w-11 h-11 rounded-[10px] bg-white/[0.04] border border-white/[0.08] flex items-center justify-center mb-3.5 text-zinc-300">
              <Play className="w-5 h-5" aria-hidden="true" />
            </div>
            <p className="text-[14px] font-semibold text-zinc-100">No training tasks</p>
            <p className="text-[13px] text-zinc-500 mt-1">Launch your first distributed job to see it here.</p>
            <button onClick={() => setShowStartDialog(true)} className="btn-ghost mt-4 text-[13px] pressable">
              Start task
            </button>
          </div>
        ) : (
          <div className="row-divider" role="list" aria-label="Training tasks">
            {tasks.map((task) => (
              <div
                key={task.id}
                className="px-5 sm:px-6 py-5 hover:bg-white/[0.015] transition-colors duration-[150ms]"
                role="listitem"
              >
                <div className="flex items-center justify-between gap-3 mb-3">
                  <div className="flex items-center gap-3 min-w-0">
                    <div className="w-8 h-8 rounded-lg bg-white/[0.04] border border-white/[0.08] flex items-center justify-center shrink-0">
                      {getStatusIcon(task.status)}
                    </div>
                    <div className="min-w-0">
                      <div className="flex items-center gap-2 min-w-0">
                        <h3 className="text-[14px] font-semibold text-white truncate tracking-[-0.01em]">{task.name}</h3>
                        <span
                          className={cn(
                            'inline-flex items-center px-2 py-0.5 rounded-full text-[11px] font-medium border capitalize shrink-0',
                            getStatusPill(task.status)
                          )}
                        >
                          {task.status}
                        </span>
                      </div>
                      <p className="text-[12px] text-zinc-500 font-mono mt-0.5">
                        {task.type} · {task.id}
                      </p>
                    </div>
                  </div>
                  <div className="flex items-center gap-1 shrink-0">
                    <button
                      onClick={() => {
                        setSelectedTask(task)
                        setShowLogs(true)
                      }}
                      className="p-2 rounded-lg text-zinc-500 hover:text-zinc-100 hover:bg-white/[0.06] active:scale-[0.95] transition-all duration-[150ms]"
                      title="View logs"
                      aria-label={`View logs for ${task.name}`}
                    >
                      <Eye className="w-4 h-4" aria-hidden="true" />
                    </button>
                    {task.status === 'running' && (
                      <button
                        onClick={() => handleStopTask(task.id)}
                        className="btn-danger-ghost"
                        title="Stop task"
                        aria-label={`Stop ${task.name}`}
                      >
                        <Square className="w-3.5 h-3.5" aria-hidden="true" />
                        Stop
                      </button>
                    )}
                  </div>
                </div>

                <dl className="grid grid-cols-2 md:grid-cols-4 gap-4">
                  <div>
                    <dt className="text-[11px] text-zinc-500 font-medium">Workers</dt>
                    <dd className="text-[13px] text-zinc-100 font-semibold tabular mt-0.5">{task.workerIds.length}</dd>
                  </div>
                  <div>
                    <dt className="text-[11px] text-zinc-500 font-medium">Progress</dt>
                    <dd className="text-[13px] text-zinc-100 font-semibold tabular mt-0.5">{task.progress}%</dd>
                  </div>
                  <div>
                    <dt className="text-[11px] text-zinc-500 font-medium">Duration</dt>
                    <dd className="text-[13px] text-zinc-100 font-semibold tabular mt-0.5">
                      {formatDuration(task.startedAt, task.completedAt)}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-[11px] text-zinc-500 font-medium">Dataset</dt>
                    <dd className="text-[13px] text-zinc-100 font-mono mt-0.5 truncate">{task.datasetId}</dd>
                  </div>
                </dl>

                {task.status === 'running' && (
                  <div
                    className="mt-3.5 h-1 bg-white/[0.07] rounded-full overflow-hidden"
                    role="progressbar"
                    aria-valuenow={task.progress}
                    aria-valuemin={0}
                    aria-valuemax={100}
                    aria-label={`${task.name} progress`}
                  >
                    <div
                      className="bg-zinc-100 h-full rounded-full transition-all duration-300 ease-out"
                      style={{ width: `${task.progress}%` }}
                    />
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      {showStartDialog && (
        <StartTaskDialog
          datasets={datasets}
          workers={workers}
          onStart={handleStartTask}
          onClose={() => setShowStartDialog(false)}
        />
      )}

      {showLogs && selectedTask && (
        <TaskLogsDialog
          task={selectedTask}
          onClose={() => {
            setShowLogs(false)
            setSelectedTask(null)
          }}
        />
      )}
    </div>
  )
}

interface StartTaskDialogProps {
  datasets: Dataset[]
  workers: Worker[]
  onStart: (req: CreateTaskRequest) => Promise<void>
  onClose: () => void
}

function StartTaskDialog({ datasets, workers, onStart, onClose }: StartTaskDialogProps) {
  const [taskName, setTaskName] = useState('')
  const [taskType, setTaskType] = useState('image_classification')
  const [datasetId, setDatasetId] = useState(datasets[0]?.id ?? '')
  const [workerCount, setWorkerCount] = useState(workers.length > 0 ? 1 : 0)
  const [epochs, setEpochs] = useState(10)
  const [batchSize, setBatchSize] = useState(32)
  const [learningRate, setLearningRate] = useState(0.001)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const activeWorkerCount = workers.filter(
    (w) => w.status === 'active' || w.status === 'training' || w.status === 'loading_data'
  ).length

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setSubmitting(true)
    setError(null)
    try {
      await onStart({
        name: taskName.trim(),
        type: taskType,
        dataset_id: datasetId,
        worker_count: Number.isFinite(workerCount) ? Math.max(0, workerCount) : 0,
        config: {
          epochs: Number.isFinite(epochs) && epochs > 0 ? epochs : 10,
          batch_size: Number.isFinite(batchSize) && batchSize > 0 ? batchSize : 32,
          learning_rate: Number.isFinite(learningRate) && learningRate > 0 ? learningRate : 0.001,
        },
      })
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to start task')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div
      className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50 p-4"
      onClick={onClose}
      role="presentation"
    >
      <div
        className="bg-[#141417] border border-white/10 rounded-[12px] shadow-pop p-6 w-full max-w-md"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Start training task"
      >
        <div className="flex items-start justify-between gap-3 mb-1">
          <div>
            <p className="eyebrow mb-1">New job</p>
            <h3 className="h-display text-[17px]">Start training task</h3>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-zinc-500 hover:text-white hover:bg-white/[0.06] transition-colors"
            aria-label="Close dialog"
          >
            <X className="w-4 h-4" aria-hidden="true" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4 mt-4">
          {error && (
            <p className="text-[12.5px] text-red-300 bg-red-500/[0.08] border border-red-500/20 rounded-lg px-3 py-2">
              {error}
            </p>
          )}

          <div>
            <label className="label" htmlFor="task-name">
              Task name
            </label>
            <input
              id="task-name"
              type="text"
              value={taskName}
              onChange={(e) => setTaskName(e.target.value)}
              className="input"
              placeholder="resnet50-imagenet-v1"
              required
            />
          </div>

          <div>
            <label className="label" htmlFor="task-type">
              Task type
            </label>
            <select
              id="task-type"
              value={taskType}
              onChange={(e) => setTaskType(e.target.value)}
              className="input"
            >
              <option value="image_classification">Image classification</option>
              <option value="object_detection">Object detection</option>
              <option value="nlp_training">NLP training</option>
              <option value="custom">Custom</option>
            </select>
          </div>

          <div>
            <label className="label" htmlFor="task-dataset">
              Dataset
            </label>
            <select
              id="task-dataset"
              value={datasetId}
              onChange={(e) => setDatasetId(e.target.value)}
              className="input"
              required
            >
              <option value="">Select dataset…</option>
              {datasets.map((dataset) => (
                <option key={dataset.id} value={dataset.id}>
                  {dataset.name} ({dataset.totalSamples.toLocaleString()} samples)
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="label" htmlFor="worker-count">
              Worker count
            </label>
            <input
              id="worker-count"
              type="number"
              value={workerCount}
              onChange={(e) => {
                const parsed = parseInt(e.target.value, 10)
                setWorkerCount(Number.isNaN(parsed) ? 0 : parsed)
              }}
              min={0}
              max={workers.length > 0 ? workers.length : undefined}
              className="input tabular"
            />
            <p className="text-[12px] text-zinc-500 mt-1.5 tabular">
              {workers.length === 0
                ? 'No workers are currently connected — task will wait for workers or allocate requested count.'
                : `${activeWorkerCount} active workers available (${workers.length} total)`}
            </p>
          </div>

          <div className="grid grid-cols-3 gap-2.5">
            <div>
              <label className="label" htmlFor="task-epochs">
                Epochs
              </label>
              <input
                id="task-epochs"
                type="number"
                min={1}
                value={epochs}
                onChange={(e) => setEpochs(parseInt(e.target.value, 10) || 1)}
                className="input tabular"
                required
              />
            </div>
            <div>
              <label className="label" htmlFor="task-batch-size">
                Batch size
              </label>
              <input
                id="task-batch-size"
                type="number"
                min={1}
                value={batchSize}
                onChange={(e) => setBatchSize(parseInt(e.target.value, 10) || 1)}
                className="input tabular"
                required
              />
            </div>
            <div>
              <label className="label" htmlFor="task-lr">
                Learning rate
              </label>
              <input
                id="task-lr"
                type="number"
                step="any"
                min={0.000001}
                value={learningRate}
                onChange={(e) => setLearningRate(parseFloat(e.target.value) || 0.001)}
                className="input tabular"
                required
              />
            </div>
          </div>

          <div className="flex gap-2.5 pt-2">
            <button type="button" onClick={onClose} className="btn-ghost flex-1 pressable">
              Cancel
            </button>
            <button type="submit" disabled={submitting} className="btn-primary flex-1 pressable">
              {submitting ? 'Starting…' : 'Start task'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

function TaskLogsDialog({ task, onClose }: { task: Task; onClose: () => void }) {
  const [liveLogs, setLiveLogs] = useState<LogEntry[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    let active = true

    const fetchLogs = async () => {
      try {
        const fetched = await api.getTaskLogs(task.id)
        if (active) {
          setLiveLogs(fetched)
        }
      } catch {
        // Keep existing logs or fallback to task.logs
      } finally {
        if (active) setLoading(false)
      }
    }

    void fetchLogs()
    const timer = setInterval(() => {
      void fetchLogs()
    }, 2000)

    return () => {
      active = false
      clearInterval(timer)
    }
  }, [task.id])

  const hasLiveLogs = liveLogs.length > 0

  return (
    <div
      className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50 p-4"
      onClick={onClose}
      role="presentation"
    >
      <div
        className="bg-[#141417] border border-white/10 rounded-[12px] shadow-pop w-full max-w-3xl max-h-[80vh] flex flex-col overflow-hidden"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label={`Task logs: ${task.name}`}
      >
        <div className="flex justify-between items-center px-5 py-4 border-b border-white/[0.07]">
          <div className="min-w-0">
            <p className="eyebrow mb-0.5">Live task logs</p>
            <h3 className="h-display text-[15px] truncate font-mono">{task.name}</h3>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-zinc-500 hover:text-white hover:bg-white/[0.06] transition-colors shrink-0"
            aria-label="Close logs"
          >
            <X className="w-4 h-4" aria-hidden="true" />
          </button>
        </div>

        <div className="flex-1 bg-[#0c0c0e] m-4 mt-4 rounded-lg border border-white/[0.06] p-4 overflow-auto font-mono text-[12.5px] leading-relaxed">
          {loading && !hasLiveLogs && task.logs.length === 0 ? (
            <p className="text-zinc-500">Loading logs…</p>
          ) : hasLiveLogs ? (
            liveLogs.map((entry) => (
              <div key={entry.id} className="text-zinc-300 mb-1 break-words flex gap-2">
                <span className="text-zinc-600 shrink-0 tabular">
                  {new Date(entry.timestamp).toLocaleTimeString('en-US', { hour12: false })}
                </span>
                <span className="text-zinc-500 uppercase shrink-0">[{entry.level}]</span>
                <span>{entry.message}</span>
              </div>
            ))
          ) : task.logs.length === 0 ? (
            <p className="text-zinc-600">No logs available</p>
          ) : (
            task.logs.map((log, index) => (
              <div key={index} className="text-zinc-300 mb-1 break-words">
                {log}
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  )
}
