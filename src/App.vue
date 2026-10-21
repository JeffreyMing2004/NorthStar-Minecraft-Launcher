<script setup lang="ts">
import { computed, ref } from 'vue'
import { isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { version } from '../package.json'

const pages = ['开始', '账号', '下载', '设置', '更多'] as const
const page = ref<(typeof pages)[number]>('开始')
const categories = ['原版', '加载器', '整合包', 'Mod'] as const
const category = ref<(typeof categories)[number]>('原版')
const desktop = isTauri()
const message = ref('')
const descriptions = { 原版: '选择 Minecraft 版本，开启新的冒险。', 加载器: '为你的实例安装 Forge、Fabric 或 NeoForge。', 整合包: '发现组合好的模组与全新玩法。', Mod: '为现有实例添加更多可能。' }
const downloadDescription = computed(() => descriptions[category.value])
const memory = ref(4096)
const javaPath = ref('')
const saveStatus = ref('')
try {
  const saved: unknown = JSON.parse(localStorage.getItem('nscl.preferences') ?? 'null')
  if (saved && typeof saved === 'object' && 'memory' in saved && 'javaPath' in saved) {
    if (typeof saved.memory === 'number' && Number.isInteger(saved.memory) && saved.memory >= 1024 && saved.memory <= 32768) memory.value = saved.memory
    if (typeof saved.javaPath === 'string') javaPath.value = saved.javaPath
  }
} catch { saveStatus.value = '无法读取本地设置，已使用默认值。' }
function saveSettings() {
  if (!Number.isInteger(memory.value) || memory.value < 1024 || memory.value > 32768) {
    saveStatus.value = '内存必须为 1024–32768 MiB 之间的整数。'
    return
  }
  try {
    localStorage.setItem('nscl.preferences', JSON.stringify({ memory: memory.value, javaPath: javaPath.value.trim() }))
    saveStatus.value = '设置已保存在本机；游戏启动功能接入后生效。'
  } catch { saveStatus.value = '保存失败，请检查本地存储是否可用。' }
}
async function windowAction(action: 'minimize' | 'toggleMaximize' | 'close') {
  if (!desktop) return
  try { await getCurrentWindow()[action]() } catch { message.value = '窗口操作失败，请重试。' }
}
</script>

<template>
  <div class="shell">
    <header class="topbar" data-tauri-drag-region>
      <div class="brand" title="NorthStar Minecraft Launcher" data-tauri-drag-region>
        <span class="star" aria-hidden="true">✦</span><strong class="full-name">NorthStar Minecraft Launcher</strong><strong class="short-name">NSCL</strong><span class="version">v{{ version }}</span>
      </div>
      <nav aria-label="主导航">
        <button v-for="item in pages" :key="item" :class="{ active: page === item }" :aria-current="page === item ? 'page' : undefined" @click="page = item">{{ item }}</button>
      </nav>
      <div class="window-controls" v-if="desktop">
        <button aria-label="最小化" @click="windowAction('minimize')">−</button>
        <button aria-label="最大化或还原" @click="windowAction('toggleMaximize')">□</button>
        <button class="close" aria-label="关闭" @click="windowAction('close')">×</button>
      </div>
      <span class="preview-label" v-else>浏览器预览</span>
    </header>

    <main>
      <div class="page-heading"><div><p class="eyebrow">NORTHSTAR / {{ page === '开始' ? 'HOME' : page === '账号' ? 'ACCOUNT' : page === '下载' ? 'DOWNLOADS' : page === '设置' ? 'SETTINGS' : 'MORE' }}</p><h1>{{ page === '开始' ? '下一段冒险，从这里开始。' : page }}</h1></div><span class="badge">开发预览</span></div>

      <div v-if="page === '开始'" class="home-grid">
        <section class="card launch-card">
          <div class="landscape" aria-hidden="true"><span class="north-star">✦</span><div class="mountain back"></div><div class="mountain front"></div><span class="landscape-label">FIND YOUR NORTH</span></div>
          <div class="card-body"><p class="eyebrow">你的游戏空间</p><h2>准备好探索新世界了吗？</h2><p>安装第一个 Minecraft 实例，让冒险拥有自己的起点。</p><div class="instance-empty"><span class="square-icon">◇</span><div><strong>暂无游戏实例</strong><p>原版 · Forge · Fabric · NeoForge</p></div></div><button class="primary" @click="page = '下载'">＋ 安装 Minecraft <span>→</span></button><p class="hint">安装与启动服务正在开发中</p></div>
        </section>
        <aside class="card account-card"><div class="section-top"><h2>我的角色</h2><span class="badge muted">未登录</span></div><div class="skin-placeholder"><span aria-hidden="true">◇</span><strong>你的角色，即将登场</strong><p>3D 皮肤预览尚未接入</p></div><h3>添加你的 Minecraft 账号</h3><p>在这里管理身份、切换账号和修改皮肤。</p><button class="secondary" @click="page = '账号'">管理账号 →</button></aside>
      </div>

      <section v-else-if="page === '账号'" class="card spacious"><p class="eyebrow">ACCOUNT & SKIN</p><h2>让冒险拥有你的名字</h2><p>Microsoft 账号登录、皮肤上传与 3D 预览将在后续版本提供。</p><div class="empty-state"><span aria-hidden="true">◎</span><h3>还没有添加账号</h3><p>认证服务尚未接入，当前不会收集账号或密码。</p><button class="secondary" disabled>Microsoft 登录 · 开发中</button></div></section>

      <section v-else-if="page === '下载'" class="card spacious"><div class="tabs" aria-label="下载分类"><button v-for="item in categories" :key="item" :aria-pressed="category === item" :class="{ selected: category === item }" @click="category = item">{{ item }}</button></div><h2>{{ category === '原版' ? 'Minecraft Java Edition' : category }}</h2><p>{{ downloadDescription }}</p><div v-if="category === '加载器'" class="loader-list"><span v-for="loader in ['Forge', 'Fabric', 'NeoForge']" :key="loader">{{ loader }}</span></div><div class="empty-state"><span aria-hidden="true">↓</span><h3>{{ category }}下载服务尚未接入</h3><p>接入资源服务后，这里将显示可用版本和安装进度。</p><span class="badge muted">暂无下载任务</span></div></section>

      <section v-else-if="page === '设置'" class="card spacious settings"><p class="eyebrow">PREFERENCES</p><h2>为你的冒险做好准备</h2><p>设置保存在本机，当前不会执行 Java 或启动游戏。</p><form @submit.prevent="saveSettings"><label for="java">Java 可执行文件路径</label><input id="java" v-model="javaPath" placeholder="例如 C:\Program Files\Java\bin\javaw.exe" /><p class="hint">留空表示自动检测（检测功能待接入）。</p><label for="memory">游戏内存（MiB）</label><input id="memory" v-model.number="memory" type="number" min="1024" max="32768" step="1" required /><p class="hint">范围 1024–32768 MiB，请根据设备可用内存配置。</p><button class="primary" type="submit">保存设置</button><p class="status-message" role="status">{{ saveStatus }}</p></form></section>

      <section v-else class="card spacious about"><span class="star large">✦</span><h2>NorthStar Minecraft Launcher</h2><p>NSCL · v{{ version }} · 开发预览</p><p>一个以 Rust 为核心的 Minecraft Java Edition 启动器。</p><div class="about-details"><h3>本版本</h3><p>已提供导航、页面框架和本地设置保存。游戏安装与启动、账号认证、模组下载及皮肤渲染尚未实现。</p><h3>项目声明</h3><p>本项目与 Mojang Studios 或 Microsoft 无隶属或背书关系。</p></div></section>
    </main>
    <footer><span><i></i> {{ message || '界面预览 · 核心游戏服务尚未接入' }}</span><span>JAVA EDITION <span class="footer-star">✦</span> NSCL</span></footer>
  </div>
</template>
