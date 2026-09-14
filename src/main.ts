/**
 * 前端入口。
 *
 * 只做四件事：装 UI 库、装中文语言包、挂全局错误处理器、挂载。
 * 任何业务逻辑都不该出现在这里。
 */

import ElementPlus from 'element-plus'
import zhCn from 'element-plus/es/locale/lang/zh-cn'
import { createApp } from 'vue'

import App from './App.vue'
import { createLogger } from './lib/logger'

// Element Plus 的样式单独引一次（组件按需引的话样式也要按需引，
// 桌面应用不追求首屏体积，全量引更省心）。
import 'element-plus/dist/index.css'
import './styles.css'

const log = createLogger('bootstrap')

const app = createApp(App)
app.use(ElementPlus, { locale: zhCn })

/**
 * 兜底：未被 try/catch 捕获的组件错误。
 *
 * 没有它，Vue 会把错误打到 console 然后**静默停止渲染** ——
 * 用户看到白屏，而你连一条带 scope 的日志都没有。
 */
app.config.errorHandler = (err, _instance, info) => {
  log.error('未捕获的组件错误', { info, error: String(err) })
}

app.mount('#app')
log.info('应用已挂载')
