# -*- encoding: utf-8 -*-
"""RapidOCR 参考流水线（v2.x 公式逐行移植，用于验证 Rust 移植前的公式正确性）
模型: ch_PP-OCRv4_det / ch_PP-OCRv4_rec / ch_ppocr_mobile_v2.0_cls
"""
import json, math
import cv2
import numpy as np
import onnxruntime as ort
from PIL import Image, ImageDraw, ImageFont

HERE = r"D:\tauriProject\BedCode\.scratch\ocr-plugin\ref"
MODELS = "D:/tauriProject/BedCode/bedcode-mobile/src-tauri/resources/ocr_models"
DICT_PATH = HERE + "/ppocr_keys_v1.txt"

# ==================== 工具 ====================

def read_dict(path):
    with open(path, "rb") as f:
        chars = [line.decode("utf-8").strip("\n").strip("\r\n") for line in f.readlines()]
    chars.insert(len(chars), " ")   # space at end
    chars.insert(0, "blank")        # blank at 0
    return chars

CHARACTER = read_dict(DICT_PATH)
assert len(CHARACTER) == 6625, len(CHARACTER)

# ==================== det ====================

class DetPreProcess:
    def __init__(self, limit_side_len=736, limit_type="min"):
        self.mean = np.array([0.485, 0.456, 0.406], dtype=np.float32)
        self.std = np.array([0.229, 0.224, 0.225], dtype=np.float32)
        self.scale = 1 / 255.0
        self.limit_side_len = limit_side_len
        self.limit_type = limit_type

    def resize(self, img):
        h, w = img.shape[:2]
        if self.limit_type == "max":
            ratio = float(self.limit_side_len) / max(h, w) if max(h, w) > self.limit_side_len else 1.0
        else:
            ratio = float(self.limit_side_len) / min(h, w) if min(h, w) < self.limit_side_len else 1.0
        resize_h = int(round(int(h * ratio) / 32) * 32)
        resize_w = int(round(int(w * ratio) / 32) * 32)
        return cv2.resize(img, (resize_w, resize_h))

    def __call__(self, img):
        img = self.resize(img)
        img = (img.astype("float32") * self.scale - self.mean) / self.std
        img = img.transpose((2, 0, 1))
        return img[np.newaxis, ...].astype(np.float32)

class DBPostProcess:
    def __init__(self, thresh=0.3, box_thresh=0.5, max_candidates=1000, unclip_ratio=1.6,
                 score_mode="fast", use_dilation=True):
        self.thresh = thresh
        self.box_thresh = box_thresh
        self.max_candidates = max_candidates
        self.unclip_ratio = unclip_ratio
        self.min_size = 3
        self.score_mode = score_mode
        self.dilation_kernel = np.array([[1, 1], [1, 1]]) if use_dilation else None

    def __call__(self, pred, ori_shape):
        src_h, src_w = ori_shape
        pred = pred[:, 0, :, :]
        segmentation = pred > self.thresh
        mask = segmentation[0].astype(np.uint8)
        if self.dilation_kernel is not None:
            mask = cv2.dilate(mask, self.dilation_kernel)
        boxes, scores = self.boxes_from_bitmap(pred[0], mask, src_w, src_h)
        return boxes, scores

    def boxes_from_bitmap(self, pred, bitmap, dest_width, dest_height):
        import pyclipper
        from shapely.geometry import Polygon
        height, width = bitmap.shape
        outs = cv2.findContours((bitmap * 255).astype(np.uint8), cv2.RETR_LIST, cv2.CHAIN_APPROX_SIMPLE)
        contours = outs[0] if len(outs) == 2 else outs[1]
        num_contours = min(len(contours), self.max_candidates)
        boxes, scores = [], []
        for index in range(num_contours):
            contour = contours[index]
            points, sside = self.get_mini_boxes(contour)
            if sside < self.min_size:
                continue
            score = self.box_score_fast(pred, points.reshape(-1, 2))
            if self.box_thresh > score:
                continue
            box = self.unclip(points, Polygon, pyclipper)
            box, sside = self.get_mini_boxes(box)
            if sside < self.min_size + 2:
                continue
            box[:, 0] = np.clip(np.round(box[:, 0] / width * dest_width), 0, dest_width)
            box[:, 1] = np.clip(np.round(box[:, 1] / height * dest_height), 0, dest_height)
            boxes.append(box.astype(np.int32))
            scores.append(score)
        return np.array(boxes, dtype=np.int32) if boxes else np.zeros((0, 4, 2), np.int32), scores

    def get_mini_boxes(self, contour):
        bounding_box = cv2.minAreaRect(contour)
        points = sorted(list(cv2.boxPoints(bounding_box)), key=lambda x: x[0])
        index_1, index_2, index_3, index_4 = 0, 1, 2, 3
        if points[1][1] > points[0][1]:
            index_1, index_4 = 0, 1
        else:
            index_1, index_4 = 1, 0
        if points[3][1] > points[2][1]:
            index_2, index_3 = 2, 3
        else:
            index_2, index_3 = 3, 2
        box = np.array([points[index_1], points[index_2], points[index_3], points[index_4]])
        return box, min(bounding_box[1])

    @staticmethod
    def box_score_fast(bitmap, _box):
        h, w = bitmap.shape[:2]
        box = _box.copy()
        xmin = np.clip(np.floor(box[:, 0].min()).astype(np.int32), 0, w - 1)
        xmax = np.clip(np.ceil(box[:, 0].max()).astype(np.int32), 0, w - 1)
        ymin = np.clip(np.floor(box[:, 1].min()).astype(np.int32), 0, h - 1)
        ymax = np.clip(np.ceil(box[:, 1].max()).astype(np.int32), 0, h - 1)
        mask = np.zeros((ymax - ymin + 1, xmax - xmin + 1), dtype=np.uint8)
        box[:, 0] = box[:, 0] - xmin
        box[:, 1] = box[:, 1] - ymin
        cv2.fillPoly(mask, box.reshape(1, -1, 2).astype(np.int32), 1)
        return cv2.mean(bitmap[ymin:ymax + 1, xmin:xmax + 1], mask)[0]

    def unclip(self, box, Polygon, pyclipper):
        poly = Polygon(box)
        distance = poly.area * self.unclip_ratio / poly.length
        offset = pyclipper.PyclipperOffset()
        offset.AddPath(box.tolist(), pyclipper.JT_ROUND, pyclipper.ET_CLOSEDPOLYGON)
        expanded = np.array(offset.Execute(distance)).reshape((-1, 1, 2))
        return expanded

    def filter_det_res(self, dt_boxes, scores, img_height, img_width):
        dt_boxes_new, new_scores = [], []
        for box, score in zip(dt_boxes, scores):
            xSorted = box[np.argsort(box[:, 0]), :]
            leftMost = xSorted[:2, :][np.argsort(xSorted[:2, :][:, 1]), :]
            (tl, bl) = leftMost
            rightMost = xSorted[2:, :][np.argsort(xSorted[2:, :][:, 1]), :]
            (tr, br) = rightMost
            box = np.array([tl, tr, br, bl], dtype="float32")
            for pno in range(4):
                box[pno, 0] = int(min(max(box[pno, 0], 0), img_width - 1))
                box[pno, 1] = int(min(max(box[pno, 1], 0), img_height - 1))
            rect_width = int(np.linalg.norm(box[0] - box[1]))
            rect_height = int(np.linalg.norm(box[0] - box[3]))
            if rect_width <= 3 or rect_height <= 3:
                continue
            dt_boxes_new.append(box)
            new_scores.append(score)
        return np.array(dt_boxes_new, dtype=np.float32), new_scores

def sorted_boxes(dt_boxes):
    if len(dt_boxes) == 0:
        return dt_boxes
    y_coords = dt_boxes[:, 0, 1]
    y_order = np.argsort(y_coords, kind="stable")
    boxes_y_sorted = dt_boxes[y_order]
    y_sorted = y_coords[y_order]
    dy = np.diff(y_sorted)
    line_increments = (dy >= 10).astype(np.int32)
    line_ids = np.concatenate([[0], np.cumsum(line_increments)])
    x_coords = boxes_y_sorted[:, 0, 0]
    final_order = np.lexsort((x_coords, line_ids))
    return boxes_y_sorted[final_order]

# ==================== crop / cls / rec ====================

def get_rotate_crop_image(img, points):
    img_crop_width = int(max(np.linalg.norm(points[0] - points[1]), np.linalg.norm(points[2] - points[3])))
    img_crop_height = int(max(np.linalg.norm(points[0] - points[3]), np.linalg.norm(points[1] - points[2])))
    if img_crop_width < 1 or img_crop_height < 1:
        return None
    pts_std = np.array([[0, 0], [img_crop_width, 0], [img_crop_width, img_crop_height], [0, img_crop_height]], dtype=np.float32)
    M = cv2.getPerspectiveTransform(points.astype(np.float32), pts_std)
    dst_img = cv2.warpPerspective(img, M, (img_crop_width, img_crop_height),
                                  borderMode=cv2.BORDER_REPLICATE, flags=cv2.INTER_CUBIC)
    dst_img_height, dst_img_width = dst_img.shape[0:2]
    if dst_img_height * 1.0 / dst_img_width >= 1.5:
        dst_img = np.rot90(dst_img)
    return dst_img

def cls_resize_norm_img(img, img_h=48, img_w=192):
    h, w = img.shape[:2]
    ratio = w / float(h)
    resized_w = img_w if math.ceil(img_h * ratio) > img_w else int(math.ceil(img_h * ratio))
    resized = cv2.resize(img, (resized_w, img_h), interpolation=cv2.INTER_LINEAR)
    resized = resized.astype("float32").transpose((2, 0, 1)) / 255
    resized -= 0.5
    resized /= 0.5
    pad = np.zeros((3, img_h, img_w), dtype=np.float32)
    pad[:, :, :resized_w] = resized
    return pad[np.newaxis, ...]

def rec_resize_norm_img(img, max_wh_ratio, img_h=48):
    img_w = int(img_h * max_wh_ratio)
    h, w = img.shape[:2]
    ratio = w / float(h)
    resized_w = img_w if math.ceil(img_h * ratio) > img_w else int(math.ceil(img_h * ratio))
    resized = cv2.resize(img, (resized_w, img_h), interpolation=cv2.INTER_LINEAR)
    resized = resized.astype("float32").transpose((2, 0, 1)) / 255
    resized -= 0.5
    resized /= 0.5
    pad = np.zeros((3, img_h, img_w), dtype=np.float32)
    pad[:, :, :resized_w] = resized
    return pad[np.newaxis, ...]

def ctc_decode(preds):
    preds_idx = preds.argmax(axis=2)
    preds_prob = preds.max(axis=2)
    token_indices = preds_idx[0]
    selection = np.ones(len(token_indices), dtype=bool)
    selection[1:] = token_indices[1:] != token_indices[:-1]
    selection &= token_indices != 0
    conf_list = np.array(preds_prob[0][selection]).tolist()
    conf_list = [round(c, 5) for c in conf_list]
    if len(conf_list) == 0:
        conf_list = [0]
    text = "".join([CHARACTER[i] for i in token_indices[selection]])
    return text, np.mean(conf_list).round(5).tolist()

# ==================== 主流程 ====================

def main():
    det_sess = ort.InferenceSession(f"{MODELS}/ch_PP-OCRv4_det_infer.onnx", providers=["CPUExecutionProvider"])
    cls_sess = ort.InferenceSession(f"{MODELS}/ch_ppocr_mobile_v2.0_cls_infer.onnx", providers=["CPUExecutionProvider"])
    rec_sess = ort.InferenceSession(f"{MODELS}/ch_PP-OCRv4_rec_infer.onnx", providers=["CPUExecutionProvider"])
    print("det input:", det_sess.get_inputs()[0].name, det_sess.get_inputs()[0].shape)
    print("det output:", det_sess.get_outputs()[0].name, det_sess.get_outputs()[0].shape)
    print("rec input:", rec_sess.get_inputs()[0].name, rec_sess.get_inputs()[0].shape)
    print("rec output:", rec_sess.get_outputs()[0].name, rec_sess.get_outputs()[0].shape)

    # 合成样张（白底 + 多行文字）
    W, H = 1200, 800
    img = Image.new("RGB", (W, H), "white")
    draw = ImageDraw.Draw(img)
    fonts = [ImageFont.truetype("C:/Windows/Fonts/msyh.ttc", 56),
             ImageFont.truetype("C:/Windows/Fonts/msyh.ttc", 40),
             ImageFont.truetype("C:/Windows/Fonts/arial.ttf", 48)]
    draw.text((80, 60), "你好世界", font=fonts[0], fill="black")
    draw.text((80, 200), "BedCode OCR 混合测试", font=fonts[1], fill="black")
    draw.text((80, 340), "Hello World 2026", font=fonts[2], fill="black")
    draw.text((80, 480), "你好，世界！", font=fonts[0], fill="black")
    rgb = np.array(img)
    bgr = cv2.cvtColor(rgb, cv2.COLOR_RGB2BGR)   # 模拟 cv2.imread 读到的 BGR

    # 模拟 Kotlin 降采样（长边 1600，这里 1200 不缩）
    work = bgr

    det_pre = DetPreProcess()
    pre_img = det_pre(work)
    print("det input shape:", pre_img.shape)
    pred = det_sess.run(None, {"x": pre_img})[0]
    print("det output shape:", pred.shape, "range:", float(pred.min()), float(pred.max()))
    boxes, scores = DBPostProcess()(pred, work.shape[:2])
    boxes, scores = DBPostProcess().filter_det_res(boxes, scores, work.shape[0], work.shape[1])
    boxes = sorted_boxes(boxes)
    print("det boxes:", len(boxes))
    for b, s in zip(boxes, scores):
        print("  box", b.astype(int).tolist(), "score", round(s, 4))

    crops = [get_rotate_crop_image(work, b) for b in boxes]
    crops = [c for c in crops if c is not None]

    # cls
    cls_in = np.concatenate([cls_resize_norm_img(c) for c in crops]).astype(np.float32)
    cls_out = cls_sess.run(None, {"x": cls_in})[0]
    cls_labels = cls_out.argmax(axis=1)
    cls_scores = cls_out.max(axis=1)
    rotated = []
    for c, lab, sc in zip(crops, cls_labels, cls_scores):
        if lab == 1 and float(sc) > 0.9:
            rotated.append(cv2.rotate(c, cv2.ROTATE_180))
        else:
            rotated.append(c)
        if lab == 1:
            print(f"  cls: 180 (score {sc:.4f})")

    # rec（单张 batch，max_wh_ratio 取最大）
    whs = [c.shape[1] / c.shape[0] for c in rotated]
    max_wh_ratio = max([320 / 48] + whs)
    rec_in = np.concatenate([rec_resize_norm_img(c, max_wh_ratio) for c in rotated]).astype(np.float32)
    rec_out = rec_sess.run(None, {"x": rec_in})[0]
    print("rec output shape:", rec_out.shape)

    results = []
    for i, b in enumerate(boxes):
        text, conf = ctc_decode(rec_out[i:i + 1])
        results.append({"text": text, "conf": conf, "box": b.astype(int).tolist()})
        print(f"  [{i}] {text!r} conf={conf}")

    # 空行/低置信度过滤（text_score=0.5）
    results = [r for r in results if r["conf"] >= 0.5]
    print("FINAL:", [(r["text"], r["conf"]) for r in results])

    # 存 golden（供 Rust 单测对比）
    golden = {
        "work_shape": [work.shape[1], work.shape[0]],
        "det_input_shape": list(pre_img.shape),
        "boxes": [b.tolist() for b in boxes],
        "scores": [round(s, 6) for s in scores],
        "results": [{"text": r["text"], "conf": r["conf"]} for r in results],
    }
    with open(HERE + "/golden.json", "w", encoding="utf-8") as f:
        json.dump(golden, f, ensure_ascii=False, indent=1)
    print("golden.json saved")

    # ---- 小合成 prob map 的 det 后处理 golden（给 Rust 几何单测）----
    rng = np.random.default_rng(42)
    pmap = np.zeros((64, 96), dtype=np.float32)
    pmap[10:24, 12:60] = 0.9
    pmap[40:52, 30:80] = 0.85
    pmap += rng.normal(0, 0.02, pmap.shape).astype(np.float32)
    pmap[pmap < 0] = 0
    pmap[pmap > 1] = 1
    pmap[60:64, 0:10] = 0.2
    db = DBPostProcess()
    boxes2, scores2 = db(pmap[None, None, ...], (64, 96))
    boxes2, scores2 = db.filter_det_res(boxes2, scores2, 64, 96)
    boxes2 = sorted_boxes(boxes2)
    print("synth boxes:", boxes2.tolist(), scores2)
    with open(HERE + "/golden_synth.json", "w") as f:
        json.dump({"boxes": boxes2.tolist(), "scores": [round(s, 6) for s in scores2]}, f)
    np.save(HERE + "/synth_pmap.npy", pmap)

    # ---- cv2.dilate 2x2 行为实证（供 Rust 移植）----
    m = np.zeros((5, 5), np.uint8); m[1, 1] = 1
    d = cv2.dilate(m, np.array([[1, 1], [1, 1]]))
    print("dilate 2x2 of pixel(1,1):\n", d)

if __name__ == "__main__":
    main()
