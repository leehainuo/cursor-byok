// decode_stored.go 负责从持久化捕获记录恢复文本、帧和 protobuf 视图。
package main

import (
	"bytes"
	"compress/zlib"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"mime"
	"net/url"
	"strings"
	"unicode"
	"unicode/utf8"

	"github.com/andybalholm/brotli"
	agentv1 "github.com/leookun/cursor-byok/cursor-proto/gen/agent/v1"
	aiserverv1 "github.com/leookun/cursor-byok/cursor-proto/gen/aiserver/v1"
	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protoreflect"
	"google.golang.org/protobuf/reflect/protoregistry"
	"google.golang.org/protobuf/types/dynamicpb"
)

// decodeCapturedContent 按内容类型和压缩编码生成可读正文视图。
func decodeCapturedContent(payload []byte, contentType, codec string) (string, string, error) {
	decoded, err := decodeHTTPContent(payload, codec)
	if err != nil {
		return "", "", err
	}
	if len(decoded) == 0 {
		return "", "", nil
	}
	mediaType := normalizedMediaType(contentType)
	if json.Valid(decoded) {
		var formatted bytes.Buffer
		if err := json.Indent(&formatted, decoded, "", "  "); err != nil {
			return string(decoded), "json", err
		}
		return formatted.String(), "json", nil
	}
	if mediaType == "application/x-www-form-urlencoded" && utf8.Valid(decoded) {
		values, parseErr := url.ParseQuery(string(decoded))
		if parseErr != nil {
			return string(decoded), "plaintext", parseErr
		}
		formatted, marshalErr := json.MarshalIndent(values, "", "  ")
		return string(formatted), "json", marshalErr
	}
	if !isTextMediaType(mediaType) || !utf8.Valid(decoded) {
		return "", "", nil
	}
	if strings.ContainsRune(string(decoded), '\x00') {
		return "", "", nil
	}
	language := textLanguage(mediaType)
	if strings.HasSuffix(mediaType, "+json") || mediaType == "application/json" {
		return string(decoded), "json", fmt.Errorf("JSON 正文格式无效")
	}
	return string(decoded), language, nil
}

// decodeHTTPContent 解压 HTTP 内容编码并返回正文副本。
func decodeHTTPContent(payload []byte, codec string) ([]byte, error) {
	encodings := strings.Split(strings.TrimSpace(codec), ",")
	decoded := payload
	for index := len(encodings) - 1; index >= 0; index-- {
		encoding := strings.ToLower(strings.TrimSpace(encodings[index]))
		switch encoding {
		case "", "identity":
		case "gzip", "x-gzip":
			var err error
			decoded, err = decompressPayload(decoded, "gzip")
			if err != nil {
				return nil, err
			}
		case "deflate":
			reader, err := zlib.NewReader(bytes.NewReader(decoded))
			if err != nil {
				return nil, fmt.Errorf("deflate 解压失败：%w", err)
			}
			result, readErr := io.ReadAll(io.LimitReader(reader, maxConnectFrameBytes+1))
			closeErr := reader.Close()
			if readErr != nil {
				return nil, fmt.Errorf("读取 deflate 内容失败：%w", readErr)
			}
			if closeErr != nil {
				return nil, fmt.Errorf("关闭 deflate 内容失败：%w", closeErr)
			}
			if len(result) > maxConnectFrameBytes {
				return nil, fmt.Errorf("deflate 解压后超过 %d 字节限制", maxConnectFrameBytes)
			}
			decoded = result
		case "br":
			result, readErr := io.ReadAll(io.LimitReader(brotli.NewReader(bytes.NewReader(decoded)), maxConnectFrameBytes+1))
			if readErr != nil {
				return nil, fmt.Errorf("读取 Brotli 内容失败：%w", readErr)
			}
			if len(result) > maxConnectFrameBytes {
				return nil, fmt.Errorf("Brotli 解压后超过 %d 字节限制", maxConnectFrameBytes)
			}
			decoded = result
		default:
			return nil, fmt.Errorf("暂不支持内容编码 %q", encoding)
		}
	}
	return decoded, nil
}

// normalizedMediaType 删除参数并统一媒体类型大小写。
func normalizedMediaType(contentType string) string {
	mediaType, _, err := mime.ParseMediaType(strings.TrimSpace(contentType))
	if err == nil {
		return strings.ToLower(mediaType)
	}
	return strings.ToLower(strings.TrimSpace(strings.SplitN(contentType, ";", 2)[0]))
}

// isProtoContentType 判断媒体类型是否表示 protobuf 二进制。
func isProtoContentType(contentType string) bool {
	return strings.Contains(normalizedMediaType(contentType), "proto")
}

// isTextMediaType 判断媒体类型是否适合直接作为文本展示。
func isTextMediaType(mediaType string) bool {
	return strings.HasPrefix(mediaType, "text/") || strings.HasSuffix(mediaType, "+json") ||
		strings.HasSuffix(mediaType, "+xml") || mediaType == "application/json" ||
		mediaType == "application/xml" || mediaType == "application/javascript" ||
		mediaType == "application/x-javascript" || mediaType == "application/graphql"
}

// textLanguage 为前端编辑器选择文本语言。
func textLanguage(mediaType string) string {
	switch {
	case strings.Contains(mediaType, "json"):
		return "json"
	case strings.Contains(mediaType, "xml"):
		return "xml"
	case strings.Contains(mediaType, "html"):
		return "html"
	case strings.Contains(mediaType, "javascript"):
		return "javascript"
	case strings.Contains(mediaType, "css"):
		return "css"
	default:
		return "plaintext"
	}
}

// decodeStoredConnectFrames 从持久化十六进制载荷恢复流式帧。
func decodeStoredConnectFrames(rawHexValue, messageType, codec string) ([]FrameView, error) {
	payload, err := hex.DecodeString(strings.TrimSpace(rawHexValue))
	if err != nil {
		return nil, fmt.Errorf("解析已存储 Connect 正文失败：%w", err)
	}
	frames := make([]FrameView, 0)
	decoder := newConnectFrameDecoder(messageType, codec, defaultMaxFrames, func(frame FrameView) {
		frames = append(frames, frame)
	})
	decoder.Write(payload)
	decoder.Close()
	return frames, nil
}

// isUnaryProtoContentType 判断媒体类型是否为可直接解码的 protobuf。
func isUnaryProtoContentType(contentType string) bool {
	mediaType := normalizedMediaType(contentType)
	return mediaType == "application/proto" || mediaType == "application/protobuf" || mediaType == "application/x-protobuf"
}

// decodeStoredRawPayload 解码持久化原始载荷并应用压缩处理。
func decodeStoredRawPayload(rawHexValue, codec string) ([]byte, error) {
	payload, err := hex.DecodeString(strings.TrimSpace(rawHexValue))
	if err != nil {
		return nil, fmt.Errorf("解析已存储正文失败：%w", err)
	}
	if codec == "" || strings.EqualFold(codec, "identity") {
		return payload, nil
	}
	return decompressPayload(payload, codec)
}

// unaryRequestMessage 根据 RPC 路径创建请求消息和稳定类型名。
func unaryRequestMessage(path string) (proto.Message, string) {
	switch path {
	case forkBackgroundComposerPath:
		return &aiserverv1.ForkBackgroundComposerRequest{}, "fork_background_composer_request"
	case notifyConversationClonePath:
		return &agentv1.NotifyConversationCloneRequest{}, "notify_conversation_clone_request"
	case uploadConversationBlobsPath:
		return &agentv1.UploadConversationBlobsRequest{}, "upload_conversation_blobs_request"
	case cppAvailableModelsPath:
		return &aiserverv1.AvailableCppModelsRequest{}, "available_cpp_models_request"
	case aiAvailableModelsPath:
		return &aiserverv1.AvailableModelsRequest{}, "available_models_request"
	case aiGetDefaultModelPath:
		return &aiserverv1.GetDefaultModelRequest{}, "get_default_model_request"
	case aiDefaultModelNudgeDataPath:
		return &aiserverv1.GetDefaultModelNudgeDataRequest{}, "get_default_model_nudge_data_request"
	case mcpGetKnownServersPath:
		return &aiserverv1.GetKnownServersRequest{}, "get_known_servers_request"
	case serverGetConfigPath:
		return &aiserverv1.GetServerConfigRequest{}, "get_server_config_request"
	default:
		method := rpcMethodDescriptor(path)
		if method == nil || method.IsStreamingClient() || method.IsStreamingServer() {
			return nil, ""
		}
		return dynamicpb.NewMessage(method.Input()), protoMessageKind(method.Input())
	}
}

// unaryResponseMessage 根据 RPC 路径创建响应消息和稳定类型名。
func unaryResponseMessage(path string) (proto.Message, string) {
	switch path {
	case forkBackgroundComposerPath:
		return &aiserverv1.ForkBackgroundComposerResponse{}, "fork_background_composer_response"
	case notifyConversationClonePath:
		return &agentv1.NotifyConversationCloneResponse{}, "notify_conversation_clone_response"
	case uploadConversationBlobsPath:
		return &agentv1.UploadConversationBlobsResponse{}, "upload_conversation_blobs_response"
	case cppAvailableModelsPath:
		return &aiserverv1.AvailableCppModelsResponse{}, "available_cpp_models_response"
	case aiAvailableModelsPath:
		return &aiserverv1.AvailableModelsResponse{}, "available_models_response"
	case aiGetDefaultModelPath:
		return &aiserverv1.GetDefaultModelResponse{}, "get_default_model_response"
	case aiDefaultModelNudgeDataPath:
		return &aiserverv1.GetDefaultModelNudgeDataResponse{}, "get_default_model_nudge_data_response"
	case mcpGetKnownServersPath:
		return &aiserverv1.GetKnownServersResponse{}, "get_known_servers_response"
	case serverGetConfigPath:
		return &aiserverv1.GetServerConfigResponse{}, "get_server_config_response"
	default:
		method := rpcMethodDescriptor(path)
		if method == nil || method.IsStreamingClient() || method.IsStreamingServer() {
			return nil, ""
		}
		return dynamicpb.NewMessage(method.Output()), protoMessageKind(method.Output())
	}
}

// streamingRequestMessageType 返回流式请求的 protobuf 类型名。
func streamingRequestMessageType(path string) string {
	if path == runSSEPath {
		return "aiserver.v1.BidiRequestId"
	}
	method := rpcMethodDescriptor(path)
	if method == nil || (!method.IsStreamingClient() && !method.IsStreamingServer()) {
		return ""
	}
	return string(method.Input().FullName())
}

// streamingResponseMessageType 返回流式响应的 protobuf 类型名。
func streamingResponseMessageType(path string) string {
	if path == runSSEPath {
		return "agent.v1.AgentServerMessage"
	}
	method := rpcMethodDescriptor(path)
	if method == nil || (!method.IsStreamingClient() && !method.IsStreamingServer()) {
		return ""
	}
	return string(method.Output().FullName())
}

// decodesUnaryRequest 判断是否存在已知的一元请求解码器。
func decodesUnaryRequest(path string) bool {
	if path == bidiAppendPath {
		return true
	}
	message, _ := unaryRequestMessage(path)
	return message != nil
}

// decodesUnaryResponse 判断是否存在已知的一元响应解码器。
func decodesUnaryResponse(path string) bool {
	message, _ := unaryResponseMessage(path)
	return message != nil
}

// newMessage 按完整 protobuf 类型名从注册表创建消息实例。
func newMessage(messageType string) proto.Message {
	switch messageType {
	case "aiserver.v1.BidiRequestId":
		return &aiserverv1.BidiRequestId{}
	case "agent.v1.AgentServerMessage":
		return &agentv1.AgentServerMessage{}
	default:
		descriptor, err := protoregistry.GlobalFiles.FindDescriptorByName(protoreflect.FullName(messageType))
		if err != nil {
			return nil
		}
		messageDescriptor, ok := descriptor.(protoreflect.MessageDescriptor)
		if !ok {
			return nil
		}
		return dynamicpb.NewMessage(messageDescriptor)
	}
}

// rpcMethodDescriptor 通过完整 RPC 路径查找注册表中的方法描述。
func rpcMethodDescriptor(path string) protoreflect.MethodDescriptor {
	parts := strings.Split(strings.Trim(strings.TrimSpace(path), "/"), "/")
	if len(parts) != 2 || parts[0] == "" || parts[1] == "" {
		return nil
	}
	descriptor, err := protoregistry.GlobalFiles.FindDescriptorByName(protoreflect.FullName(parts[0]))
	if err != nil {
		return nil
	}
	service, ok := descriptor.(protoreflect.ServiceDescriptor)
	if !ok {
		return nil
	}
	return service.Methods().ByName(protoreflect.Name(parts[1]))
}

// protoMessageKind 从消息描述推导稳定的 JSON kind 名称。
func protoMessageKind(descriptor protoreflect.MessageDescriptor) string {
	if descriptor == nil {
		return ""
	}
	return snakeCase(string(descriptor.Name()))
}

// snakeCase 将 protobuf 名称转换为前端稳定的下划线命名。
func snakeCase(value string) string {
	var result strings.Builder
	for index, character := range value {
		if unicode.IsUpper(character) {
			if index > 0 {
				result.WriteByte('_')
			}
			result.WriteRune(unicode.ToLower(character))
			continue
		}
		result.WriteRune(character)
	}
	return result.String()
}

// marshalProtoJSON 把 protobuf 消息编码为前端可读 JSON。
func marshalProtoJSON(message proto.Message) string {
	if message == nil {
		return ""
	}
	payload, err := (protojson.MarshalOptions{
		UseProtoNames:   true,
		EmitUnpopulated: false,
		Indent:          "  ",
	}).Marshal(message)
	if err != nil {
		return ""
	}
	return string(payload)
}

// activeOneofName 返回 Agent 消息当前激活的 oneof 名称。
func activeOneofName(message proto.Message) string {
	if message == nil {
		return ""
	}
	reflected := message.ProtoReflect()
	oneofs := reflected.Descriptor().Oneofs()
	for index := 0; index < oneofs.Len(); index++ {
		oneof := oneofs.Get(index)
		field := reflected.WhichOneof(oneof)
		if field != nil {
			return string(field.Name())
		}
	}
	return string(reflected.Descriptor().Name())
}

// prettyJSON 尝试格式化 JSON，失败时返回原始文本。
func prettyJSON(payload []byte) string {
	var target any
	if err := json.Unmarshal(payload, &target); err != nil {
		return string(payload)
	}
	formatted, err := json.MarshalIndent(target, "", "  ")
	if err != nil {
		return string(payload)
	}
	return string(formatted)
}

// clippedHex 限制原始载荷展示长度并标记省略部分。
func clippedHex(payload []byte, max int) string {
	if len(payload) > max {
		return hex.EncodeToString(payload[:max]) + "..."
	}
	return hex.EncodeToString(payload)
}
