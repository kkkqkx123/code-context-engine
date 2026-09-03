当需要创建架构地图时，请按以下格式生成：

【文件组织规则】
1. 每个功能模块独立成一个子目录，放在 docs/architecture/map/ 下
2. 每个子目录包含：
   - 一个主描述文件（00_overview.txt），描述模块的整体架构、关键入口和文件间调用关系
   - 若干单文件描述（01_xxx.txt、02_xxx.txt...），每个对应一个核心源文件
3. 单文件描述仅当该文件承担关键枢纽角色时才需要创建
4. 辅助文件仅在主描述文件中列出，不单独成文件
5. 编号在一个子目录内共享。不要跨目录共享

【主描述文件（00_overview.txt）格式要求】
1. 使用纯缩进表示层级，每级缩进2个空格
2. 每个函数/类用 [数字编号] 标记，层级编号如 [1]、[1.1]、[1.1.1]
3. 编号按代码执行顺序排列，从数据流入到数据流出
4. 关键入口用 ★ 标记
5. 实现位置仅标注文件名，不写行号
6. 调用关系标注"被调用"和"调用下级"，位置描述到文件名或阶段即可
7. 顶部列出：模块名称、代码根目录、数据流向简述
8. 底部汇总所有 ★ 标记的入口速查表
9. 不使用装饰线，只使用空行分隔章节

【单文件描述文件（01_xxx.txt）格式要求】
1. 文件名对应源文件名
2. 顶部标注对应的源文件路径和该文件的角色简述
3. 仅列出该文件内的函数/类，编号延续主描述文件
4. 每个条目包含：函数签名、简要说明、被谁调用

【示例】

模块名称: VideoEncoder
代码根目录: src/video/
数据流向: 硬件采集 → 预处理 → 编码 → 封装 → 投送

对外入口

include/video/encoder_api.h
  [1] class VideoEncoderAPI
    [1.1] Init() ★初始化入口
      实现: encoder_api.cpp
      被调用: main.cpp
    [1.2] Start() ★启动入口
      实现: encoder_api.cpp
      被调用: main.cpp
    [1.3] Stop() ★停止入口
      实现: encoder_api.cpp
      被调用: main.cpp

=== 内部核心链(复杂的调用链可以自成一段) ===

src/video/capture/capture_thread.cpp
  [2] CaptureThread
    [2.1] Start()
      被调用: VideoEncoderAPI::Start() [1.2]
      调用下级: ReadFrame() [2.2]
    [2.2] ReadFrame()
      输出到: FrameQueue [3]

src/video/core/preprocessor.cpp
  [3] FrameQueue
    生产者: ReadFrame() [2.2]
    消费者: Preprocessor::Process() [4.1]

  [4] Preprocessor
    [4.1] Process() ★预处理入口
      被调用: CaptureThread 回调
      调用下级: Convert(), Resize()
      输出到: EncoderCore [5]

src/video/core/encoder_core.cpp
  [5] EncoderCore
    [5.1] Open() ★编码器打开
      被调用: VideoEncoderAPI::Start() [1.2]
    [5.2] Encode() ★编码执行
      被调用: Preprocessor::Process() [4.1] 回调
      输出到: Muxer [6]

src/video/mux/muxer.cpp
  [6] Muxer
    [6.1] WritePacket() ★封装入口
      被调用: EncoderCore::Encode() [5.2] 回调
      输出到: MediaQueue [7]

src/video/output/media_thread.cpp
  [7] MediaQueue
    生产者: Muxer::WritePacket() [6.1]
    消费者: MediaThread::Send() [8]

  [8] MediaThread
    [8.1] Send() ★投送入口
      被调用: 队列回调

★关键入口速查

[1.1] Init() → encoder_api.cpp
[1.2] Start() → encoder_api.cpp
[1.3] Stop() → encoder_api.cpp
[4.1] Process() → preprocessor.cpp
[5.1] Open() → encoder_core.cpp
[5.2] Encode() → encoder_core.cpp
[6.1] WritePacket() → muxer.cpp
[8.1] Send() → media_thread.cpp


单文件描述示例

对应源文件: src/video/core/encoder_core.cpp
角色: 编码核心调度器

[5.1] Open()
  说明: 创建编码会话
  被调用: VideoEncoderAPI::Start() [1.2]
  调用下级: NvencSession::Create(), SoftEncoder::Create()

[5.2] Encode()
  说明: 执行编码
  被调用: Preprocessor::Process() [4.1] 回调
  调用下级: NvencSession::EncodeFrame()