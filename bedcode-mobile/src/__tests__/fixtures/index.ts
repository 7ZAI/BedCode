/**
 * Fixtures 工厂统一出口
 *
 * 对齐机制见 drift.ts：每个工厂产出时做运行时字段集合断言，
 * Rust 侧 DTO / 事件载荷漂移时引用方测试立刻失败。
 */

export * from './auth'
export * from './session'
export * from './sync'
