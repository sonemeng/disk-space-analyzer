import { createApp } from 'vue'
import App from './App.vue'

function showBootError(message: unknown) {
  const text = message instanceof Error
    ? `${message.message}\n${message.stack || ''}`
    : String(message)
  const root = document.getElementById('app')
  if (root) {
    root.innerHTML = `<div class="boot-error">${text.replace(/</g, '&lt;')}</div>`
  }
  console.error(message)
}

try {
  const app = createApp(App)
  app.config.errorHandler = (err) => {
    showBootError(err)
  }
  app.mount('#app')
} catch (error) {
  showBootError(error)
}
