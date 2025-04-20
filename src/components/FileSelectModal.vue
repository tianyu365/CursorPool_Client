<script setup lang="ts">
  import { NModal, NSpace, NIcon, useMessage, NButton, useThemeVars } from 'naive-ui'
  import { useCursorStore, useAppStore } from '@/stores'
  import { watch, computed, ref } from 'vue'
  import { useTheme } from '../composables/theme'
  import {
    DocumentOutline,
    InformationCircleOutline,
    RefreshOutline,
    Warning,
    ShieldCheckmarkOutline,
  } from '@vicons/ionicons5'
  import { open } from '@tauri-apps/plugin-shell'

  // 获取主题变量
  const { isDarkMode } = useTheme()
  const themeVars = useThemeVars()
  const appStore = useAppStore()

  // 标记是否显示macOS权限错误界面
  const showMacOSPermissionError = ref(false)
  // 标记是否已尝试赋予权限
  const permissionGrantedAttempted = ref(false)

  // 计算主题相关样式
  const cardStyle = computed(() => ({
    backgroundColor: isDarkMode.value ? themeVars.value.cardColor : '#f5f5f7',
  }))

  // 获取 Cursor Store
  const cursorStore = useCursorStore()
  // 获取消息组件
  const message = useMessage()

  // 监听文件选择状态变化，提供消息反馈
  watch(
    () => cursorStore.showSelectFileModal,
    (newValue) => {
      if (newValue) {
        // 模态框显示时重置错误
        cursorStore.fileSelectError = ''
        // 重置macOS权限错误状态
        showMacOSPermissionError.value = false
        permissionGrantedAttempted.value = false
      }
    },
  )

  // 处理文件选择
  const handleSelectPath = async () => {
    await cursorStore.handleSelectCursorPath()

    // 处理成功状态
    if (!cursorStore.showSelectFileModal && !cursorStore.fileSelectError) {
      message.success('文件选择成功，系统已找到并保存Cursor路径')

      // 检查是否有待处理操作
      if (cursorStore.pendingAction) {
        message.loading(`正在执行${cursorStore.pendingAction.type}操作...`)

        // 等待操作完成
        setTimeout(() => {
          if (!cursorStore.fileSelectError) {
            if (cursorStore.pendingAction?.type === 'applyHook') {
              message.success('Hook应用成功！')
            } else if (cursorStore.pendingAction?.type === 'restoreHook') {
              message.success('Hook恢复成功！')
            }
          }
        }, 1000)
      }
    } else if (cursorStore.fileSelectError) {
      // 显示错误消息
      message.error(cursorStore.fileSelectError)
    }
  }

  const handleOpenDocs = () => {
    if (appStore.currentPlatform === 'windows') {
      open('https://docs.52ai.org/troubleshooting/windows/injection')
    } else if (appStore.currentPlatform === 'macos') {
      open('https://docs.52ai.org/troubleshooting/macos/permissions')
    }
  }

  // 打开macOS权限设置
  const handleOpenMacOSPermissionSettings = async () => {
    try {
      message.loading('正在打开系统偏好设置...')
      await cursorStore.openMacOSPermissionSettings()
      message.success('已打开系统偏好设置，请给予应用所需权限')
    } catch (error) {
      message.error('打开系统偏好设置失败，请手动打开')
    }
  }

  // 重试注入操作
  const handleRetryAfterPermissionGrant = async () => {
    permissionGrantedAttempted.value = true
    message.loading('正在尝试使用新权限重新注入...')

    try {
      // 如果有待处理的操作，执行它
      if (cursorStore.pendingAction) {
        const action = cursorStore.pendingAction.type

        if (action === 'applyHook') {
          const result = await cursorStore.applyHookToClient(true) // 强制关闭

          if (result.status === 'success') {
            message.success('Hook应用成功！')
            cursorStore.showSelectFileModal = false
          } else if (
            result.status === 'error' &&
            result.errorType === cursorStore.macOSPermissionError
          ) {
            message.error('权限验证失败，请确保已正确授予权限')
          } else if (result.status === 'running') {
            message.warning('Cursor仍在运行，请手动关闭Cursor后重试')
          }
        }
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error)
      message.error(errorMsg)
      cursorStore.fileSelectError = errorMsg
    }
  }

  // 处理注入正在运行的Cursor
  const handleInjectRunningCursor = async () => {
    message.loading('正在尝试注入运行中的Cursor...')
    try {
      await cursorStore.injectRunningCursor()
      message.success('成功注入正在运行的Cursor!')

      // 关闭模态框
      cursorStore.showSelectFileModal = false
    } catch (error) {
      // 显示错误消息
      const errorMsg = error instanceof Error ? error.message : String(error)
      message.error(errorMsg || '注入失败，请尝试手动选择文件')
      cursorStore.fileSelectError = errorMsg || '注入失败，请尝试手动选择文件'
    }
  }

  // 关闭模态框
  const closeModal = () => {
    cursorStore.showSelectFileModal = false
  }

  // 设置macOS权限界面显示
  const setMacOSPermissionError = (show: boolean) => {
    showMacOSPermissionError.value = show
    permissionGrantedAttempted.value = false
  }

  // 检查错误是否为macOS权限错误
  watch(
    () => cursorStore.pendingAction,
    (newAction) => {
      // 如果待处理操作产生了错误，检查是否是macOS权限错误
      if (
        newAction &&
        newAction.params &&
        newAction.params.errorType === cursorStore.macOSPermissionError
      ) {
        setMacOSPermissionError(true)
      }
    },
  )
</script>

<template>
  <!-- 文件选择模态框 -->
  <n-modal
    v-model:show="cursorStore.showSelectFileModal"
    preset="card"
    :title="showMacOSPermissionError ? '需要系统权限' : '选择Cursor路径'"
    :bordered="false"
    style="width: 550px"
    @close="closeModal"
  >
    <!-- macOS权限错误界面 -->
    <n-space
      v-if="showMacOSPermissionError && appStore.currentPlatform === 'macos'"
      vertical
    >
      <div
        class="text-base"
        :style="{ color: themeVars.textColor1 }"
      >
        无法终止Cursor进程，需要授予系统权限才能继续操作
      </div>

      <div style="margin-top: 12px">
        <n-space
          vertical
          :size="12"
        >
          <div
            class="option-item"
            :style="cardStyle"
          >
            <div
              class="option-title"
              :style="{ color: themeVars.textColor1 }"
            >
              <n-icon
                size="20"
                class="option-icon"
                :color="themeVars.primaryColor"
              >
                <ShieldCheckmarkOutline />
              </n-icon>
              <b>授予App管理权限</b>
            </div>
            <div
              class="option-desc"
              :style="{ color: themeVars.textColor3 }"
            >
              请在系统偏好设置中为应用授予必要的权限，以便能够管理Cursor进程
            </div>
          </div>

          <div
            v-if="permissionGrantedAttempted"
            class="option-item"
            :style="cardStyle"
          >
            <div
              class="option-title"
              :style="{ color: themeVars.textColor1 }"
            >
              <n-icon
                size="20"
                class="option-icon"
                :color="themeVars.warningColor"
              >
                <Warning />
              </n-icon>
              <b>权限验证失败</b>
            </div>
            <div
              class="option-desc"
              :style="{ color: themeVars.textColor3 }"
            >
              权限验证失败，请尝试以下步骤：
              <ol style="margin-top: 8px; padding-left: 16px">
                <li>确保已在系统偏好设置中勾选本应用</li>
                <li>尝试重启应用后再次授权</li>
                <li>手动关闭所有Cursor进程后重试</li>
              </ol>
            </div>
          </div>
        </n-space>
      </div>

      <div
        v-if="cursorStore.fileSelectError"
        :class="['error-container', isDarkMode ? 'dark' : 'light']"
      >
        <n-icon
          size="16"
          style="margin-right: 6px"
        >
          <Warning />
        </n-icon>
        {{ cursorStore.fileSelectError }}
      </div>

      <div style="margin-top: 20px; display: flex; justify-content: flex-end; gap: 12px">
        <n-button
          type="primary"
          @click="handleOpenDocs"
        >
          打开文档
        </n-button>
        <n-button
          type="primary"
          @click="handleOpenMacOSPermissionSettings"
        >
          打开系统设置
        </n-button>
        <n-button
          type="primary"
          :loading="cursorStore.operationLoading"
          @click="handleRetryAfterPermissionGrant"
        >
          我已授权，重试
        </n-button>
      </div>
    </n-space>

    <!-- Windows/默认界面 -->
    <n-space
      v-else
      vertical
    >
      <div
        class="text-base"
        :style="{ color: themeVars.textColor1 }"
      >
        需要设置Cursor路径以完成注入操作，请选择以下方式之一：
      </div>

      <div style="margin-top: 12px">
        <n-space
          vertical
          :size="12"
        >
          <div
            class="option-item"
            :style="cardStyle"
          >
            <div
              class="option-title"
              :style="{ color: themeVars.textColor1 }"
            >
              <n-icon
                size="20"
                class="option-icon"
                :color="themeVars.primaryColor"
              >
                <DocumentOutline />
              </n-icon>
              <b>选择Cursor程序</b>
            </div>
            <div
              class="option-desc"
              :style="{ color: themeVars.textColor3 }"
            >
              直接选择cursor.exe程序文件，系统会自动查找main.js的位置
            </div>
          </div>

          <div
            class="option-item"
            :style="cardStyle"
          >
            <div
              class="option-title"
              :style="{ color: themeVars.textColor1 }"
            >
              <n-icon
                size="20"
                class="option-icon"
                :color="themeVars.primaryColor"
              >
                <RefreshOutline />
              </n-icon>
              <b>注入运行中的Cursor</b>
            </div>
            <div
              class="option-desc"
              :style="{ color: themeVars.textColor3 }"
            >
              检测当前正在运行的Cursor实例并尝试直接注入
            </div>
          </div>

          <div
            class="option-item"
            :style="cardStyle"
          >
            <div
              class="option-title"
              :style="{ color: themeVars.textColor1 }"
            >
              <n-icon
                size="20"
                class="option-icon"
                :color="themeVars.primaryColor"
              >
                <InformationCircleOutline />
              </n-icon>
              <b>查看帮助文档</b>
            </div>
            <div
              class="option-desc"
              :style="{ color: themeVars.textColor3 }"
            >
              如果你不确定如何操作，可以查看详细的帮助文档
            </div>
          </div>
        </n-space>
      </div>

      <div
        v-if="cursorStore.fileSelectError"
        :class="['error-container', isDarkMode ? 'dark' : 'light']"
      >
        <n-icon
          size="16"
          style="margin-right: 6px"
        >
          <Warning />
        </n-icon>
        {{ cursorStore.fileSelectError }}
      </div>

      <div style="margin-top: 20px; display: flex; justify-content: flex-end; gap: 12px">
        <n-button
          type="primary"
          @click="handleOpenDocs"
        >
          打开文档
        </n-button>
        <n-button
          type="primary"
          :loading="cursorStore.operationLoading"
          @click="handleInjectRunningCursor"
        >
          注入正在运行的Cursor
        </n-button>
        <n-button
          type="primary"
          :loading="cursorStore.fileSelectLoading"
          @click="handleSelectPath"
        >
          选择文件
        </n-button>
      </div>
    </n-space>
  </n-modal>
</template>

<style scoped>
  .option-item {
    padding: 12px;
    border-radius: 6px;
    transition: background-color 0.2s;
    border: 1px solid rgba(0, 0, 0, 0.1);
  }

  .option-item:hover {
    filter: brightness(0.95);
  }

  .option-title {
    display: flex;
    align-items: center;
    font-size: 14px;
    margin-bottom: 6px;
  }

  .option-icon {
    margin-right: 8px;
  }

  .option-desc {
    margin-left: 28px;
    font-size: 13px;
  }

  .error-container {
    margin-top: 16px;
    padding: 10px;
    border-radius: 4px;
    display: flex;
    align-items: center;
  }

  .error-container.light {
    color: #d03050;
    background-color: rgba(208, 48, 80, 0.1);
  }

  .error-container.dark {
    color: #e88080;
    background-color: rgba(232, 128, 128, 0.1);
  }
</style>
