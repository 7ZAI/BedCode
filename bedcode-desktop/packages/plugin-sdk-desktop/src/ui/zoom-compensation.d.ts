/**
 * fixed 定位覆盖层在根元素 CSS zoom 下的坐标换算（详见 zoom-compensation.ts）
 */

/** 获取当前文档下 fixed 赋值坐标的换算因子（赋值 px = 设计 px / F） */
export declare function getFixedZoomCompensation(): number

/** 重置探针缓存（测试隔离用） */
export declare function resetFixedZoomProbe(): void
