import { Cpu, LayoutDashboard, Database, Server, Settings, Activity, Play, FileText } from 'lucide-react'
import { cn } from '../lib/utils'

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
  return (
    <aside className="w-16 border-r border-zinc-800 flex flex-col items-center py-6 gap-6">
      <div className="w-9 h-9 rounded-lg bg-zinc-800 flex items-center justify-center">
        <Cpu className="w-5 h-5 text-zinc-300" />
      </div>
      
      <nav className="flex flex-col gap-2 mt-4">
        {NAV_ITEMS.map((item) => {
          const Icon = item.icon
          const isActive = activeTab === item.id
          
          return (
            <button
              key={item.id}
              onClick={() => onTabChange(item.id)}
              className={cn(
                'w-10 h-10 rounded-lg flex items-center justify-center transition-colors',
                isActive
                  ? 'bg-zinc-800'
                  : 'hover:bg-zinc-800/50'
              )}
              title={item.label}
              aria-label={item.label}
              aria-current={isActive ? 'page' : undefined}
            >
              <Icon className={cn('w-5 h-5', isActive ? 'text-white' : 'text-zinc-500')} />
            </button>
          )
        })}
      </nav>
    </aside>
  )
}
