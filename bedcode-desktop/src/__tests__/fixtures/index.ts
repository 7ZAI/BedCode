/**
 * Fixtures 工厂统一出口
 *
 * 所有 mock invoke 返回数据的单一真源。各 fixture 与对应 Rust DTO 字段级对齐
 * （snake_case/camelCase 以各 DTO 的 serde 属性为准，文件头注明 Rust 源文件），
 * 工厂内 assertDtoFields 运行时断言键集合，drift.test.ts 对全部 DTO 做回归。
 * 存量测试应从本模块取数，禁止各测试文件重复手写 DTO 字面量。
 */

export * from './drift'
export * from './server'
export * from './pairing'
export * from './session'
export * from './plugin'
