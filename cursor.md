
## 完整目录

```text
server/
├── src/                                        # ≈36,000 行；服务端全部业务代码
│   ├── app.rs                                  # ≈180 行；依赖组装和服务启动
│   ├── config.rs                               # ≈180 行；进程配置
│   ├── error.rs                                # ≈150 行；统一错误
│   ├── network.rs                              # ≈100 行；网络公共配置
│   │
│   ├── bin/                                    # ≈100 行；可执行程序入口
│   │   └── cursor-server.rs                    # ≈100 行；启动服务
│   │
│   ├── api/                                    # ≈1,500 行；HTTP/Connect API
│   │   ├── mod.rs                              # ≈20 行；模块导出
│   │   ├── router.rs                           # ≈100 行；总路由
│   │   └── cursor/                             # ≈1,350 行；Cursor API
│   │       ├── mod.rs                          # ≈20 行；Cursor 路由
│   │       ├── bidi.rs                         # ≈250 行；上行请求
│   │       ├── run_sse.rs                      # ≈300 行；下行订阅
│   │       ├── handlers.rs                     # ≈450 行；其他 Cursor API
│   │       └── proxy.rs                        # ≈330 行；本地/官方服务选择
│   │
│   ├── cursor/                                 # ≈18,000 行；Cursor Agent 适配层
│   │   ├── mod.rs                              # ≈40 行；公共导出
│   │   │
│   │   ├── transport/                          # ≈800 行；request_id 双向通道
│   │   │   ├── mod.rs                          # ≈20 行；模块导出
│   │   │   ├── registry.rs                     # ≈220 行；request_id → TransportHandle
│   │   │   ├── handle.rs                       # ≈200 行；输入、订阅和终态
│   │   │   ├── inbox.rs                        # ≈70 行；append_seqno 排序
│   │   │   └── output.rs                       # ≈290 行；缓存、广播、重放、关闭
│   │   │
│   │   ├── conversation/                       # ≈1,600 行；Conversation 运行协调
│   │   │   ├── mod.rs                          # ≈30 行；公共类型
│   │   │   ├── registry.rs                     # ≈220 行；conversation_id → Runtime
│   │   │   ├── runtime.rs                      # ≈500 行；current_run 唯一所有者
│   │   │   ├── command.rs                      # ≈120 行；Start/Action/Cancel/Disconnect
│   │   │   ├── delivery.rs                     # ≈280 行；Ignore/Insert/Break
│   │   │   ├── pending.rs                      # ≈150 行；Run 边界上的待处理消息
│   │   │   └── output.rs                       # ≈300 行；RunEvent 下行及 Step 记录
│   │   │
│   │   ├── compile/                            # ≈2,700 行；Cursor 输入编译
│   │   │   ├── mod.rs                          # ≈30 行；统一入口
│   │   │   ├── run.rs                          # ≈650 行；RunRequest → PreparedRun
│   │   │   ├── context.rs                      # ≈650 行；rules/skills/MCP/environment
│   │   │   ├── action.rs                       # ≈250 行；Action 分类和路由
│   │   │   ├── insert_messages.rs              # ≈400 行；非打断消息
│   │   │   ├── break_messages.rs               # ≈400 行；打断当前 cycle 的消息
│   │   │   ├── images.rs                       # ≈100 行；图片和 Blob
│   │   │   └── model.rs                        # ≈220 行；Cursor model → Provider model
│   │   │
│   │   ├── checkpoint/                         # ≈2,200 行；Conversation 持久化和恢复
│   │   │   ├── mod.rs                          # ≈30 行；公共接口
│   │   │   ├── builder.rs                      # ≈280 行；构建 Checkpoint
│   │   │   ├── steps.rs                        # ≈120 行；尚未持久化的步骤缓存
│   │   │   ├── turns.rs                        # ≈150 行；Conversation turns
│   │   │   ├── roots.rs                        # ≈150 行；稳定根消息
│   │   │   ├── recovery.rs                     # ≈100 行；恢复 Conversation
│   │   │   ├── summary.rs                      # ≈120 行；压缩摘要
│   │   │   ├── derived.rs                      # ≈220 行；Todo/Plan 等派生状态
│   │   │   ├── worker.rs                       # ≈250 行；异步持久化和 barrier
│   │   │   └── messages/                       # ≈800 行；Message 编解码
│   │   │       ├── mod.rs                      # ≈20 行；统一入口
│   │   │       ├── decode.rs                   # ≈250 行；Checkpoint → Message
│   │   │       ├── encode.rs                   # ≈280 行；Message → Checkpoint
│   │   │       └── tests.rs                    # ≈250 行；稳定性测试
│   │   │
│   │   ├── tools/                              # ≈6,500 行；可扩展 Tool 系统
│   │   │   ├── mod.rs                          # ≈180 行；公共类型和注册
│   │   │   ├── registry.rs                     # ≈180 行；Tool 定义
│   │   │   ├── runtime.rs                      # ≈420 行；运行状态和取消
│   │   │   ├── stream.rs                       # ≈350 行；流式参数
│   │   │   ├── edit.rs                         # ≈340 行；编辑状态
│   │   │   ├── schedule.rs                     # ≈100 行；后台任务调度
│   │   │   ├── compat.rs                       # ≈150 行；兼容工具转换
│   │   │   │
│   │   │   ├── codec/                          # ≈1,750 行；Tool Wire Protocol
│   │   │   │   ├── mod.rs                      # ≈20 行；模块导出
│   │   │   │   ├── request.rs                  # ≈520 行；执行请求编码
│   │   │   │   ├── response.rs                 # ≈360 行；执行响应编码
│   │   │   │   ├── query.rs                    # ≈250 行；InteractionQuery
│   │   │   │   └── render.rs                   # ≈600 行；Cursor Tool 卡片
│   │   │   │
│   │   │   ├── tool_call_dispatch/             # ≈700 行；ToolCall 分发
│   │   │   │   ├── mod.rs                      # ≈260 行；主 Dispatcher
│   │   │   │   ├── exec.rs                     # ≈80 行；命令执行
│   │   │   │   ├── edit.rs                     # ≈40 行；编辑调用
│   │   │   │   ├── interaction.rs              # ≈260 行；用户交互
│   │   │   │   ├── local.rs                    # ≈30 行；本地工具
│   │   │   │   └── search.rs                   # ≈40 行；搜索工具
│   │   │   │
│   │   │   └── tool_call_result/               # ≈3,000 行；ToolResult 消费
│   │   │       ├── mod.rs                      # ≈180 行；统一结果
│   │   │       ├── gate.rs                     # ≈850 行；完成关联和门控
│   │   │       ├── interaction.rs              # ≈450 行；用户交互结果
│   │   │       ├── local.rs                    # ≈220 行；本地工具结果
│   │   │       ├── mcp.rs                      # ≈80 行；MCP 结果
│   │   │       ├── mcp_state.rs                # ≈150 行；MCP 状态
│   │   │       ├── search.rs                   # ≈150 行；搜索结果
│   │   │       └── exec/                       # ≈920 行；命令执行结果
│   │   │           ├── mod.rs                   # ≈180 行；执行结果入口
│   │   │           ├── output.rs                # ≈500 行；输出处理
│   │   │           └── render.rs                # ≈240 行；结果渲染
│   │   │
│   │   ├── protocol/                           # ≈600 行；非 Tool Wire Protocol
│   │   │   ├── mod.rs                          # ≈20 行；模块导出
│   │   │   ├── proto.rs                        # ≈80 行；protobuf 类型
│   │   │   ├── connect.rs                      # ≈150 行；Connect framing
│   │   │   ├── json_stream.rs                  # ≈280 行；JSON 流
│   │   │   └── events.rs                       # ≈300 行；实时下行消息
│   │   │
│   │   ├── prompting/                          # ≈650 行；Prompt 编译
│   │   │   ├── mod.rs                          # ≈20 行；模块导出
│   │   │   ├── compiler.rs                     # ≈120 行；PromptSpec 编译
│   │   │   ├── catalog.rs                      # ≈100 行；Prompt 目录
│   │   │   ├── assets.rs                       # ≈220 行；资源加载
│   │   │   └── derived_state.rs                # ≈190 行；稳定派生上下文
│   │   │
│   │   └── services/                           # ≈2,800 行；非 Agent Loop 服务
│   │       ├── mod.rs                          # ≈30 行；模块导出
│   │       ├── account.rs                      # ≈470 行；账号信息
│   │       ├── analytics.rs                    # ≈240 行；Analytics
│   │       ├── blob_sync.rs                    # ≈320 行；Blob 同步
│   │       ├── context_sync.rs                 # ≈200 行；上下文同步
│   │       ├── model_catalog.rs                # ≈730 行；模型目录
│   │       ├── observability.rs                # ≈230 行；Cursor Trace
│   │       ├── tab.rs                          # ≈80 行；Tab 信息
│   │       └── usage.rs                        # ≈350 行；用量统计
│   │
│   ├── run/                                    # ≈2,400 行；通用 Agent Loop
│   │   ├── mod.rs                              # ≈30 行；公共接口
│   │   ├── engine.rs                           # ≈550 行；Loop 主流程
│   │   ├── handle.rs                           # ≈180 行；RunHandle/RunPhase
│   │   ├── command.rs                          # ≈180 行；RunCommand/CommandResult
│   │   ├── event.rs                            # ≈180 行；RunEvent/RunOutcome
│   │   ├── model_cycle.rs                      # ≈380 行；单次 LLM 调用
│   │   ├── tool_round.rs                       # ≈320 行；单轮 Tool 调用
│   │   ├── messages.rs                         # ≈220 行；幂等追加消息
│   │   ├── compaction.rs                       # ≈260 行；显式上下文压缩
│   │   └── port.rs                             # ≈100 行；外部端口
│   │
│   ├── model/                                  # ≈1,900 行；公共数据类型
│   │   ├── mod.rs                              # ≈30 行；模块导出
│   │   ├── conversation.rs                     # ≈100 行；Conversation 类型
│   │   ├── checkpoint.rs                       # ≈80 行；Checkpoint 类型
│   │   ├── message.rs                          # ≈180 行；Message 类型
│   │   ├── run.rs                              # ≈100 行；Run 类型
│   │   ├── tool.rs                             # ≈100 行；ToolCall/ToolResult
│   │   ├── inference.rs                        # ≈150 行；模型请求和响应
│   │   ├── projection.rs                       # ≈180 行；Provider 输入消息
│   │   ├── configuration.rs                    # ≈550 行；模型配置
│   │   ├── observability.rs                    # ≈300 行；调用观测
│   │   ├── token_count.rs                      # ≈50 行；Token 统计
│   │   └── tool_result_replay.rs               # ≈230 行；ToolResult 恢复
│   │
│   ├── provider/                               # ≈3,000 行；Provider 适配
│   │   ├── mod.rs                              # ≈80 行；Provider trait
│   │   ├── router.rs                           # ≈230 行；Provider 路由
│   │   ├── event.rs                            # ≈100 行；统一流事件
│   │   ├── normalize.rs                        # ≈50 行；响应归一化
│   │   ├── retry.rs                            # ≈270 行；重试
│   │   ├── recorder.rs                         # ≈600 行；调用记录
│   │   ├── anthropic.rs                        # ≈500 行；Anthropic
│   │   ├── openai_chat.rs                      # ≈580 行；Chat Completions
│   │   └── openai_responses.rs                 # ≈650 行；Responses
│   │
│   ├── store/                                  # ≈4,100 行；本地持久化
│   │   ├── mod.rs                              # ≈40 行；Store 接口
│   │   ├── sqlite.rs                           # ≈60 行；SQLite 初始化
│   │   ├── writer.rs                           # ≈30 行；串行写事务
│   │   ├── cas.rs                              # ≈120 行；并发写检查
│   │   ├── conversations.rs                    # ≈180 行；Conversation
│   │   ├── checkpoints.rs                      # ≈400 行；Checkpoint
│   │   ├── messages.rs                         # ≈150 行；Message 和幂等
│   │   ├── runs.rs                             # ≈300 行；Run
│   │   ├── tool_rounds.rs                      # ≈330 行；Tool Round
│   │   ├── input_anchors.rs                    # ≈60 行；输入去重
│   │   ├── llm_calls.rs                        # ≈650 行；LLM 调用记录
│   │   ├── models.rs                           # ≈430 行；模型配置
│   │   ├── settings.rs                         # ≈430 行；应用设置
│   │   ├── storage.rs                          # ≈230 行；Blob 存储
│   │   ├── cursor_traces.rs                    # ≈400 行；Cursor Trace
│   │   └── overview.rs                         # ≈350 行；控制台查询
│   │
│   ├── control/                                # ≈2,100 行；管理端 API
│   │   ├── mod.rs                              # ≈30 行；模块导出
│   │   ├── service.rs                          # ≈500 行；管理端服务
│   │   ├── settings.rs                         # ≈350 行；设置接口
│   │   ├── models.rs                           # ≈350 行；模型接口
│   │   ├── overview.rs                         # ≈300 行；概览
│   │   ├── calls.rs                            # ≈250 行；调用记录
│   │   ├── ads.rs                              # ≈150 行；广告配置
│   │   └── harness.rs                          # ≈170 行；Harness 控制
│   │
│   ├── search/                                 # ≈1,400 行；搜索能力
│   │   ├── mod.rs                              # ≈30 行；模块导出
│   │   ├── engine.rs                           # ≈350 行；搜索入口
│   │   ├── catalog.rs                          # ≈250 行；搜索服务目录
│   │   ├── federation.rs                       # ≈280 行；聚合搜索
│   │   ├── fetch.rs                            # ≈250 行；网页获取
│   │   └── search_provider.rs                  # ≈240 行；搜索 Provider
│   │
│   └── local_app/                              # ≈1,000 行；本地运行环境
│       ├── mod.rs                              # ≈100 行；local_app 入口（原Harness）
│       ├── account.rs                          # ≈150 行；账号
│       ├── proxy.rs                            # ≈250 行；代理
│       ├── settings.rs                         # ≈200 行；设置
│       └── ca/                                 # ≈300 行；证书
│           ├── mod.rs                          # ≈250 行；CA 实现
│           └── windows.rs                      # ≈50 行；Windows 支持
│
└── tests/                                      # ≈3,500 行；跨模块行为测试
    ├── conversation_delivery.rs                # ≈400 行；消息投递时序
    ├── interrupt.rs                            # ≈400 行；Break 和取消
    ├── error_lifecycle.rs                      # ≈300 行；终态唯一性
    ├── checkpoint_recovery.rs                  # ≈350 行；恢复
    ├── prefix_stability.rs                     # ≈450 行；前缀稳定
    ├── compaction.rs                           # ≈300 行；压缩
    ├── tool_round.rs                           # ≈450 行；Tool Round
    └── connect_wire.rs                         # ≈300 行；Wire Protocol
```

## 顶层架构

```text
                              Cursor Client
                    ┌──────────────┴──────────────┐
                    │                             │
                Bidi 上行                     RunSSE 下行
                    │                             ▲
                    ▼                             │
          ┌──────────────────────┐                │
          │ Transport            │                │
          │                      │                │
          │ request_id           │                │
          │ OrderedInbox         │                │
          │ OutputHub ────────────────────────────┘
          └──────────┬───────────┘
                     │
                     │ conversation_id
                     ▼
        ┌──────────────────────────────┐
        │ ConversationRegistry         │
        │                              │
        │ conversation_id              │
        │ → ConversationRuntime        │
        └──────────────┬───────────────┘
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Conversation                                                │
│                                                             │
│ Messages                                                    │
│ current_run: Option<RunHandle>                               │
│ pending_messages                                            │
│ Checkpoint                                                  │
│ Transport bindings                                          │
│                                                             │
│ 唯一负责：                                                  │
│ 创建 Run / 投递 Message / Cancel / RunOutcome / 输出终态    │
└──────────────┬────────────────┬─────────────────────────────┘
               │                │
         RunCommand            Checkpoint
               │                │
               ▼                ▼
      ┌─────────────────┐   ┌──────────────────────┐
      │ RunEngine       │   │ CheckpointBuilder    │
      │                 │   │                      │
      │ Model Cycle     │   │ Messages             │
      │ Tool Round      │   │ Turns                │
      │ Message Append  │   │ Steps                │
      │ Compaction      │   │ Derived State        │
      └────────┬────────┘   └──────────┬───────────┘
               │                       │
        ┌──────┴───────┐               ▼
        │              │        ┌──────────────────┐
        ▼              ▼        │ Store            │
    Provider      Tool Runtime   │                  │
        │              │        │ Conversations    │
        └──────┬───────┘        │ Messages         │
               │                │ Checkpoints      │
               └───────────────→│ Runs             │
                                │ Tool Rounds       │
                                └──────────────────┘
```

## 上行主链路

```text
BidiAppendRequest
        │
        ▼
api/cursor/bidi.rs
├── decode request_id
├── decode append_seqno
└── decode AgentClientMessage
        │
        ▼
TransportRegistry
        │
        ▼
OrderedInbox
        │
        ▼
compile/action.rs
        │
        ├── Ignore
        ├── InsertMessages
        └── BreakMessages
        │
        ▼
ConversationRuntime
        │
        ▼
current_run
```

## 下行主链路

```text
RunEvent
   │
   ▼
conversation/output.rs
   │
   ├── protocol/events.rs
   │       │
   │       ▼
   │   AgentServerMessage
   │       │
   │       ▼
   │   Transport OutputHub
   │       │
   │       ▼
   │     RunSSE
   │
   └── checkpoint/steps.rs
           │
           ▼
       StepBuffer
           │
           ▼
       CheckpointWorker
```

## Message 编译

```text
Cursor Action
     │
     ▼
compile/action.rs
     │
     ▼
CompiledMessages
├── event_id
├── target_run_id
├── messages
└── delivery
     │
     ├── Ignore
     ├── InsertMessages
     └── BreakMessages
```

## Message 投递

```text
                         Ignore        InsertMessages       BreakMessages

Run 开始前              丢弃          initial_messages     initial_messages

Run 运行中              丢弃          等当前 cycle 完成    取消当前 cycle
                                      后追加               后追加

Run Finalizing          丢弃          pending_messages     pending_messages

Run 结束后              丢弃          启动下一个 Run       启动下一个 Run
```

带 `target_run_id` 时：

```text
target_run_id == current_run_id
└── 按 delivery 消费

target_run_id != current_run_id
└── StaleTarget，忽略
```

## RunEngine

```text
RunEngine
│
├── Running
│   ├── 接受 InsertMessages
│   ├── 接受 BreakMessages
│   ├── 接受 ToolResult
│   └── 接受 Cancel
│
├── Finalizing
│   ├── 拒绝新消息
│   ├── 提交最终 Message
│   ├── 等待 Checkpoint barrier
│   └── 返回 RunClosing
│
└── Ended
    └── 返回 RunEnded
```

```text
RunCommand
├── InsertMessages(MessageBatch)
├── BreakMessages(MessageBatch)
├── ToolResult(ToolResult)
└── Cancel

CommandResult
├── Applied
├── Duplicate
├── RunClosing
├── RunEnded
└── StaleTarget
```

## InsertMessages

```text
同一个 Run
│
├── LLM Call #1 正在执行
│       │
│       └── 收到 InsertMessages
│               └── pending_insertions
│
├── LLM Call #1 完成
├── 提交 Assistant Message
├── 追加 InsertMessages
├── 持久化 Checkpoint
└── LLM Call #2
```

不会创建新 Run。

## BreakMessages

```text
同一个 Run
│
├── LLM Call / Tool Round 正在执行
│       │
│       └── 收到 BreakMessages
│
├── 取消当前 cycle
├── 中止未完成 Tool
├── 写入 interrupted ToolResult
├── 追加 BreakMessages
├── 持久化 Checkpoint
└── 重新进入 Model Cycle
```

取消的是当前 cycle，不是整个 Run。

## Tool 链路

```text
RunEngine
    │ ToolCall
    ▼
ConversationRuntime
    │
    ▼
ToolDispatcher
    │
    ├── Local Tool
    ├── Exec Tool
    ├── Edit Tool
    ├── Interaction Tool
    ├── Search Tool
    ├── MCP Tool
    └── Subagent Tool
    │
    ▼
ToolRuntime
    │
    ├── stream
    ├── cancel
    ├── result gate
    └── completion
    │
    ▼
ToolResult
    │
    ▼
RunEngine
```

Tool 的 Cursor Wire Protocol：

```text
ToolCall
├── tools/codec/query.rs
│       └── InteractionQuery
├── tools/codec/render.rs
│       └── Cursor Tool 卡片
├── tools/codec/request.rs
│       └── Exec 请求
└── tools/codec/response.rs
        └── Exec 响应
```

## Checkpoint 链路

持久化：

```text
Conversation Messages
        │
        ▼
checkpoint/messages/encode.rs
        │
        ▼
Stable root messages
        │
        ├── Turns
        ├── Steps
        ├── Tool state
        ├── Todo/Plan
        └── Read paths
        │
        ▼
Checkpoint
        │
        ▼
Cursor ConversationState
```

恢复：

```text
Cursor ConversationState
        │
        ▼
checkpoint/recovery.rs
        │
        ▼
checkpoint/messages/decode.rs
        │
        ▼
Conversation Messages
        │
        ▼
PreparedRun
```

稳定性：

```text
没有压缩
└── 之前的 Message 不修改、不删除、不重排
    └── 新 Message 只追加

发生压缩
└── 显式替换 Checkpoint roots
    └── 保留最新稳定上下文
```

## Cancel 链路

```text
Bidi Cancel / RunSSE Disconnect / Shutdown
                    │
                    ▼
         ConversationRuntime
                    │
          ┌─────────┴─────────┐
          │                   │
          ▼                   ▼
     RunHandle.cancel     ToolRuntime.abort
          │                   │
          └─────────┬─────────┘
                    ▼
                RunOutcome
                    │
                    ▼
             Final Checkpoint
                    │
                    ▼
          TransportHandle.terminal
                    │
                    ▼
             OutputHub.close
```

只有 `ConversationRuntime` 可以：

```text
Cancel current_run
结束 Tool
发送 terminal
关闭 OutputHub
删除 request_id 路由
```

## 模块依赖

```text
api
└── cursor

cursor/transport
└── cursor/conversation

cursor/conversation
├── cursor/compile
├── cursor/checkpoint
├── cursor/tools
├── cursor/protocol
└── run

run
├── model
├── provider
└── store

cursor/checkpoint
├── model
├── store
└── cursor/protocol

cursor/tools
├── model
├── store
└── cursor/protocol

provider
└── model

store
└── model
```

禁止反向依赖：

```text
run       ─X→ cursor
provider  ─X→ cursor
store     ─X→ cursor
model     ─X→ cursor
```




## 最终核心

```text
Bidi
  → Transport(request_id)
  → Compile
  → Conversation(conversation_id)
  → RunEngine
  → Provider / Tools
  → Conversation
  → Checkpoint
  → Transport
  → RunSSE
```
