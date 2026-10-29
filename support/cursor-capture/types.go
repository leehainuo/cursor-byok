// types.go 定义协议调试器配置、捕获详情和会话摘要模型。
package main

import (
	"os"
	"path/filepath"
	"time"
)

// 默认值限制调试器只监听本机并约束内存捕获规模。
const (
	defaultServiceAddr     = "127.0.0.1:9090"
	defaultUpstreamURL     = "https://api2.cursor.sh"
	debugBasePath          = "/__debuger__"
	defaultMaxExchanges    = 200
	defaultMaxCaptureBytes = 2 << 20
	defaultMaxFrames       = 2000
	defaultDatabaseName    = "cursor-proxy-debugger.db"
)

// Config 控制独立协议调试器的监听和存储限制。
type Config struct {
	// ServiceAddr 是 Cursor API 调试服务监听地址。
	ServiceAddr string
	// MaxExchanges 是内存保留的最大请求数。
	MaxExchanges int
	// MaxCaptureBytes 是单向载荷保存上限。
	MaxCaptureBytes int
	// MaxFrames 是单条流保存的 Connect 帧上限。
	MaxFrames int
	// DatabasePath 是 SQLite 捕获数据库路径。
	DatabasePath string
}

// normalized 补齐空值并拒绝无效的容量配置。
func (config Config) normalized() Config {
	if config.ServiceAddr == "" {
		config.ServiceAddr = defaultServiceAddr
	}
	if config.MaxExchanges <= 0 {
		config.MaxExchanges = defaultMaxExchanges
	}
	if config.MaxCaptureBytes <= 0 {
		config.MaxCaptureBytes = defaultMaxCaptureBytes
	}
	if config.MaxFrames <= 0 {
		config.MaxFrames = defaultMaxFrames
	}
	if config.DatabasePath == "" {
		config.DatabasePath = defaultDatabasePath()
	}
	return config
}

// defaultDatabasePath 返回当前用户配置目录下的默认数据库路径。
func defaultDatabasePath() string {
	configDir, err := os.UserConfigDir()
	if err != nil || configDir == "" {
		return defaultDatabaseName
	}
	return filepath.Join(configDir, "cursor-byok", defaultDatabaseName)
}

// ExchangeSummary 是请求列表使用的紧凑捕获摘要。
type ExchangeSummary struct {
	// ID 是进程内递增的捕获标识。
	ID string `json:"id"`
	// StartedAt 是请求开始时间。
	StartedAt time.Time `json:"startedAt"`
	// Method 是 HTTP 方法。
	Method string `json:"method"`
	// URL 是完整请求地址。
	URL string `json:"url"`
	// Host 是请求目标主机。
	Host string `json:"host"`
	// Path 是 RPC 或 HTTP 路径。
	Path string `json:"path"`
	// Status 是 HTTP 响应状态码。
	Status int `json:"status"`
	// State 是捕获处理阶段。
	State string `json:"state"`
	// DurationMS 是请求总耗时毫秒数。
	DurationMS int64 `json:"durationMs"`
	// RequestBytes 是完整请求体字节数。
	RequestBytes int64 `json:"requestBytes"`
	// ResponseBytes 是完整响应体字节数。
	ResponseBytes int64 `json:"responseBytes"`
	// RequestID 是协议请求标识。
	RequestID string `json:"requestId,omitempty"`
	// ConversationID 是关联会话标识。
	ConversationID string `json:"conversationId,omitempty"`
	// RequestKind 是解码后的请求消息类型。
	RequestKind string `json:"requestKind,omitempty"`
	// ResponseKind 是解码后的响应消息类型。
	ResponseKind string `json:"responseKind,omitempty"`
	// FrameCount 是双向 Connect 帧总数。
	FrameCount int `json:"frameCount"`
	// Error 是转发或解码错误。
	Error string `json:"error,omitempty"`
}

// Exchange 保存调试界面展示的请求和响应详情。
type Exchange struct {
	ExchangeSummary
	// Request 是请求方向载荷。
	Request Payload `json:"request"`
	// Response 是响应方向载荷。
	Response Payload `json:"response"`
}

// Payload 保存请求头、原始副本、解码正文和协议帧。
type Payload struct {
	// Headers 是脱敏且排序稳定的 HTTP 请求头。
	Headers []Header `json:"headers"`
	// ContentType 是规范化媒体类型。
	ContentType string `json:"contentType,omitempty"`
	// ContentCodec 是内容压缩算法。
	ContentCodec string `json:"contentCodec,omitempty"`
	// Size 是完整方向载荷字节数。
	Size int64 `json:"size"`
	// RawHex 是受限原始副本的十六进制文本。
	RawHex string `json:"rawHex,omitempty"`
	// RawTruncated 表示原始副本达到保存上限。
	RawTruncated bool `json:"rawTruncated,omitempty"`
	// DecodedJSON 是格式化后的结构化正文。
	DecodedJSON string `json:"decodedJson,omitempty"`
	// DecodedLang 是前端编辑器使用的语言标识。
	DecodedLang string `json:"decodedLanguage,omitempty"`
	// DecodeError 是不影响转发的解码错误。
	DecodeError string `json:"decodeError,omitempty"`
	// Frames 是 Connect 流的逐帧视图。
	Frames []FrameView `json:"frames,omitempty"`
}

// Header 是排序稳定的 HTTP 请求头键值对。
type Header struct {
	// Name 是请求头名称。
	Name string `json:"name"`
	// Value 是已脱敏的请求头值。
	Value string `json:"value"`
}

// FrameView 描述一条 Connect 流式信封。
type FrameView struct {
	// Index 是帧在当前方向的序号。
	Index int `json:"index"`
	// Flags 是 Connect 原始标志位。
	Flags uint8 `json:"flags"`
	// Length 是解压前帧载荷长度。
	Length int `json:"length"`
	// Compressed 表示帧载荷使用压缩。
	Compressed bool `json:"compressed"`
	// EndStream 表示帧携带流结束标志。
	EndStream bool `json:"endStream"`
	// Kind 是解码后的业务消息类型。
	Kind string `json:"kind,omitempty"`
	// MessageType 是 protobuf 完整消息名。
	MessageType string `json:"messageType,omitempty"`
	// RequestID 是帧中解析出的请求标识。
	RequestID string `json:"requestId,omitempty"`
	// JSON 是 protobuf 的 JSON 视图。
	JSON string `json:"json,omitempty"`
	// RawHex 是无法解码时保留的载荷文本。
	RawHex string `json:"rawHex,omitempty"`
	// Error 是当前帧的解压或解码错误。
	Error string `json:"error,omitempty"`
}

// storeEvent 是 SSE 通知使用的最小变化事件。
type storeEvent struct {
	// Type 是捕获记录变化类型。
	Type string `json:"type"`
	// ID 是关联捕获标识。
	ID string `json:"id,omitempty"`
}

// ConversationSummary 描述按会话聚合的持久化流量。
type ConversationSummary struct {
	// ConversationID 是会话稳定标识。
	ConversationID string `json:"conversationId"`
	// ExchangeCount 是会话捕获记录数。
	ExchangeCount int `json:"exchangeCount"`
	// LastStartedAt 是会话最近请求时间。
	LastStartedAt time.Time `json:"lastStartedAt"`
	// RequestBytes 是会话累计请求字节数。
	RequestBytes int64 `json:"requestBytes"`
	// ResponseBytes 是会话累计响应字节数。
	ResponseBytes int64 `json:"responseBytes"`
}
