<script setup lang="ts">
  import { ref, onMounted, h } from 'vue'
  import { useMessage } from 'naive-ui'
  import type { DataTableColumns } from 'naive-ui'
  import { NDataTable, NSpace, NButton, NCard, NProgress } from 'naive-ui'
  import type { HistoryAccount } from '@/types/history'
  import type { PendingForceKillAction } from '@/types/dashboard'
  import { useHistoryStore } from '@/stores/history'
  import { useCursorStore } from '@/stores/cursor'
  import CursorRunningModal from '../components/CursorRunningModal.vue'
  import { useI18n } from '../locales'

  const message = useMessage()
  const historyStore = useHistoryStore()
  const cursorStore = useCursorStore()
  const { t } = useI18n()

  // 模态框状态
  const showCursorRunningModal = ref(false)
  const pendingAccount = ref<HistoryAccount | null>(null)

  // 计算使用率
  const calculateUsagePercent = (count: number, maxUsage: number | null | undefined) => {
    // 确保count是数字
    if (typeof count === 'string') {
      count = parseInt(count, 10) || 0
    }

    // 确保count为非负数
    count = Math.max(0, count || 0)

    // 确保maxUsage是有效数字且不为0
    if (maxUsage === undefined || maxUsage === null || maxUsage <= 0 || isNaN(Number(maxUsage))) {
      maxUsage = count > 150 ? count : 150 // 默认值150与API返回的常见值一致
    } else {
      maxUsage = Number(maxUsage)
    }

    // 计算百分比并取整
    const percent = (count / maxUsage) * 100
    return Math.round(Math.min(percent, 100))
  }

  const columns: DataTableColumns<HistoryAccount> = [
    {
      title: t('historyAccount.email'),
      key: 'email',
    },
    {
      title: t('historyAccount.machineCode'),
      key: 'machineCode',
      render(row) {
        const code = row.machineCode
        return code.slice(0, 4) + '**' + code.slice(-4)
      },
    },
    {
      title: t('historyAccount.advancedModelUsage'),
      key: 'gpt4Count',
      width: 150,
      render(row) {
        const percent = calculateUsagePercent(row.gpt4Count, row.gpt4MaxUsage)
        const color = percent >= 90 ? 'error' : percent >= 70 ? 'warning' : 'success'

        return h(NProgress, {
          type: 'line',
          percentage: percent,
          status: color,
          indicatorPlacement: 'inside',
          height: 12,
          showIndicator: false,
        })
      },
    },
    {
      title: t('historyAccount.actions'),
      key: 'actions',
      render(row) {
        return h(NSpace, null, {
          default: () => [
            h(
              NButton,
              {
                size: 'small',
                loading: historyStore.switchingAccount[row.email] || false,
                onClick: () => handleSwitch(row),
              },
              {
                default: () => t('historyAccount.switchButton'),
              },
            ),
            h(
              NButton,
              {
                size: 'small',
                type: 'error',
                loading: historyStore.deletingAccount[row.email] || false,
                onClick: () => handleRemove(row),
              },
              {
                default: () => t('historyAccount.deleteButton'),
              },
            ),
          ],
        })
      },
    },
  ]

  async function handleSwitch(account: HistoryAccount) {
    try {
      const result = await cursorStore.switchToHistoryAccount(account)

      if (result.status === 'running') {
        pendingAccount.value = account
        showCursorRunningModal.value = true
        return
      }

      if (result.status === 'hook_failed') {
        message.error(t('historyAccount.hookFailed'))
        return
      }

      if (result.status === 'success') {
        message.success(t('historyAccount.switchSuccess'))
        window.location.reload()
      }
    } catch (error) {
      message.error(t('historyAccount.switchFailed'))
      console.error('切换账户失败:', error)
    }
  }

  async function handleRemove(account: HistoryAccount) {
    try {
      await historyStore.removeHistoryAccountItem(account.email)
      message.success(t('historyAccount.deleteSuccess'))
    } catch (error) {
      message.error(t('historyAccount.deleteFailed'))
      console.error('删除账户失败:', error)
    }
  }

  async function handleClearHighUsageAccounts() {
    if (historyStore.highUsageAccounts.length === 0) {
      message.info(t('historyAccount.noHighUsageAccounts'))
      return
    }

    try {
      const result = await historyStore.clearHighUsageAccounts()
      message.success(t('historyAccount.clearHighUsageSuccess', { count: result.success }))
    } catch (error) {
      message.error(t('historyAccount.clearHighUsageFailed'))
      console.error('清理高使用量账户失败:', error)
    }
  }

  async function refreshUsage() {
    try {
      const result = await historyStore.refreshAccountsUsage()

      if (result.success === result.total) {
        message.success(t('historyAccount.refreshSuccess'))
      } else {
        message.warning(
          t('historyAccount.refreshPartial', {
            success: result.success,
            total: result.total,
          }),
        )
      }
    } catch (error) {
      message.error(t('historyAccount.refreshFailed'))
      console.error('刷新使用情况失败:', error)
    }
  }

  const handleForceKill = async () => {
    if (!pendingAccount.value) return

    try {
      showCursorRunningModal.value = false
      const account = pendingAccount.value

      // 使用 store 中的方法
      const result = await cursorStore.forceCloseAndSwitch(account)

      if (result.status === 'hook_failed') {
        message.error(t('historyAccount.hookFailed'))
        return
      }

      if (result.status === 'success') {
        message.success(t('historyAccount.switchSuccess'))
        window.location.reload()
      }
    } catch (error) {
      message.error(t('historyAccount.switchFailed'))
      console.error('切换账户失败:', error)
    } finally {
      pendingAccount.value = null
    }
  }

  onMounted(async () => {
    try {
      await historyStore.fetchHistoryAccounts(false)
    } catch (error) {
      console.error('加载历史账户失败:', error)
      message.error(t('historyAccount.loadFailed'))
    }

    window.addEventListener('force_kill_cursor', async (e: Event) => {
      const detail = (e as CustomEvent).detail as PendingForceKillAction
      if (detail.type === 'account') {
        await handleForceKill()
      }
    })
  })
</script>

<template>
  <n-space
    vertical
    :size="24"
  >
    <n-card :title="t('historyAccount.title')">
      <template #header-extra>
        <n-space>
          <n-button
            :loading="historyStore.loadingAccounts"
            type="primary"
            @click="refreshUsage"
          >
            {{ t('historyAccount.refreshAll') }}
          </n-button>
          <n-button
            :loading="historyStore.clearingHighUsage"
            type="error"
            :disabled="historyStore.highUsageAccounts.length === 0"
            @click="handleClearHighUsageAccounts"
          >
            {{ t('historyAccount.clearHighUsage') }} ({{ historyStore.highUsageAccounts.length }})
          </n-button>
        </n-space>
      </template>
      <n-data-table
        :columns="columns"
        :data="historyStore.filteredHistoryAccounts"
        :loading="historyStore.loadingAccounts"
        :bordered="false"
        :pagination="{
          pageSize: 10,
        }"
      />
    </n-card>
  </n-space>

  <cursor-running-modal
    v-model:show="showCursorRunningModal"
    :title="t('common.cursorRunning')"
    :content="t('common.cursorRunningMessage')"
    :confirm-button-text="t('common.forceClose')"
    @confirm="handleForceKill"
  />
</template>
