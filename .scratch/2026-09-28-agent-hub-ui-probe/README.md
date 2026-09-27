# Agent Hub UI 实测探针（dev-shell + WebKit2 离屏截图 / computed style）

**为什么有这份**：2026-09-28 用户三次反馈「关键词输入框和同排控件不一致」，前两轮都只改了
**声明**（token 对齐、字号、占位色、阴影），全绿——但截图一拉、computed style 一打，
真相是两个**声明层面看不出、渲染后才暴露**的 bug。教训：控件一致性类问题必须看**渲染结果**，
不能只对齐声明。

## 复现命令

### 1. 起 dev-shell（插件前端在浏览器里跑，带 mock 领域数据）

```bash
cd bedcode-desktop/wasm-apps/agent-hub
pnpm exec bedcode-plugin-desktop dev --port 5180     # → http://localhost:5180/
```

dev-shell 自带**宿主 token 副本**（`packages/plugin-sdk-desktop/dev-shell/src/styles/style.css`，
746 行，含 `--input-height` / `--border-input` / `--font-size-lg` 与 `fontSize` 覆写），
只是 `--ui-scale: 1`（真机 Linux 是 1.15 + html 115%），**等比缩放，不影响控件间对比**。

导航路径：侧栏「Agent Hub」→ 顶部 tab「会话日志」→ 「日志来源」折叠区（默认收起，
查询条件条不受它影响，但展开后布局更接近用户截图）。

### 2. 离屏截图 + computed style（本机无 chrome/chromium，用宿主同款 WebKitGTK）

```bash
python3 - <<'PY'
import os
os.environ['WEBKIT_DISABLE_COMPOSITING_MODE'] = '1'   # 离屏窗口不关合成不落帧
import gi; gi.require_version('Gtk','3.0'); gi.require_version('Gdk','3.0'); gi.require_version('WebKit2','4.1')
from gi.repository import Gtk, Gdk, WebKit2, GLib
win = Gtk.Window(); win.set_default_size(1400, 1000); win.move(-4000, -4000)   # 离屏，不弹用户屏
web = WebKit2.WebView(); web.get_settings().set_enable_developer_extras(True)
win.add(web); win.show_all(); web.load_uri('http://localhost:5180/')

def js(script, cb):        # 回调签名是 (view, task, *_)——少一个参数会静默不触发
    def h(view, task, *_):
        cb(view.evaluate_javascript_finish(task).to_string())
    web.evaluate_javascript(script, -1, None, None, None, h, None)

def probe(view, *_):
    js("""JSON.stringify((()=>{const d=el=>{const c=getComputedStyle(el),r=el.getBoundingClientRect();
      return {h:r.height,w:r.width,y:r.y,height:c.height,minHeight:c.minHeight,fontSize:c.fontSize,
        lineHeight:c.lineHeight,padding:c.padding,border:c.borderTopWidth+' '+c.borderTopColor,
        radius:c.borderRadius,bg:c.backgroundColor,fg:c.color,font:c.fontFamily.split(',')[0]}};
      return {agent:d(document.querySelector('.ah-lg-filter .form-group button')),
              keyword:d(document.querySelector('.ah-lg-filter-q input')),
              date:d(document.querySelector('.ah-lg-filter-date .dp__input'))};})())""",
       lambda s: (print(s), shoot(view)))
    return False

def shoot(view):
    def snap(view, task, *_):
        png = view.snapshot_finish(task)          # 走 pixbuf 更稳：view.snapshot() 在离屏窗口会挂
        view.get_window()  # noqa
        Gdk.pixbuf_get_from_window(win.get_window(), 0, 0, 1400, 1000).savev('/tmp/row.png', 'png', [], [])
        Gtk.main_quit(); return False
    GLib.timeout_add(500, lambda: (win.get_window() and None, False)[1])   # 等一拍让布局稳定
    GLib.timeout_add(900, lambda: (Gdk.pixbuf_get_from_window(win.get_window(), 0, 0, 1400, 1000)
                                   .savev('/tmp/row.png', 'png', [], []), Gtk.main_quit(), False)[2])
    return False

web.connect('load-changed', lambda v, e: probe(v) if e == WebKit2.LoadEvent.FINISHED else None)
# 先点开面板与 tab（等插件 activate 完成，约 2-3s）
GLib.timeout_add(4000, lambda: (js("(()=>{const li=[...document.querySelectorAll('li')].find(e=>e.innerText.trim()==='Agent Hub'); li&&li.querySelector('button').click(); const t=[...document.querySelectorAll('.ah-tab')].find(b=>/日志/.test(b.innerText)); t&&t.click(); return 1})()", lambda _r: None), False)[1])
GLib.timeout_add(7000, lambda: (probe(web), False)[1])
GLib.timeout_add(25000, lambda: (Gtk.main_quit(), False)[1])
Gtk.main()
PY
```

要点（都是踩过的坑）：
- `win.move(-4000,-4000)` 离屏：窗口仍会 map，**不会**在用户屏幕上弹窗。
- 截图用 `Gdk.pixbuf_get_from_window`；`WebKit2.WebView.snapshot()` 在离屏窗口会**挂住**。
- 必须 `WEBKIT_DISABLE_COMPOSITING_MODE=1`（`Settings` 无对应 setter，只能走环境变量）。
- `evaluate_javascript` 的回调签名是 `(view, task, *user_data)`，写错会静默不触发。
- 裁剪区域要在 grab 之前重新量（布局会因展开折叠区而位移），否则截到上一帧的偏移。

## 本次抓到的两个 bug（都是「声明绿、渲染错」）

| # | 现象 | 根因 | 判据 |
| --- | --- | --- | --- |
| 1 | 关键词框 20.8px，同排 Select / 日期框 36px | `.ah-input` 要在横向 flex 行里占满宽度 → `flex: 1`；而查询条件条的字段容器是**列向** flex，flex-basis 作用于高度、容器高度 auto → 高度塌成内容高 | computed `height` 20.8px vs 36px |
| 2 | 日期框边框比同排控件浅一档 | `--ah-ctl-border: 1px solid var(--border-input)` 被当作**长写** `border-color` 用 → 整条声明失效 → 回退 vendor 的 `--dp-border-color`（= `--border`） | computed `border-top-color` rgb(236,232,220) vs rgb(60,55,43) |

附带发现：vendor 把自带字体栈（Linux 落到 `-apple-system`）设在 `.dp__main` 上，
`.dp__input { font-family: inherit }` 只是继承到这个栈——必须连外层一起覆盖；
自定义属性写 `--dp-font-family: inherit` 等于没改（取的是**父级的同名 token**）。

## 落锁

`src/__tests__/styleGuards.test.ts` S12 新增四条声明级护栏（都是针对上面两类 bug 的形态）：
height 必须与 min-height 成对 · 颜色长写不得含宽度/线型关键字（直接写或经 token）·
vendor 用 `--dp-font-family` 的每个选择器都要被 `font-family: inherit` 覆盖 ·
`.dp__input` 行高跟宿主继承值。变异自检：删 `min-height`、把简写塞进 `border-color`，
两条用例分别变红。
