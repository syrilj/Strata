import { Layers, LayoutDashboard, Database, Server, Settings, Activity, Play, FileText } from 'lucide-react'
import { cn } from '../lib/utils'
import { useDashboardStore } from '../store'

interface SidebarProps {
  activeTab: string
  onTabChange: (tab: string) => void
}

const NAV_ITEMS = [
  { id: 'dashboard', icon: LayoutDashboard, label: 'Dashboard' },
  { id: 'workers', icon: Server, label: 'Workers' },
  { id: 'datasets', icon: Database, label: 'Datasets' },
  { id: 'tasks', icon: Play, label: 'Tasks' },
  { id: 'activity', icon: Activity, label: 'Activity' },
  { id: 'logs', icon: FileText, label: 'Logs' },
  { id: 'settings', icon: Settings, label: 'Settings' },
]

export function Sidebar({ activeTab, onTabChange }: SidebarProps) {
  const { workers, tasks, coordinator } = useDashboardStore()
  const badges: Record<string, number> = {
    workers: workers.length,
    tasks: tasks.filter((t) => t.status === 'running').length,
  }

  return (
    <aside className="w-16 lg:w-60 shrink-0 border-r border-white/[0.07] bg-[#0e0e10] flex flex-col items-center lg:items-stretch py-5 gap-5 sticky top-0 h-screen">
      <div className="flex items-center gap-3 px-0 lg:px-5">
        <div className="w-9 h-9 rounded-[10px] bg-zinc-100 flex items-center justify-center shrink-0">
          <Layers className="w-[18px] h-[18px] text-zinc-950" aria-hidden="true" strokeWidth={2.25} />
        </div>
        <div className="hidden lg:block min-w-0">
          <p className="text-[14px] font-semibold text-white leading-tight tracking-[-0.01em]">Strata</p>
          <p className="text-[11px] text-zinc-500 leading-tight truncate mt-0.5">Training Control Plane</p>
        </div>
      </div>

      <div className="hidden lg:flex px-5">
        <span className={cn('inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border tabular',
          coordinator.connected
            ? 'bg-emerald-500/[0.08] text-emerald-300 border-emerald-500/20'
            : 'bg-white/[0.04] text-zinc-400 border-white/10')}>
          <span className={cn('w-1.5 h-1.5 rounded-full', coordinator.connected ? 'bg-emerald-400 animate-pulse-dot' : 'bg-zinc-600')} aria-hidden="true" />
          {coordinator.connected ? 'Live' : 'Offline'}
        </span>
      </div>

      <nav className="flex flex-col gap-1 mt-1 px-0 lg:px-3 w-full items-center lg:items-stretch" aria-label="Primary">
        {NAV_ITEMS.map((item) => {
          const Icon = item.icon
          const isActive = activeTab === item.id
          const count = badges[item.id]

          return (
            <button
              key={item.id}
              onClick={() => onTabChange(item.id)}
              className={cn(
                'group w-10 h-10 lg:w-full lg:h-auto lg:px-3 lg:py-2 rounded-lg flex items-center justify-center lg:justify-start lg:gap-3 transition-all duration-[150ms] ease-out active:scale-[0.98]',
                isActive
                  ? 'bg-white/[0.08] text-white'
                  : 'text-zinc-500 hover:text-zinc-200 hover:bg-white/[0.04]'
              )}
              title={item.label}
              aria-label={item.label}
              aria-current={isActive ? 'page' : undefined}
            >
              <Icon className={cn('w-[18px] h-[18px] shrink-0', isActive ? 'text-white' : 'text-zinc-500 group-hover:text-zinc-300')} aria-hidden="true" strokeWidth={2} />
              <span className={cn('hidden lg:block text-[13.5px] font-medium tracking-[-0.005em]', isActive ? 'text-white' : 'text-zinc-400 group-hover:text-zinc-200')}>{item.label}</span>
              {typeof count === 'number' && count > 0 && (
                <span className="hidden lg:inline-flex ml-auto text-[11px] tabular px-1.5 py-0.5 rounded-md bg-white/[0.06] text-zinc-300 border border-white/[0.08]">
                  {count}
                </span>
              )}
            </button>
          )
        })}
      </nav>

      <div className="mt-auto hidden lg:block px-3">
        <div className="rounded-[10px] border border-white/[0.07] bg-white/[0.02] px-3.5 py-3">
          <p className="eyebrow">Coordinator</p>
          <p className="text-xs text-zinc-300 font-mono mt-1.5 truncate">{coordinator.address}</p>
          <p className="text-[11px] text-zinc-500 mt-1 tabular">v{coordinator.version} · {Math.floor(coordinator.uptime / 60)}m uptime</p>
        </div>
      </div>
    </aside>
  )
}
