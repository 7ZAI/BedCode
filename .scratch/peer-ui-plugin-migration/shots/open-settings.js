// 桌面 dev-shell 驱动：打开文件传输面板 → 打开设置覆盖层，带重试与错误捕获
(async function () {
  const app = () => document.querySelector('#app')?.__vue_app__
  const errs = []
  const hook = () => {
    const a = app()
    if (!a) return false
    a.config.errorHandler = (e, i, info) => errs.push('E:' + String(e?.stack || e).slice(0, 300) + ' @' + info)
    a.config.warnHandler = (m) => errs.push('W:' + String(m).slice(0, 200))
    return true
  }
  // 1. 打开插件面板（重试至成功）
  for (let i = 0; i < 12; i++) {
    const btn = [...document.querySelectorAll('aside button')].find((b) => b.textContent.includes('文件传输'))
    if (btn) {
      btn.click()
      await new Promise((r) => setTimeout(r, 700))
      if (document.body.innerText.includes('发送到手机')) break
    }
    await new Promise((r) => setTimeout(r, 500))
  }
  if (!document.body.innerText.includes('发送到手机')) return JSON.stringify({ fail: 'panel-not-open' })
  hook()
  // 2. 点设置（重试至覆盖层出现）
  for (let i = 0; i < 3; i++) {
    const s = [...document.querySelectorAll('button')].find((x) => x.offsetParent && x.textContent.trim() === '设置')
    if (s) {
      s.click()
      await new Promise((r) => setTimeout(r, 700))
      if (document.querySelector('[class*=ft-settings-backdrop]')) return JSON.stringify({ ok: true, errs })
    }
    await new Promise((r) => setTimeout(r, 400))
  }
  return JSON.stringify({ fail: 'settings-not-open', errs })
})()
