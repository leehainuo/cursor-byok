// capture.go 在不影响上游转发的前提下截取有限大小的 HTTP 流。
package main

import (
	"bytes"
	"encoding/hex"
	"io"
	"sync"
)

// captureReadCloser 包装响应体并并发安全地累计诊断副本。
type captureReadCloser struct {
	source    io.ReadCloser
	mu        sync.Mutex
	buffer    bytes.Buffer
	limit     int
	size      int64
	truncated bool
	done      bool
	onChunk   func([]byte)
	onDone    func(captured []byte, size int64, truncated bool, readErr error)
}

// newCaptureReadCloser 创建带分块和完成回调的捕获读取器。
func newCaptureReadCloser(
	source io.ReadCloser,
	limit int,
	onChunk func([]byte),
	onDone func(captured []byte, size int64, truncated bool, readErr error),
) *captureReadCloser {
	return &captureReadCloser{
		source:  source,
		limit:   limit,
		onChunk: onChunk,
		onDone:  onDone,
	}
}

// Read 转发读取结果并保存不超过限制的副本。
func (reader *captureReadCloser) Read(payload []byte) (int, error) {
	read, err := reader.source.Read(payload)
	if read > 0 {
		chunk := payload[:read]
		reader.mu.Lock()
		reader.size += int64(read)
		remaining := reader.limit - reader.buffer.Len()
		if remaining > 0 {
			captured := read
			if captured > remaining {
				captured = remaining
			}
			_, _ = reader.buffer.Write(chunk[:captured])
		}
		if reader.buffer.Len() >= reader.limit && reader.size > int64(reader.buffer.Len()) {
			reader.truncated = true
		}
		reader.mu.Unlock()
		if reader.onChunk != nil {
			reader.onChunk(append([]byte(nil), chunk...))
		}
	}
	if err != nil {
		reader.finish(err)
	}
	return read, err
}

// Close 关闭原始响应体并保证完成回调只执行一次。
func (reader *captureReadCloser) Close() error {
	err := reader.source.Close()
	reader.finish(err)
	return err
}

// finish 固化捕获快照并在锁外调用完成回调。
func (reader *captureReadCloser) finish(readErr error) {
	reader.mu.Lock()
	if reader.done {
		reader.mu.Unlock()
		return
	}
	reader.done = true
	captured := append([]byte(nil), reader.buffer.Bytes()...)
	size := reader.size
	truncated := reader.truncated
	reader.mu.Unlock()
	if reader.onDone != nil {
		reader.onDone(captured, size, truncated, readErr)
	}
}

// rawHex 把捕获字节编码为便于 JSON 持久化的十六进制文本。
func rawHex(payload []byte) string {
	return hex.EncodeToString(payload)
}
