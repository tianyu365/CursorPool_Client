<script setup lang="ts">
  import { ref, onMounted, computed } from 'vue'
  import { NSelect, useMessage } from 'naive-ui'
  import { getUserData, setUserData, delUserData } from '@/api'
  import { useI18n } from '../locales'

  const message = useMessage()
  const { t } = useI18n()

  const props = defineProps({
    // 是否在紧凑布局中使用
    compact: {
      type: Boolean,
      default: false,
    },
    // 是否显示标签
    showLabel: {
      type: Boolean,
      default: true,
    },
  })

  // 关闭方式选项
  const closeTypeOptions = computed(() => [
    {
      label: t('closeType.ask'),
      value: 'ask',
    },
    {
      label: t('closeType.minimize'),
      value: 'minimize',
    },
    {
      label: t('closeType.exit'),
      value: 'exit',
    },
  ])

  // 当前选中的关闭方式
  const selectedCloseType = ref('ask')

  // 初始化设置
  onMounted(async () => {
    try {
      // 从数据库获取当前设置
      const savedCloseType = await getUserData('system.close.type')
      if (savedCloseType === 'minimize' || savedCloseType === 'exit') {
        selectedCloseType.value = savedCloseType
      }
    } catch (error) {
      console.error('获取关闭类型设置失败:', error)
    }
  })

  // 处理选择变化
  const handleChange = async (value: string) => {
    // 更新UI显示
    selectedCloseType.value = value

    try {
      // 如果选择"每次询问"，则删除设置
      if (value === 'ask') {
        await delUserData('system.close.type')
      } else {
        // 保存具体设置
        await setUserData('system.close.type', value)
      }
    } catch (error) {
      console.error('保存关闭类型设置失败:', error)
      message.error(t('settings.settingsFailed'))
    }
  }
</script>

<template>
  <div
    class="flex items-center"
    :class="{ 'gap-0': props.compact, 'gap-1': !props.compact }"
  >
    <!-- 标签 -->
    <div
      v-if="showLabel"
      class="text-sm whitespace-nowrap"
    >
      {{ t('settings.closeMethod') }}
    </div>

    <!-- 选择器 -->
    <n-select
      v-model:value="selectedCloseType"
      :options="closeTypeOptions"
      size="small"
      :style="{
        width: props.compact ? '100px' : '120px',
      }"
      @update:value="handleChange"
    />
  </div>
</template>

<style scoped>
  /* Remove custom styling to use default NaiveUI appearance */
</style>
