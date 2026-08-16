// SDK 独立构建时的 PostCSS 配置：避免 postcss-load-config 向上命中主机根
// 的 postcss.config.js（CI 上主机依赖未安装，tailwindcss 解析失败）
export default {
  plugins: {
    tailwindcss: {},
    autoprefixer: {},
  },
}