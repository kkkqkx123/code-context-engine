当需要创建或重写数据流文档时，请按以下格式生成：

1. 文档聚焦**数据如何流动与内存如何管理**，而非代码结构地图（结构地图用 code-map.md 规范）
2. 文档按**阶段**拆分：每个阶段一个文件（如 00-overview 总览、01-capture 硬件采集、02-encode 编码、03-mux 封装），不同阶段分别给出文档说明。统一放在 docs/architecture/dataflow/ 下
3. 每个阶段文档**自包含**：在对应组件处内联其输入/输出数据结构与缓存区细节，不引入复杂的文档间引用（数据与数据流紧密相关，禁止"详见另一文档"式跳转；总览末尾仅允许一个简单索引链接列表）
4. 组件输入/输出一律用**函数签名**形式说明，签名与源码一致（含默认参数、参数类型），便于在原代码中对照；参数/回调的物理意义以注释标注
5. 数据结构（包括用于管理状态流转的数据结构）用**原始代码或伪代码**形式给出，名称/字段名/类型与原始代码保持一致；物理意义以注释形式（`// 物理意义`）标注，逐字段说明（不存在缩写且完整自描述的字段可以省去注释）
6. 重要的数据交互临界区（例如io缓冲区、用户态的热数据缓存）需细化描述：每个区域说明其**大小计算方式/产生方式（比如输入数据给硬件后返回某个值）、配置来源、物理意义（代表什么，如"一帧 NV12""一个 period 的 PCM"）**，并说明由哪些输入确定
7. 禁止使用表格表达数据结构或组件 I/O（表格不够直观）；一律用代码块 + 注释。仅辅助说明时额外附加的对等概念对比可用表格
8. 内存管理总结用"分配者/持有者/释放者/生命周期"的注释式代码块给出，明确谁分配、谁持有、谁释放、何时归还
9. 端到端数据流用"组件 → 函数调用 → 输出回调"的注释式链路图给出（状态流转可以使用`状态转换：具体函数实现    含义的形式给出，不同状态流转以换行的形式标记），标注跨线程投递点（如 `_encode_poller->async` / `_poller->async`）
10. 文档以 md 格式编写；不使用任何装饰线（===、---、├─ 等），只使用空行分隔章节

示例：

# 采集阶段数据流与内存管理

模块名称: HdmiIn 采集阶段（V4L2 视频 + ALSA 音频）
代码根目录: src/HdmiIn/capture/

说明: 本阶段从硬件采集原始音视频数据。组件输入/输出用函数签名给出，数据结构用伪代码给出
（字段与源码一致），物理意义以注释标注。

## 一、V4L2 视频采集

### 1.1 组件输入/输出

```cpp
// 源: src/HdmiIn/capture/HdmiInV4l2Capture.h
class HdmiInV4l2Capture {
    // 打开设备 + 格式协商 + 建立缓冲；输入: 设备路径/期望宽高/fps/buffer 数
    bool Open（const std::string &device_path, int width, int height, int fps, int buffer_count = 4）;
    // 启动采集线程（STREAMON + CaptureLoop）；输入: 帧回调
    void Start（OnFrameCB cb）;
};

// 帧回调契约: 采集线程每 DQBUF 一帧调用一次
//   frame   —— 一帧视频的描述（见下，不持有像素数据，只引用 VA + dmabuf fd）
//   release —— 归还回调：消费方在帧用完后调用，QBUF 归还驱动（必须且仅调用一次）
using OnFrameCB = std::function<void（HdmiCaptureFrame frame, std::function<void（）> release）>;
```

### 1.2 输出数据结构

```cpp
struct HdmiCaptureFrame {
    uint32_t buffer_index;              // V4L2 环形缓冲槽位号 [0, buffer_count），QBUF 归还索引
    int dmabuf_fd;                      // 该缓冲预导出的 dma-buf fd（Open 时 EXPBUF 缓存），零拷贝句柄
    void *va;                           // plane0 的 mmap 虚拟地址（CPU 读像素用；零拷贝路径不触碰）
    size_t bytesused;                   // 全部平面实际使用字节数（DQBUF 累加，软排空/丢帧判定用）
    uint64_t pts;                       // 采集时刻，内核 MONOTONIC 校准到 ZLM 单调域，单位 ms
    bool key_frame;                     // 是否关键帧（V4L2_BUF_FLAG_KEYFRAME 解析，驱动不支持恒 false）
};
```

### 1.3 缓存区大小配置

```text
buffer_count = 4                    // config.ini [hdmi_in] buffer_count，Open 内钳制 [2,16]，单位: 帧
                                    // 驱动环形缓冲槽数；每槽一帧 NV12。越大越抗无信号抖动，但常驻内存增加
size_image   = 驱动按协商格式定     // 单位: 字节/帧（≈ hor_stride×h×3/2），G_FMT 回读
```

### 1.4 内存管理总结

```text
V4L2 mmap 内存      分配: 驱动（REQBUFS）        释放: Stop（） munmap        生命周期: Open → Stop
V4L2 dmabuf fd      分配: Open（） EXPBUF          释放: Stop（） close         生命周期: Open → Stop
V4L2 buffer 占用态  分配: 驱动 DQBUF/QBUF        释放: release → QBUF       生命周期: 逐帧流转
```
