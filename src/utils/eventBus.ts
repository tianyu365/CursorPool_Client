import { listen } from '@tauri-apps/api/event'

/**
 * 事件监听器列表
 */
const listeners: (() => void)[] = []

/**
 * 初始化所有事件监听
 */
export async function initEventListeners() {
  // 清除之前的监听器
  listeners.forEach((unlisten) => unlisten())
  listeners.length = 0

  // 添加仪表盘刷新事件监听
  const unlistenDashboardRefresh = await listen('refresh-dashboard', () => {
    // 检查当前是否在仪表盘页面
    const currentPath = window.location.pathname

    // 只检查是否为dashboard路径
    const isDashboardPage = currentPath === '/dashboard'

    if (isDashboardPage) {
      // 触发前端刷新事件
      window.dispatchEvent(new Event('refresh_dashboard_data'))
    }
  })

  listeners.push(unlistenDashboardRefresh)
}

/**
 * 销毁所有事件监听
 */
export function destroyEventListeners() {
  listeners.forEach((unlisten) => unlisten())
  listeners.length = 0
}
