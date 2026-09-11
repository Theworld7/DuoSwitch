import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [vue(), tailwindcss()],

  // Tauri 期望固定端口，端口被占用时直接失败而不是自动换端口
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Rust 侧由 tauri dev 自己监听，避免重复触发
      ignored: ['**/src-tauri/**'],
    },
  },

  build: {
    target: 'chrome110',
    emptyOutDir: true,
  },
})
