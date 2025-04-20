import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import {
  getMachineIds,
  getUsage,
  resetMachineId,
  switchAccount,
  checkHookStatus,
  applyHook,
  restoreHook,
  closeCursor,
  launchCursor,
  checkCursorRunning,
  getAccount,
  saveHistoryRecord,
  findCursorPath,
  getRunningCursorPath,
} from '@/api'
import type { UsageInfo, MachineInfo } from '@/api/types'
import type { HistoryAccount } from '@/types/history'
import { useHistoryStore } from './history'
import { open } from '@tauri-apps/plugin-dialog'
import Logger from '../utils/logger'
import { useRouter } from 'vue-router'
import { useAppStore } from './app'
import { Command } from '@tauri-apps/plugin-shell'

export const useCursorStore = defineStore('cursor', () => {
  // 状态
  const machineCode = ref('')
  const currentAccount = ref('')
  const cursorToken = ref('')
  const cursorInfo = ref<{
    userInfo: any | null
    usage: UsageInfo | null
    errorType: string | null
  }>({
    userInfo: null,
    usage: null,
    errorType: null,
  })
  const isLoading = ref(false)
  const hookStatus = ref<boolean | null>(null)
  const router = useRouter()
  const appStore = useAppStore()
  const operationLoading = ref(false)
  const machineCodeLoading = ref(false)
  const accountSwitchLoading = ref(false)
  const quickChangeLoading = ref(false)
  const isForceKilling = ref(false)
  const needSaveCurrentAccount = ref(false)

  // 添加文件选择模态框状态
  const showSelectFileModal = ref(false)
  const fileSelectError = ref('')
  const fileSelectLoading = ref(false)
  const pendingAction = ref<{
    type: string
    params?: any
  } | null>(null)

  // 添加macOS权限错误类型
  const macOSPermissionError = 'MACOS_PERMISSION_ERROR'

  // Getters
  const gpt4Usage = computed(() => {
    const usage = cursorInfo.value?.usage?.['gpt-4']
    if (!usage)
      return {
        used: 0,
        total: 0,
        percentage: 0,
      }
    return {
      used: usage.numRequests || 0,
      total: usage.maxRequestUsage || 0,
      percentage: getUsagePercentage(usage.numRequests, usage.maxRequestUsage),
    }
  })

  const gpt35Usage = computed(() => {
    const usage = cursorInfo.value?.usage?.['gpt-3.5-turbo']
    if (!usage)
      return {
        used: 0,
        total: 0,
        percentage: 0,
      }
    return {
      used: usage.numRequests || 0,
      total: usage.maxRequestUsage || 0,
      percentage: getUsagePercentage(usage.numRequests, usage.maxRequestUsage),
    }
  })

  const isHooked = computed(() => hookStatus.value === true)

  // 辅助函数
  function getUsagePercentage(used: number | null | undefined, total: number | null | undefined) {
    if (!used || !total) return 0
    return Math.min(100, Math.round((used / total) * 100))
  }

  // 格式化日期函数
  function formatDate(dateStr: string) {
    const date = new Date(dateStr)
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`
  }

  /**
   * 安全地获取Cursor使用情况
   * 失败时只记录日志，不会抛出异常
   * @param operationName 当前执行的操作名称，用于日志记录
   * @returns 获取是否成功
   */
  async function safelyFetchCursorUsage(operationName: string): Promise<boolean> {
    try {
      await fetchCursorUsage()
      return true
    } catch (error) {
      // 仅记录日志，不影响主流程
      await Logger.info(`获取Cursor使用情况失败，但不影响${operationName}流程`)
      console.error(`获取Cursor使用情况失败:`, error)
      return false
    }
  }

  // Actions
  /**
   * 获取机器码信息
   * @returns MachineInfo 机器码信息
   */
  async function fetchMachineIds(): Promise<MachineInfo> {
    try {
      isLoading.value = true
      const result = await getMachineIds()
      machineCode.value = result.machineId
      currentAccount.value = result.currentAccount
      cursorToken.value = result.cursorToken

      // 获取 Hook 状态
      await checkHook()

      return result
    } catch (error) {
      console.error('获取机器码失败:', error)
      throw error
    } finally {
      isLoading.value = false
    }
  }

  /**
   * 获取 Cursor 使用量
   */
  async function fetchCursorUsage() {
    try {
      // 如果没有token，尝试获取机器码信息
      if (!cursorToken.value) {
        try {
          // 获取机器码信息，但失败不阻止后续流程
          await fetchMachineIds()
        } catch (error) {
          // 只记录错误但继续尝试使用现有token
          console.error('获取机器码信息失败，但仍尝试获取使用量:', error)
          await Logger.warn('获取机器码信息失败，但仍尝试获取使用量')
        }
      }

      // 检查token是否可用
      if (!cursorToken.value) {
        console.error('未找到 Cursor Token')
        // 设置错误类型为数据库错误，但保留现有的usage数据
        cursorInfo.value = {
          userInfo: cursorInfo.value?.userInfo,
          usage: cursorInfo.value?.usage, // 保留现有usage数据
          errorType: 'cursor_db_error',
        }
        return
      }

      isLoading.value = true
      try {
        // 无论如何都尝试获取使用量数据
        const usageData = await getUsage(cursorToken.value)

        // TODO: 临时处理，Cursor高级模型使用量上限为50 适配cursor最新政策
        if (usageData && usageData['gpt-4'] && usageData['gpt-4'].maxRequestUsage === 150) {
          usageData['gpt-4'].maxRequestUsage = 50
        }

        // 更新状态，保留现有的userInfo
        cursorInfo.value = {
          userInfo: {
            email: currentAccount.value,
            email_verified: true,
            name: currentAccount.value.split('@')[0],
            sub: '',
            updated_at: new Date().toISOString(),
            picture: null,
          },
          usage: usageData,
          errorType: null,
        }
      } catch (usageError) {
        console.error('获取 Cursor 使用量失败:', usageError)

        // 设置适当的错误类型，但保留现有数据
        const errorMsg = usageError instanceof Error ? usageError.message : String(usageError)
        cursorInfo.value = {
          userInfo: cursorInfo.value?.userInfo,
          usage: cursorInfo.value?.usage, // 保留现有usage数据
          errorType: errorMsg,
        }
      }
    } finally {
      isLoading.value = false
    }
  }

  /**
   * 重置机器码
   */
  async function resetMachine({
    forceKill = false,
    machineId,
  }: {
    forceKill?: boolean
    machineId?: string
  } = {}) {
    try {
      machineCodeLoading.value = true
      await Logger.info('开始重置机器码')

      // 检查 Cursor 是否在运行
      await ensureCursorNotRunning(forceKill)

      await resetMachineId({
        forceKill,
        machineId,
      })
      await Logger.info('机器码重置成功')

      // 添加历史记录
      await saveHistoryRecord({
        id: Date.now(),
        type_name: '机器码修改',
        detail: `修改机器码: ${machineCode.value}`,
        timestamp: new Date().toISOString(),
        operator: '用户',
      })

      await fetchMachineIds()
      await safelyFetchCursorUsage('机器码重置')

      return true
    } catch (error) {
      await Logger.error(`重置机器码失败: ${error}`)
      throw error
    } finally {
      isLoading.value = false
      machineCodeLoading.value = false
    }
  }

  /**
   * 切换账户
   */
  async function switchCursorAccount(email?: string, token?: string, forceKill: boolean = false) {
    try {
      accountSwitchLoading.value = true
      await Logger.info('开始切换账户操作')

      // 检查 Cursor 是否在运行
      await ensureCursorNotRunning(forceKill)

      // 如果未提供邮箱和token，则自动获取
      if (!email || !token) {
        const accountInfo = await getAccount(undefined, '1')
        if (!accountInfo.account_info.account || !accountInfo.account_info.token) {
          await Logger.error('获取账户信息失败，无法进行切换')
          throw new Error('获取账户信息失败')
        }
        email = accountInfo.account_info.account
        token = accountInfo.account_info.token
      }

      await switchAccount(email, token, forceKill)
      await Logger.info(`账户切换成功: ${email}`)

      // 添加历史记录
      await saveHistoryRecord({
        id: Date.now(),
        type_name: '账户切换',
        detail: `切换到账户: ${email} 扣除50积分`,
        timestamp: new Date().toISOString(),
        operator: '用户',
      })

      await fetchMachineIds()
      await safelyFetchCursorUsage('账户切换')

      return true
    } catch (error) {
      await Logger.error(`账户切换失败: ${error}`)
      throw error
    } finally {
      isLoading.value = false
      accountSwitchLoading.value = false
    }
  }

  /**
   * 一键更换（账户+机器码）
   */
  async function quickChange(email?: string, token?: string, forceKill: boolean = false) {
    try {
      quickChangeLoading.value = true
      await Logger.info('开始一键换号操作')

      // 检查 Cursor 是否在运行
      await ensureCursorNotRunning(forceKill)

      // 保存原始机器码，以便失败时恢复
      const originalMachineId = machineCode.value

      let machineResetSuccess = false

      // 先重置机器码
      try {
        await resetMachine({
          forceKill,
        })
        machineResetSuccess = true
      } catch (error) {
        await Logger.error('一键换号时重置机器码失败')
        throw error
      }

      // 再切换账户
      try {
        await switchCursorAccount(email, token, forceKill)
        await Logger.info('一键换号完成')
      } catch (error) {
        await Logger.error('一键换号时切换账户失败')

        // 机器码重置成功 但账户切换失败 恢复原始机器码
        if (machineResetSuccess && originalMachineId) {
          try {
            await resetMachineId({
              forceKill,
              machineId: originalMachineId,
            })
            await Logger.info('恢复原始机器码成功')
            await fetchMachineIds() // 刷新机器码信息
          } catch (restoreError) {
            await Logger.error(`恢复原始机器码失败: ${restoreError}`)
          }
        }

        throw error
      }

      await fetchMachineIds()
      await safelyFetchCursorUsage('一键换号')

      return true
      // eslint-disable-next-line no-useless-catch
    } catch (error) {
      throw error
    } finally {
      isLoading.value = false
      quickChangeLoading.value = false
    }
  }

  /**
   * 检查Hook状态
   */
  async function checkHook() {
    try {
      // 清除先前的状态
      isLoading.value = true

      try {
        const status = await checkHookStatus()

        // 更新状态
        hookStatus.value = status
        return status
      } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error)

        // 如果是找不到main.js的错误，设置默认值false而不是抛出错误
        if (errorMsg.includes('MAIN_JS_NOT_FOUND') || errorMsg.includes('创建应用路径失败')) {
          await Logger.warn('找不到main.js，设置hook状态为false')
          hookStatus.value = false
          return false
        }

        // 其他错误情况下重置状态
        console.error('检查Hook状态失败:', error)
        hookStatus.value = null
        throw error
      }
    } finally {
      isLoading.value = false
    }
  }

  /**
   * 应用 Hook
   */
  async function applyHookToClient(forceKill: boolean = false) {
    try {
      await Logger.info('开始注入Hook')
      operationLoading.value = true
      isLoading.value = true

      // 检查Cursor是否在运行
      const isRunning = await checkCursorRunning()
      await Logger.info(`Cursor运行状态: ${isRunning ? '运行中' : '未运行'}`)

      // 如果Cursor在运行且不是强制模式，返回特殊状态让外部组件显示确认弹窗
      if (isRunning && !forceKill) {
        await Logger.info('检测到Cursor正在运行，需要用户确认')
        return { status: 'running', action: 'applyHook' }
      }

      // Windows平台特殊处理 - 尝试注入运行中的Cursor
      if (appStore.currentPlatform === 'windows' && isRunning && forceKill) {
        try {
          await Logger.info('强制模式：检测到Cursor正在运行，尝试注入运行中的Cursor')
          await injectRunningCursor()
          hookStatus.value = true
          await checkHook()
          await Logger.info('注入运行中的Cursor成功')
          return { status: 'success' }
        } catch (injectError) {
          await Logger.error(`注入运行中的Cursor失败: ${injectError}`)
          // 注入失败，继续下一步
        }
      }

      // 尝试使用系统变量查找Cursor路径并注入
      try {
        await Logger.info('尝试使用系统变量查找Cursor路径并注入')
        await applyHook(forceKill)
        hookStatus.value = true
        await checkHook()
        await Logger.info('使用系统变量查找的路径注入Hook成功')
        return { status: 'success' }
      } catch (error) {
        // 获取完整的错误信息
        const errorMsg = error instanceof Error ? error.message : String(error)

        // 如果是找不到main.js的错误，继续下一步
        if (errorMsg.includes('MAIN_JS_NOT_FOUND') || errorMsg.includes('创建应用路径失败')) {
          // 显示文件选择模态框
          await Logger.info('无法自动查找Cursor路径，显示文件选择模态框')
          if (router) router.push('/settings')
          setPendingAction('applyHook', { forceKill })
          return { status: 'need_select_file' }
        } else if (errorMsg.includes('Cursor进程正在运行') && !forceKill) {
          // 如果是Cursor正在运行的错误且不是强制模式
          await Logger.info('后端检测到Cursor正在运行且不是强制模式，显示确认对话框')
          return { status: 'running', action: 'applyHook' }
        } else if (
          (errorMsg.includes('Cursor进程正在运行') || errorMsg.includes('请先关闭 Cursor')) &&
          forceKill
        ) {
          // 如果是强制模式，尝试关闭Cursor并注入
          await Logger.info('强制模式：检测到Cursor正在运行，尝试关闭并重新注入')
          try {
            await closeCursorApp()
            await new Promise((resolve) => setTimeout(resolve, 1000))
            await applyHook(true)
            hookStatus.value = true
            await checkHook()
            await Logger.info('强制关闭Cursor并注入Hook成功')
            return { status: 'success' }
          } catch (closeError) {
            const closeErrorMsg =
              closeError instanceof Error ? closeError.message : String(closeError)

            // 检查是否是macOS达到最大重试次数错误
            if (
              appStore.currentPlatform === 'macos' &&
              (closeErrorMsg.includes('达到最大重试次数') ||
                closeErrorMsg.includes('无法终止所有Cursor进程'))
            ) {
              await Logger.error(`macOS关闭Cursor失败，可能需要系统权限: ${closeErrorMsg}`)
              // 返回特殊状态，让外部组件显示macOS权限提示
              return {
                status: 'error',
                errorType: macOSPermissionError,
                message: '无法终止Cursor进程，需要系统权限',
              }
            }

            // 其他错误直接抛出
            throw closeError
          }
        } else {
          // 其他错误，直接抛出
          throw error
        }
      }
    } catch (error) {
      await Logger.error(`Hook注入失败: ${error}`)
      hookStatus.value = false
      throw error
    } finally {
      isLoading.value = false
      operationLoading.value = false
    }
  }

  /**
   * 恢复 Hook
   */
  async function restoreHookFromClient(forceKill: boolean = false) {
    try {
      await Logger.info('开始恢复Hook')
      operationLoading.value = true
      isLoading.value = true

      // 执行操作
      await restoreHook(forceKill)

      // 明确设置状态为 false
      hookStatus.value = false

      // 触发检查以确保状态已更新
      await checkHook()

      await Logger.info('Hook恢复成功')
      return true
    } catch (error) {
      await Logger.error(`Hook恢复失败: ${error}`)
      throw error
    } finally {
      isLoading.value = false
      operationLoading.value = false
    }
  }

  /**
   * 关闭 Cursor
   */
  async function closeCursorApp() {
    try {
      operationLoading.value = true
      return await closeCursor()
    } catch (error) {
      console.error('关闭 Cursor 失败:', error)
      throw error
    } finally {
      operationLoading.value = false
    }
  }

  /**
   * 启动 Cursor
   */
  async function launchCursorApp() {
    try {
      operationLoading.value = true
      return await launchCursor()
    } catch (error) {
      console.error('启动 Cursor 失败:', error)
      throw error
    } finally {
      operationLoading.value = false
    }
  }

  /**
   * 检查是否需要注入Hook并自动注入
   */
  async function ensureHookApplied() {
    // 检查 Hook 状态
    await checkHook()

    // 如果未注入，尝试自动注入
    if (!hookStatus.value) {
      return await applyHookToClient(false)
    }

    return true
  }

  /**
   * 刷新所有Cursor相关数据
   */
  async function refreshAllCursorData() {
    try {
      isLoading.value = true
      await fetchMachineIds()
      await safelyFetchCursorUsage('数据刷新')

      return true
    } catch (error) {
      console.error('刷新数据失败:', error)
      throw error
    } finally {
      isLoading.value = false
    }
  }

  /**
   * 切换到历史账户
   */
  async function switchToHistoryAccount(account: HistoryAccount) {
    const historyStore = useHistoryStore()
    historyStore.switchingAccount[account.email] = true

    try {
      // 检查Cursor是否在运行
      const isRunning = await checkCursorRunning()
      if (isRunning) {
        // 返回需要处理的状态
        return {
          status: 'running',
          account,
        }
      }

      // 检查钩子状态
      const hookStatus = await checkHookStatus()
      if (!hookStatus) {
        const hookSuccess = await applyHookToClient(false)
        if (!hookSuccess) {
          return {
            status: 'hook_failed',
          }
        }
      }

      // 切换账户 - 后端会自动保存历史记录
      await resetMachineId({
        machineId: account.machineCode,
      })
      await switchAccount(account.email, account.token, false)

      await saveHistoryRecord({
        id: Date.now(),
        type_name: '历史账户切换',
        detail: `切换到历史账户: ${account.email}`,
        timestamp: new Date().toISOString(),
        operator: '用户',
      })

      await fetchMachineIds()
      await safelyFetchCursorUsage('历史账户切换')

      return {
        status: 'success',
      }
    } catch (error) {
      console.error('切换到历史账户失败:', error)
      throw error
    } finally {
      historyStore.switchingAccount[account.email] = false
    }
  }

  /**
   * 强制关闭并切换账户
   */
  async function forceCloseAndSwitch(account: HistoryAccount) {
    const historyStore = useHistoryStore()
    historyStore.switchingAccount[account.email] = true
    isForceKilling.value = true

    try {
      // 关闭 Cursor
      await closeCursorApp()
      await new Promise((resolve) => setTimeout(resolve, 1000))

      // 检查并应用钩子
      if (!(await checkHookStatus())) {
        const hookSuccess = await applyHookToClient(true)
        if (!hookSuccess) {
          return {
            status: 'hook_failed',
          }
        }
      }

      // 账户切换 - 后端会自动保存历史记录
      await resetMachineId({
        machineId: account.machineCode,
      })
      await switchAccount(account.email, account.token, true)

      await saveHistoryRecord({
        id: Date.now(),
        type_name: '历史账户切换',
        detail: `切换到历史账户: ${account.email}`,
        timestamp: new Date().toISOString(),
        operator: '用户',
      })

      await fetchMachineIds()
      await safelyFetchCursorUsage('强制切换账户')

      // 启动 Cursor
      await launchCursorApp()

      return { status: 'success' }
    } catch (error) {
      console.error('强制切换账户失败:', error)
      throw error
    } finally {
      needSaveCurrentAccount.value = false
      isForceKilling.value = false
      historyStore.switchingAccount[account.email] = false
    }
  }

  /**
   * 处理文件选择
   */
  async function handleSelectCursorPath() {
    // 不在这里调用useMessage，而是通过外部传入或通过事件处理
    if (fileSelectLoading.value) return

    fileSelectLoading.value = true
    fileSelectError.value = ''

    try {
      // 调用文件选择对话框
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: 'Cursor程序',
            extensions: ['exe'],
          },
          {
            name: 'JavaScript文件',
            extensions: ['js'],
          },
          {
            name: '所有文件',
            extensions: ['*'],
          },
        ],
      })

      // 检查用户是否取消了选择
      if (!selected) {
        fileSelectLoading.value = false
        return
      }

      // 调用API处理选择的文件路径
      const result = await findCursorPath(selected as string)

      if (result) {
        showSelectFileModal.value = false

        // 如果有待处理的操作，执行它
        if (pendingAction.value) {
          // 保存然后清空待处理操作
          const currentAction = {
            ...pendingAction.value,
          }
          pendingAction.value = null

          try {
            // 根据待处理操作类型执行相应的方法
            switch (currentAction.type) {
              case 'applyHook':
                await applyHookToClient(currentAction.params?.forceKill || false)
                break
              case 'restoreHook':
                await restoreHookFromClient(currentAction.params?.forceKill || false)
                break
              // 可以添加其他操作类型的处理...
            }

            // 强制重新获取Hook状态以刷新UI
            await checkHook()
          } catch (actionError) {
            console.error('执行操作失败:', actionError)
            fileSelectError.value =
              '执行操作失败: ' +
              (actionError instanceof Error ? actionError.message : String(actionError))
          }
        }

        // 操作完成后，设置加载状态为false
        fileSelectLoading.value = false
      } else {
        throw new Error('无法验证所选择的文件路径')
      }
    } catch (error) {
      console.error('文件选择处理错误:', error)
      fileSelectError.value = error instanceof Error ? error.message : String(error)
      fileSelectLoading.value = false
    }
  }

  /**
   * 设置待执行的操作
   */
  function setPendingAction(type: string, params?: any) {
    pendingAction.value = {
      type,
      params,
    }
    showSelectFileModal.value = true
  }

  /**
   * 检查Cursor是否正在运行，如果正在运行且不允许强制关闭则抛出错误
   */
  async function ensureCursorNotRunning(forceKill: boolean) {
    if (!forceKill && (await checkCursorRunning())) {
      throw new Error('Cursor进程正在运行, 请先关闭Cursor')
    }
  }

  /**
   * 注入正在运行的Cursor
   * 获取正在运行的Cursor路径，检查它是否已被注入，如果没有则执行注入操作
   */
  async function injectRunningCursor() {
    try {
      operationLoading.value = true
      await Logger.info('开始注入正在运行的Cursor')

      // 获取正在运行的Cursor路径
      const cursorExePath = await getRunningCursorPath()
      await Logger.info(`获取到的Cursor可执行文件路径: ${cursorExePath}`)

      // 保存路径到数据库
      if (cursorExePath) {
        await findCursorPath(cursorExePath)

        // 检查当前注入状态
        const isCurrentlyHooked = await checkHookStatus()

        // 如果已经注入，显示信息并返回
        if (isCurrentlyHooked) {
          await Logger.info('Cursor已经被注入，无需重复操作')
          hookStatus.value = true
          return true
        }

        // 执行注入操作
        await applyHook(true)
        await Logger.info('注入操作完成')

        // 更新hook状态
        hookStatus.value = true
        await Logger.info('成功注入正在运行的Cursor')

        try {
          await launchCursorApp()
          await Logger.info('Cursor已重新启动')
        } catch (launchError) {
          console.error('重新启动Cursor失败:', launchError)
          await Logger.error(`重新启动Cursor失败: ${launchError}`)
        }

        return true
      } else {
        throw new Error('未能获取正在运行的Cursor路径')
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error)
      console.error('注入失败:', errorMsg)
      await Logger.error(`注入正在运行的Cursor失败: ${errorMsg}`)

      // 如果操作失败，尝试更新hook状态以确保UI显示正确
      try {
        await checkHook()
      } catch (checkError) {
        console.error('检查Hook状态也失败:', checkError)
      }

      throw error
    } finally {
      operationLoading.value = false
    }
  }

  /**
   * 打开macOS系统偏好设置权限面板
   */
  async function openMacOSPermissionSettings() {
    try {
      // 在macOS上使用命令行打开系统偏好设置
      const result = await Command.create('exec-sh', [
        '-c',
        "open 'x-apple.systempreferences:com.apple.preference.security?Privacy_AppBundles'",
      ]).execute()

      if (result.code === 0) {
        return true
      } else {
        console.error('打开macOS权限设置失败，错误码:', result.code)
        return false
      }
    } catch (error) {
      console.error('打开macOS权限设置失败:', error)
      return false
    }
  }

  return {
    // 状态
    machineCode,
    currentAccount,
    cursorToken,
    cursorInfo,
    isLoading,
    hookStatus,
    operationLoading,
    machineCodeLoading,
    accountSwitchLoading,
    quickChangeLoading,
    isForceKilling,
    needSaveCurrentAccount,
    macOSPermissionError,

    // 添加文件选择模态框状态
    showSelectFileModal,
    fileSelectError,
    fileSelectLoading,
    pendingAction,

    // Getters
    gpt4Usage,
    gpt35Usage,
    isHooked,
    formatDate,

    // Actions
    fetchMachineIds,
    fetchCursorUsage,
    resetMachine,
    switchCursorAccount,
    quickChange,
    checkHook,
    applyHookToClient,
    restoreHookFromClient,
    closeCursorApp,
    launchCursorApp,
    ensureHookApplied,
    refreshAllCursorData,
    switchToHistoryAccount,
    forceCloseAndSwitch,
    openMacOSPermissionSettings,

    // 添加文件选择相关方法
    handleSelectCursorPath,
    setPendingAction,

    // 注入正在运行的Cursor
    injectRunningCursor,
  }
})
