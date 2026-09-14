<script setup lang="ts">
/**
 * 黄金纵切的 UI 侧：表格 + 表单 + 分页。
 *
 * 三条纪律（AGENTS.md「UI 技术栈规范」）：
 * 1. 只用 Element Plus 提供的组件，**不自造基础组件**（按钮/输入框/弹窗都不手写）。
 * 2. 功能图标只用官方图标库 `@element-plus/icons-vue`，**禁止 emoji**。
 * 3. 色值只用组件库的语义变量，不自定义颜色 —— 所以本文件里一条 CSS 颜色都没有。
 */

import { Delete, Edit, Plus, Refresh } from '@element-plus/icons-vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { computed, onMounted, reactive, ref } from 'vue'

import { demoItemApi, toUserMessage } from '../lib/api'
import { createLogger } from '../lib/logger'
import {
  DEMO_ITEM_LIMITS,
  type DemoItem,
  type DemoItemPatch,
  type NewDemoItem,
} from '../types/demo_item'

// 带 scope 的 logger：所有本模块日志都能被 `grep demo-item` 捞出来。
const log = createLogger('demo-item')

const items = ref<DemoItem[]>([])
const total = ref(0)
const page = ref(1)
// ⚠️ 必须显式写 <number>：DEMO_ITEM_LIMITS 是 `as const`，所以 pageDefault 的
// 类型是字面量 50，`ref(DEMO_ITEM_LIMITS.pageDefault)` 会被推断成 Ref<50>，
// 于是 handlePageSizeChange 里赋一个 number 就报 TS2322。
// （这条是 vue-tsc 抓出来的，eslint 与 vite build 都看不见。）
const pageSize = ref<number>(DEMO_ITEM_LIMITS.pageDefault)
const loading = ref(false)
const submitting = ref(false)

const dialogVisible = ref(false)
/** null = 新建；否则是正在编辑的 id。用一个状态表达两种模式，
 *  比维护 `isEdit: boolean` + `editingId: number` 两个字段更难写错。 */
const editingId = ref<number | null>(null)

const form = reactive<Required<NewDemoItem>>({ name: '', quantity: 0, note: '' })

const dialogTitle = computed(() =>
  editingId.value === null ? '新建条目' : `编辑条目 #${editingId.value}`,
)

/**
 * 把数据库给的时间戳显示成本地时间。
 *
 * ⚠️ 这里有个真实的坑：SQLite 的 `datetime('now')` 返回
 * `"2026-09-10 12:00:00"` —— **UTC 时间，但不带时区标记**。
 * 直接 `new Date("2026-09-10 12:00:00")` 在不同 JS 引擎上结果不同
 * （有的按本地时区解析），于是同一个值在 Windows 和 macOS 上差 8 小时。
 * 所以显式补上 `T` 与 `Z`，把语义钉死成 UTC，再让浏览器换算成本地时间显示。
 */
function formatTime(raw: string): string {
  if (!raw) return '—'
  const iso = raw.includes('T') ? raw : `${raw.replace(' ', 'T')}Z`
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return raw // 解析不了就原样显示，绝不显示 Invalid Date
  const pad = (n: number): string => String(n).padStart(2, '0')
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
    ` ${pad(date.getHours())}:${pad(date.getMinutes())}`
  )
}

async function load(): Promise<void> {
  loading.value = true
  try {
    const result = await demoItemApi.list(pageSize.value, (page.value - 1) * pageSize.value)
    items.value = result.items
    total.value = result.total
    log.debug('列表已加载', { total: result.total, page: page.value })
  } catch (error) {
    // 打日志 + 给用户一句话：两者都要，且顺序不能反
    // （日志带的是原始错误，给用户的是人话）。
    log.error('列表加载失败', { error: String(error), code: readCode(error) })
    ElMessage.error(toUserMessage(error))
  } finally {
    loading.value = false
  }
}

/** 从抛出物里取错误码，取不到就返回 undefined —— 仅用于日志字段。 */
function readCode(error: unknown): string | undefined {
  if (typeof error === 'object' && error !== null && 'code' in error) {
    const code = (error as { code: unknown }).code
    return typeof code === 'string' ? code : undefined
  }
  return undefined
}

function openCreate(): void {
  editingId.value = null
  form.name = ''
  form.quantity = 0
  form.note = ''
  dialogVisible.value = true
}

function openEdit(row: DemoItem): void {
  editingId.value = row.id
  form.name = row.name
  form.quantity = row.quantity
  form.note = row.note
  dialogVisible.value = true
}

async function handleSubmit(): Promise<void> {
  submitting.value = true
  try {
    if (editingId.value === null) {
      await demoItemApi.create({ name: form.name, quantity: form.quantity, note: form.note })
      ElMessage.success('已新建')
    } else {
      const patch: DemoItemPatch = {
        name: form.name,
        quantity: form.quantity,
        note: form.note,
      }
      await demoItemApi.update(editingId.value, patch)
      ElMessage.success('已保存')
    }
    dialogVisible.value = false
    await load()
  } catch (error) {
    log.error('保存失败', { error: String(error), code: readCode(error) })
    ElMessage.error(toUserMessage(error))
  } finally {
    submitting.value = false
  }
}

async function handleDelete(row: DemoItem): Promise<void> {
  try {
    // 危险动作用 confirm 拦一道；文案里带上名字，避免"删错了才发现"。
    await ElMessageBox.confirm(`确定删除「${row.name}」吗？此操作不可撤销。`, '删除确认', {
      type: 'warning',
      confirmButtonText: '删除',
      cancelButtonText: '取消',
    })
  } catch {
    // 用户点了取消 —— ElMessageBox 用 reject 表达"取消"，
    // 这不是错误，什么都不做就对了（绝不能在这里弹 error）。
    return
  }

  try {
    await demoItemApi.remove(row.id)
    ElMessage.success('已删除')

    // 删掉当前页最后一条时，页码回退一页 —— 否则会停在一个空页上，
    // 用户以为"数据全没了"。
    if (items.value.length === 1 && page.value > 1) {
      page.value -= 1
    }
    await load()
  } catch (error) {
    log.error('删除失败', { error: String(error), code: readCode(error), id: row.id })
    ElMessage.error(toUserMessage(error))
  }
}

function handlePageChange(next: number): void {
  page.value = next
  void load()
}

function handlePageSizeChange(next: number): void {
  pageSize.value = next
  // 换页大小后原来的页码没有意义，回到第一页。
  page.value = 1
  void load()
}

onMounted(() => {
  log.info('面板已挂载')
  void load()
})
</script>

<template>
  <el-card shadow="never">
    <template #header>
      <div class="panel-header">
        <span class="panel-title">条目管理</span>
        <el-space>
          <el-button type="primary" :icon="Plus" @click="openCreate">新建</el-button>
          <el-button :icon="Refresh" :loading="loading" @click="load">刷新</el-button>
        </el-space>
      </div>
    </template>

    <el-table v-loading="loading" :data="items" border stripe empty-text="还没有任何条目">
      <el-table-column prop="id" label="ID" width="80" />
      <el-table-column prop="name" label="名称" min-width="160" show-overflow-tooltip />
      <el-table-column prop="quantity" label="数量" width="100" align="right" />
      <el-table-column prop="note" label="备注" min-width="160" show-overflow-tooltip />
      <el-table-column label="更新时间" width="200">
        <template #default="scope">
          <!-- 用插值而不是 v-text：这里要拼接格式化结果，插值更直观。 -->
          <span>{{ formatTime(scope.row.updatedAt) }}</span>
        </template>
      </el-table-column>
      <el-table-column label="操作" width="160" fixed="right">
        <template #default="scope">
          <el-button link type="primary" :icon="Edit" @click="openEdit(scope.row)">编辑</el-button>
          <el-button link type="danger" :icon="Delete" @click="handleDelete(scope.row)">
            删除
          </el-button>
        </template>
      </el-table-column>
    </el-table>

    <el-pagination
      class="panel-pager"
      layout="total, sizes, prev, pager, next"
      :total="total"
      :current-page="page"
      :page-size="pageSize"
      :page-sizes="[10, 20, 50, 100]"
      @current-change="handlePageChange"
      @size-change="handlePageSizeChange"
    />

    <el-dialog v-model="dialogVisible" :title="dialogTitle" width="520px">
      <el-form label-width="72px" @submit.prevent>
        <el-form-item label="名称">
          <el-input
            v-model="form.name"
            :maxlength="DEMO_ITEM_LIMITS.nameMax"
            show-word-limit
            placeholder="例如：写周报"
          />
        </el-form-item>
        <el-form-item label="数量">
          <el-input-number
            v-model="form.quantity"
            :min="0"
            :max="DEMO_ITEM_LIMITS.quantityMax"
            controls-position="right"
          />
        </el-form-item>
        <el-form-item label="备注">
          <el-input
            v-model="form.note"
            type="textarea"
            :rows="3"
            :maxlength="DEMO_ITEM_LIMITS.noteMax"
            show-word-limit
          />
        </el-form-item>
      </el-form>

      <template #footer>
        <el-button @click="dialogVisible = false">取消</el-button>
        <el-button type="primary" :loading="submitting" @click="handleSubmit">保存</el-button>
      </template>
    </el-dialog>
  </el-card>
</template>

<style scoped>
/* 只写布局，不写颜色 —— 配色由 Element Plus 的语义变量决定。 */
.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.panel-title {
  font-weight: 600;
}

.panel-pager {
  margin-top: 16px;
  justify-content: flex-end;
}
</style>
