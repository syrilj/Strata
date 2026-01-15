import { useState } from 'react'
import { Play, Square, Eye, Clock, CheckCircle, XCircle, AlertCircle } from 'lucide-react'
import { useDashboardStore } from '../store'
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

export function TaskManager() {
  const { tasks, datasets, workers } = useDashboardStore()
  const [showStartDialog, setShowStartDialog] = useState(false)
  const [selectedTask, setSelectedTask] = useState<Task | null>(null)
  const [showLogs, setShowLogs] = useState(false)

  const handleStartTask = async (taskConfig: any) => {
    try {
      if (import.meta.env.DEV) {
        console.log('Starting task with config:', taskConfig)
      }
      const result = await api.startTask(taskConfig)
      if (import.meta.env.DEV) {
        console.log('Task started successfully:', result)
      }
      setShowStartDialog(false)
      // Refresh data
      await useDashboardStore.getState().fetchLiveData()
    } catch (error) {
      console.error('Failed to start task:', error)
      alert(`Failed to start task: ${error instanceof Error ? error.message : 'Unknown error'}`)
    }
  }

  const handleStopTask = async (taskId: string) => {
    try {
      if (import.meta.env.DEV) {
        console.log('Stopping task:', taskId)
      }
      const result = await api.stopTask(taskId)
      if (import.meta.env.DEV) {
        console.log('Task stopped successfully:', result)
      }
      // Refresh data
      await useDashboardStore.getState().fetchLiveData()
    } catch (error) {
      console.error('Failed to stop task:', error)
      alert(`Failed to stop task: ${error instanceof Error ? error.message : 'Unknown error'}`)
    }
  }

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'running':
        return <Play className="w-4 h-4 text-blue-400" />
      case 'completed':
        return <CheckCircle className="w-4 h-4 text-green-400" />
      case 'failed':
        return <XCircle className="w-4 h-4 text-red-400" />
      case 'pending':
        return <Clock className="w-4 h-4 text-yellow-400" />
      default:
        return <AlertCircle className="w-4 h-4 text-gray-400" />
    }
  }

  const formatDuration = (startTime: number, endTime?: number) => {
    const duration = (endTime || Date.now()) - startTime
    const minutes = Math.floor(duration / 60000)
    const seconds = Math.floor((duration % 60000) / 1000)
    return `${minutes}m ${seconds}s`
  }

  return (
    <div className="space-y-6">
      {/* Header with Start Task button */}
      <div className="flex justify-between items-center">
        <h2 className="text-lg font-semibold text-white">Training Tasks</h2>
        <button
          onClick={() => setShowStartDialog(true)}
          className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg flex items-center gap-2"
        >
          <Play className="w-4 h-4" />
          Start Task
        </button>
      </div>

      {/* Tasks List */}
      <div className="card">
        <div className="p-6">
          {tasks.length === 0 ? (
            <div className="text-center py-8 text-zinc-500">
              <AlertCircle className="w-8 h-8 mx-auto mb-2" />
              <p>No training tasks running</p>
              <p className="text-sm">Click "Start Task" to begin training</p>
            </div>
          ) : (
            <div className="space-y-4">
              {tasks.map((task) => (
                <div key={task.id} className="border border-zinc-700 rounded-lg p-4">
                  <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center gap-3">
                      {getStatusIcon(task.status)}
                      <div>
                        <h3 className="font-medium text-white">{task.name}</h3>
                        <p className="text-sm text-zinc-400">{task.type}</p>
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <button
                        onClick={() => {
                          setSelectedTask(task)
                          setShowLogs(true)
                        }}
                        className="p-2 text-zinc-400 hover:text-white"
                        title="View logs"
                      >
                        <Eye className="w-4 h-4" />
                      </button>
                      {task.status === 'running' && (
                        <button
                          onClick={() => handleStopTask(task.id)}
                          className="p-2 text-red-400 hover:text-red-300"
                          title="Stop task"
                        >
                          <Square className="w-4 h-4" />
                        </button>
                      )}
                    </div>
                  </div>

                  <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
                    <div>
                      <span className="text-zinc-500">Workers:</span>
                      <span className="ml-2 text-white">{task.worker_ids.length}</span>
                    </div>
                    <div>
                      <span className="text-zinc-500">Progress:</span>
                      <span className="ml-2 text-white">{task.progress}%</span>
                    </div>
                    <div>
                      <span className="text-zinc-500">Duration:</span>
                      <span className="ml-2 text-white">
                        {formatDuration(task.started_at, task.completed_at)}
                      </span>
                    </div>
                    <div>
                      <span className="text-zinc-500">Dataset:</span>
                      <span className="ml-2 text-white">{task.dataset_id}</span>
                    </div>
                  </div>

                  {task.status === 'running' && (
                    <div className="mt-3">
                      <div className="w-full bg-zinc-700 rounded-full h-2">
                        <div
                          className="bg-blue-600 h-2 rounded-full transition-all duration-300"
                          style={{ width: `${task.progress}%` }}
                        />
                      </div>
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Start Task Dialog */}
      {showStartDialog && (
        <StartTaskDialog
          datasets={datasets}
          workers={workers}
          onStart={handleStartTask}
          onClose={() => setShowStartDialog(false)}
        />
      )}

      {/* Task Logs Dialog */}
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

function StartTaskDialog({ datasets, workers, onStart, onClose }: any) {
  const [taskName, setTaskName] = useState('')
  const [taskType, setTaskType] = useState('image_classification')
  const [datasetId, setDatasetId] = useState('')
  const [workerCount, setWorkerCount] = useState(1)

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    onStart({
      name: taskName,
      type: taskType,
      dataset_id: datasetId,
      worker_count: workerCount,
      config: {
        epochs: 10,
        batch_size: 32,
        learning_rate: 0.001
      }
    })
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-zinc-800 rounded-lg p-6 w-full max-w-md">
        <h3 className="text-lg font-semibold text-white mb-4">Start Training Task</h3>
        
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm text-zinc-400 mb-1">Task Name</label>
            <input
              type="text"
              value={taskName}
              onChange={(e) => setTaskName(e.target.value)}
              className="w-full px-3 py-2 bg-zinc-700 border border-zinc-600 rounded text-white"
              placeholder="My Training Task"
              required
            />
          </div>

          <div>
            <label className="block text-sm text-zinc-400 mb-1">Task Type</label>
            <select
              value={taskType}
              onChange={(e) => setTaskType(e.target.value)}
              className="w-full px-3 py-2 bg-zinc-700 border border-zinc-600 rounded text-white"
            >
              <option value="image_classification">Image Classification</option>
              <option value="object_detection">Object Detection</option>
              <option value="nlp_training">NLP Training</option>
              <option value="custom">Custom</option>
            </select>
          </div>

          <div>
            <label className="block text-sm text-zinc-400 mb-1">Dataset</label>
            <select
              value={datasetId}
              onChange={(e) => setDatasetId(e.target.value)}
              className="w-full px-3 py-2 bg-zinc-700 border border-zinc-600 rounded text-white"
              required
            >
              <option value="">Select dataset...</option>
              {datasets.map((dataset: any) => (
                <option key={dataset.id} value={dataset.id}>
                  {dataset.name} ({dataset.totalSamples.toLocaleString()} samples)
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-sm text-zinc-400 mb-1">Worker Count</label>
            <input
              type="number"
              value={workerCount}
              onChange={(e) => setWorkerCount(parseInt(e.target.value))}
              min="1"
              max={workers.length}
              className="w-full px-3 py-2 bg-zinc-700 border border-zinc-600 rounded text-white"
            />
            <p className="text-xs text-zinc-500 mt-1">
              Available workers: {workers.filter((w: any) => w.status === 'active').length}
            </p>
          </div>

          <div className="flex gap-3 pt-4">
            <button
              type="button"
              onClick={onClose}
              className="flex-1 px-4 py-2 border border-zinc-600 text-zinc-400 rounded hover:bg-zinc-700"
            >
              Cancel
            </button>
            <button
              type="submit"
              className="flex-1 px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded"
            >
              Start Task
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

function TaskLogsDialog({ task, onClose }: any) {
  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-zinc-800 rounded-lg p-6 w-full max-w-4xl max-h-[80vh] flex flex-col">
        <div className="flex justify-between items-center mb-4">
          <h3 className="text-lg font-semibold text-white">Task Logs: {task.name}</h3>
          <button
            onClick={onClose}
            className="text-zinc-400 hover:text-white"
          >
            ✕
          </button>
        </div>
        
        <div className="flex-1 bg-zinc-900 rounded p-4 overflow-auto font-mono text-sm">
          {task.logs.length === 0 ? (
            <p className="text-zinc-500">No logs available</p>
          ) : (
            task.logs.map((log: string, index: number) => (
              <div key={index} className="text-zinc-300 mb-1">
                {log}
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  )
}