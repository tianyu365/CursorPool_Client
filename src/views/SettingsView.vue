<script setup lang="ts">
  import { ref, onMounted, nextTick, watch, computed } from 'vue'
  import {
    NCard,
    NSpace,
    NForm,
    NFormItem,
    NInput,
    NButton,
    NInputGroup,
    useMessage,
    NSpin,
    NSelect,
    NTag,
    NTooltip,
  } from 'naive-ui'
  import { useI18n } from '../locales'
  import LanguageSwitch from '../components/LanguageSwitch.vue'
  import InboundSelector from '../components/InboundSelector.vue'
  import CloseTypeSelector from '../components/CloseTypeSelector.vue'
  import CursorRunningModal from '../components/CursorRunningModal.vue'
  import { changePassword, activate, checkCursorRunning, restoreHook, getUserData } from '@/api'
  import { addHistoryRecord } from '../utils/history'
  import { version } from '../../package.json'
  import { useUserStore } from '../stores/user'
  import { useCursorStore } from '../stores'
  import FileSelectModal from '../components/FileSelectModal.vue'
  import { useRouter } from 'vue-router'
  import { useAppStore } from '../stores/app'

  const message = useMessage()
  const { t } = useI18n()
  const userStore = useUserStore()
  const cursorStore = useCursorStore()
  const router = useRouter()
  const appStore = useAppStore()
  const cursorPath = ref<string | null>(null)

  interface SettingsForm {
    activationCode: string
    currentPassword: string
    newPassword: string
    confirmPassword: string
  }

  const formValue = ref<SettingsForm>({
    activationCode: '',
    currentPassword: '',
    newPassword: '',
    confirmPassword: '',
  })

  // 修改控制状态
  const controlStatus = ref({
    isHooked: false,
    isChecking: false,
  })

  // 为每个操作添加单独的加载状态
  const applyHookLoading = ref(false)
  const restoreHookLoading = ref(false)

  const showControlRunningModal = ref(false)
  const pendingControlAction = ref<'applyHook' | 'restoreHook' | null>(null)

  // 为激活和修改密码添加独立的加载状态
  const activateLoading = ref(false)
  const passwordChangeLoading = ref(false)

  // 添加按钮模式选项
  const buttonModeOptions = computed(() => [
    {
      label: t('settings.simpleMode'),
      value: 'simple',
    },
    {
      label: t('settings.advancedMode'),
      value: 'advanced',
    },
  ])

  // 计算属性用于转换值
  const buttonMode = computed({
    get: () => (appStore.showAllButtons ? 'advanced' : 'simple'),
    set: (value: string) => {
      appStore.setButtonVisibility(value === 'advanced')
    },
  })

  const handleActivate = async () => {
    if (!formValue.value.activationCode) {
      message.error(t('message.pleaseInputActivationCode'))
      return
    }

    activateLoading.value = true
    try {
      await activate(formValue.value.activationCode)
      message.success(t('message.activationSuccess'))
      addHistoryRecord(t('settings.activation'), t('message.activationSuccess'))
      formValue.value.activationCode = ''

      // 设置刷新标记，确保dashboard页面刷新数据
      localStorage.setItem('need_refresh_dashboard', 'true')

      // 激活成功后跳转到 dashboard 页面
      router.push('/dashboard')
    } catch (error) {
      message.error(error instanceof Error ? error.message : t('message.activationFailed'))
    } finally {
      activateLoading.value = false
    }
  }

  const handleChangePassword = async () => {
    if (
      !formValue.value.currentPassword ||
      !formValue.value.newPassword ||
      !formValue.value.confirmPassword
    ) {
      message.error(t('message.pleaseInputPassword'))
      return
    }

    if (formValue.value.newPassword !== formValue.value.confirmPassword) {
      message.error(t('message.passwordNotMatch'))
      return
    }

    passwordChangeLoading.value = true
    try {
      await changePassword(formValue.value.currentPassword, formValue.value.newPassword)
      message.success(t('message.passwordChangeSuccess'))

      formValue.value.currentPassword = ''
      formValue.value.newPassword = ''
      formValue.value.confirmPassword = ''

      await handleLogout()
    } catch (error) {
      message.error(error instanceof Error ? error.message : t('message.passwordChangeFailed'))
    } finally {
      passwordChangeLoading.value = false
    }
  }

  const handleLogout = async () => {
    try {
      await userStore.logout()
      addHistoryRecord(t('history.userOperation'), t('common.logout'))
    } catch (error) {
      message.error(t('common.logoutFailed'))
    }
  }

  // 修改 handleControlAction 函数
  const handleControlAction = async (
    action: 'applyHook' | 'restoreHook',
    force_kill: boolean = false,
  ) => {
    // 根据操作设置对应的加载状态
    const loadingRef = {
      applyHook: applyHookLoading,
      restoreHook: restoreHookLoading,
    }[action]

    try {
      loadingRef.value = true

      let successMessage = ''
      let historyAction = ''

      // 根据操作类型执行相应的操作
      switch (action) {
        case 'applyHook': {
          const result = await cursorStore.applyHookToClient(force_kill)

          if (result.status === 'success') {
            successMessage = t('systemControl.messages.applyHookSuccess')
            historyAction = t('systemControl.history.applyHook')
            controlStatus.value.isHooked = true

            message.success(successMessage)
            addHistoryRecord(t('systemControl.title'), historyAction)
          } else if (result.status === 'running') {
            showControlRunningModal.value = true
            pendingControlAction.value = action
            return
          } else if (result.status === 'need_select_file') {
            return
          } else if (
            result.status === 'error' &&
            result.errorType === cursorStore.macOSPermissionError
          ) {
            // 处理macOS权限错误
            message.error('无法终止Cursor进程，需要系统权限')
            cursorStore.setPendingAction(action, {
              forceKill: force_kill,
              errorType: cursorStore.macOSPermissionError,
              message: result.message,
            })
            cursorStore.showSelectFileModal = true
            return
          }
          break
        }

        case 'restoreHook': {
          if (!force_kill) {
            const isRunning = await checkCursorRunning()
            if (isRunning) {
              showControlRunningModal.value = true
              pendingControlAction.value = action
              return
            }
          }

          try {
            await restoreHook(force_kill)
            successMessage = t('systemControl.messages.restoreHookSuccess')
            historyAction = t('systemControl.history.restoreHook')
            controlStatus.value.isHooked = false

            // 显示成功消息
            message.success(successMessage)
            showControlRunningModal.value = false
            addHistoryRecord(t('systemControl.title'), historyAction)
          } catch (error) {
            // 获取完整的错误信息
            const errorMsg = error instanceof Error ? error.message : String(error)

            // 检查是否包含MAIN_JS_NOT_FOUND
            if (errorMsg.includes('MAIN_JS_NOT_FOUND') || errorMsg.includes('创建应用路径失败')) {
              cursorStore.setPendingAction(action, { forceKill: force_kill })
              return
            }
            throw error
          }
          break
        }
      }

      // 操作完成后重新检查状态
      await checkControlStatus()
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error)
      console.error('控制操作错误:', errorMsg)
      message.error(errorMsg)
    } finally {
      loadingRef.value = false
    }
  }

  // 检查控制状态
  const checkControlStatus = async () => {
    try {
      // 添加loading状态
      controlStatus.value.isChecking = true

      // 使用cursorStore的checkHook方法，保持状态同步
      const hookResult = await cursorStore.checkHook()
      // 处理可能为null的情况
      controlStatus.value.isHooked = hookResult === true

      // 检查Cursor路径
      await getCursorPath()

      // 确保UI反映最新状态
      await nextTick()
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error)
      console.error('检查控制状态失败:', errorMsg)
    } finally {
      // 完成检查，移除loading状态
      controlStatus.value.isChecking = false
    }
  }

  // 获取Cursor路径
  const getCursorPath = async () => {
    try {
      // 从数据库读取cursor路径
      const path = await getUserData('system.cursor.path.mainJs')
      cursorPath.value = path
    } catch (error) {
      console.error('获取Cursor路径失败:', error)
      cursorPath.value = null
    }
  }

  // 处理强制关闭
  const handleControlForceKill = async () => {
    if (pendingControlAction.value) {
      try {
        // 关闭确认对话框
        showControlRunningModal.value = false
        // 设置为加载状态
        const loadingRef = {
          applyHook: applyHookLoading,
          restoreHook: restoreHookLoading,
        }[pendingControlAction.value]
        loadingRef.value = true

        // 使用强制关闭参数调用操作
        await handleControlAction(pendingControlAction.value, true)
      } catch (error) {
        console.error('强制关闭操作失败:', error)
        message.error(t('systemControl.messages.forceFailed'))
      }
    }
  }

  // 监听cursorStore的hookStatus变化
  watch(
    () => cursorStore.hookStatus,
    (newStatus) => {
      if (newStatus !== null) {
        controlStatus.value.isHooked = newStatus
      }
    },
    { immediate: true },
  )

  // 监听文件选择模态框的显示状态
  watch(
    () => cursorStore.showSelectFileModal,
    (newValue, oldValue) => {
      if (oldValue && !newValue) {
        checkControlStatus()
      }
    },
  )

  // 在组件挂载时检查控制状态
  onMounted(async () => {
    // 初始化状态
    controlStatus.value = {
      isHooked: cursorStore.hookStatus ?? false,
      isChecking: false,
    }

    // 检查Hook状态
    await checkControlStatus()
  })

  // 修改处理函数
  const handleButtonVisibilityChange = async (value: string) => {
    try {
      await appStore.setButtonVisibility(value === 'advanced')
      message.success(
        value === 'advanced' ? t('settings.switchedToAdvanced') : t('settings.switchedToSimple'),
      )
    } catch (error) {
      message.error(t('settings.settingsFailed'))
    }
  }
</script>

<template>
  <n-space
    vertical
    :size="24"
  >
    <!-- 系统控制 -->
    <n-card>
      <template #header>
        <div class="text-xl font-medium">{{ t('systemControl.title') }}</div>
      </template>
      <n-space
        vertical
        :size="16"
      >
        <div class="flex items-center justify-between">
          <div>
            {{ t('systemControl.clientStatus') }}:
            <template v-if="controlStatus.isChecking">
              <n-spin size="small" />
            </template>
            <template v-else>
              <n-tag
                :type="controlStatus.isHooked ? 'success' : 'warning'"
                size="small"
                round
              >
                {{
                  controlStatus.isHooked
                    ? t('systemControl.hookApplied')
                    : t('systemControl.hookNotApplied')
                }}
              </n-tag>
            </template>
          </div>
          <div class="flex gap-2">
            <n-button
              type="warning"
              :loading="applyHookLoading"
              :disabled="controlStatus.isHooked"
              @click="handleControlAction('applyHook')"
            >
              {{ t('systemControl.applyHook') }}
            </n-button>
            <n-button
              type="primary"
              :loading="restoreHookLoading"
              :disabled="!controlStatus.isHooked"
              @click="handleControlAction('restoreHook')"
            >
              {{ t('systemControl.restoreHook') }}
            </n-button>
          </div>
        </div>

        <!-- 注入Cursor路径 -->
        <div v-if="controlStatus.isHooked && cursorPath && appStore.currentPlatform === 'windows'">
          <div class="flex items-center justify-between gap-2 mt-1">
            <div
              class="text-sm overflow-hidden text-ellipsis whitespace-nowrap"
              style="flex: 1"
            >
              <n-tooltip
                placement="bottom"
                trigger="hover"
                style="max-width: 400px; white-space: normal; word-break: break-word"
              >
                <template #trigger>
                  <span class="cursor-pointer"
                    >{{ t('systemControl.cursorPath') }}{{ cursorPath }}</span
                  >
                </template>
                <div style="white-space: normal; word-break: break-all">{{ cursorPath }}</div>
              </n-tooltip>
            </div>
            <n-button
              type="primary"
              @click="cursorStore.showSelectFileModal = true"
            >
              {{ t('systemControl.changePath') }}
            </n-button>
          </div>
        </div>
      </n-space>
    </n-card>

    <!-- 全局偏好设置 -->
    <n-card>
      <template #header>
        <div class="text-xl font-medium">{{ t('settings.globalPreferences') }}</div>
      </template>
      <div class="p-5">
        <div class="grid grid-cols-2 gap-x-8 gap-y-6">
          <div class="flex items-center">
            <div class="text-base w-20">{{ t('inbound.title') }}</div>
            <div class="flex-1">
              <inbound-selector
                :show-label="false"
                class="settings-selector"
              />
            </div>
          </div>
          <div class="flex items-center">
            <div class="text-base w-20">{{ t('settings.closeMethod') }}</div>
            <div class="flex-1">
              <close-type-selector
                :show-label="false"
                class="settings-selector"
              />
            </div>
          </div>
          <div class="flex items-center">
            <div class="text-base w-20">{{ t('language.title') }}</div>
            <div class="flex-1">
              <language-switch
                :show-label="false"
                class="settings-selector"
              />
            </div>
          </div>
          <div class="flex items-center">
            <div class="text-base w-20">{{ t('settings.operationMode') }}</div>
            <div class="flex-1">
              <n-select
                v-model:value="buttonMode"
                :options="buttonModeOptions"
                size="small"
                class="w-full"
                @update:value="handleButtonVisibilityChange"
              />
            </div>
          </div>
        </div>
      </div>
    </n-card>

    <!-- 激活码兑换 -->
    <n-card>
      <template #header>
        <div class="text-xl font-medium">{{ t('settings.activation') }}</div>
      </template>
      <n-space
        vertical
        :size="16"
      >
        <div class="flex items-center justify-between">
          <div style="width: 80px">{{ t('settings.activationCode') }}</div>
          <div class="flex-1">
            <n-input-group>
              <n-input
                v-model:value="formValue.activationCode"
                :placeholder="t('message.pleaseInputActivationCode')"
                class="flex-1"
              />
              <n-button
                type="primary"
                :loading="activateLoading"
                @click="handleActivate"
              >
                {{ t('settings.activate') }}
              </n-button>
            </n-input-group>
          </div>
        </div>
      </n-space>
    </n-card>

    <!-- 密码修改 -->
    <n-card>
      <template #header>
        <div class="text-xl font-medium">{{ t('settings.changePassword') }}</div>
      </template>
      <n-space
        vertical
        :size="16"
      >
        <n-form
          :model="formValue"
          label-placement="left"
          label-width="100"
          require-mark-placement="right-hanging"
        >
          <n-form-item :label="t('settings.currentPassword')">
            <n-input
              v-model:value="formValue.currentPassword"
              type="password"
              show-password-on="click"
              :placeholder="t('settings.currentPassword')"
              maxlength="20"
              minlength="6"
              :allow-input="(value) => /^[a-zA-Z0-9]*$/.test(value)"
            />
          </n-form-item>

          <n-form-item :label="t('settings.newPassword')">
            <n-input
              v-model:value="formValue.newPassword"
              type="password"
              show-password-on="click"
              :placeholder="t('settings.newPassword')"
              maxlength="20"
              minlength="6"
              :allow-input="(value) => /^[a-zA-Z0-9]*$/.test(value)"
            />
          </n-form-item>

          <n-form-item :label="t('settings.confirmPassword')">
            <n-input
              v-model:value="formValue.confirmPassword"
              type="password"
              show-password-on="click"
              :placeholder="t('settings.confirmPassword')"
              maxlength="20"
              minlength="6"
              :allow-input="(value) => /^[a-zA-Z0-9]*$/.test(value)"
            />
          </n-form-item>

          <div style="margin-top: 12px">
            <n-space>
              <n-button
                type="primary"
                :loading="passwordChangeLoading"
                @click="handleChangePassword"
              >
                {{ t('settings.changePassword') }}
              </n-button>
              <n-button
                type="error"
                @click="handleLogout"
              >
                {{ t('common.logout') }}
              </n-button>
            </n-space>
          </div>
        </n-form>
      </n-space>
    </n-card>

    <!-- 关于 -->
    <n-card>
      <template #header>
        <div class="text-xl font-medium">{{ t('settings.about') }}</div>
      </template>
      <n-space
        vertical
        :size="12"
      >
        <p>{{ t('about.appName') }} v{{ version }}</p>
        <p>
          {{ t('about.copyright') }} ©
          {{ new Date().getFullYear() }}
          <n-button
            text
            tag="a"
            href="https://github.com/Sanyela"
            target="_blank"
          >
            Sanyela
          </n-button>
          &
          <n-button
            text
            tag="a"
            href="https://github.com/Cloxl"
            target="_blank"
            >Cloxl</n-button
          >
        </p>
        <p>{{ t('about.license') }}</p>
      </n-space>
    </n-card>

    <!-- 模态框 -->
    <cursor-running-modal
      v-model:show="showControlRunningModal"
      :title="t('common.cursorRunning')"
      :content="t('common.cursorRunningMessage')"
      :confirm-button-text="t('common.forceClose')"
      @confirm="handleControlForceKill"
    />

    <!-- 文件选择模态框 -->
    <file-select-modal />
  </n-space>
</template>

<style scoped>
  /* 使用naive-ui变量和unocss类保持一致的样式 */

  /* 覆盖自定义组件内部的固定宽度样式 */
  .settings-selector :deep(.n-select) {
    width: 100% !important;
  }

  /* 确保所有选择器内部的下拉菜单宽度是响应式的 */
  .settings-selector :deep(.n-base-selection) {
    width: 100% !important;
  }
</style>
