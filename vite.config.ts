import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
// Windows 下 cargo 编译时会锁定 src-tauri/target 内的文件，Vite 监视到会以 EBUSY 崩溃
export default defineConfig({ plugins: [vue()], server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } }, clearScreen: false })
