import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { resolve } from 'node:path';

// 前端放在 ui/，兩個入口：設定視窗 (index.html) 與開啟成功浮窗 (toast.html)
export default defineConfig({
  root: 'ui',
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      input: {
        index: resolve(import.meta.dirname, 'ui/index.html'),
        toast: resolve(import.meta.dirname, 'ui/toast.html'),
      },
    },
  },
});
