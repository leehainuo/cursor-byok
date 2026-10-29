// web.go 提供调试器只读 API、SSE 更新流和内嵌静态页面。
package main

import (
	"embed"
	"encoding/json"
	"fmt"
	"io/fs"
	"net/http"
	"strings"
	"time"
)

// webAssets 保存无需外部文件即可启动的调试页面资源。
//
//go:embed web/*
var webAssets embed.FS

// newUIHandler 注册只绑定本机界面的调试 API 和静态资源。
func (server *Server) newUIHandler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /api/status", server.handleStatus)
	mux.HandleFunc("GET /api/exchanges", server.handleExchangeList)
	mux.HandleFunc("GET /api/exchanges/{id}", server.handleExchangeDetail)
	mux.HandleFunc("GET /api/conversations", server.handleConversationList)
	mux.HandleFunc("DELETE /api/exchanges", server.handleClearExchanges)
	mux.HandleFunc("GET /api/events", server.handleEvents)
	assets, _ := fs.Sub(webAssets, "web")
	fileServer := http.FileServer(http.FS(assets))
	mux.Handle("/", fileServer)
	return securityHeaders(mux)
}

// handleStatus 返回监听地址、固定上游和数据库状态。
func (server *Server) handleStatus(writer http.ResponseWriter, _ *http.Request) {
	databasePath, databaseError := server.store.status()
	writeJSON(writer, http.StatusOK, map[string]any{
		"serviceAddr":   server.config.ServiceAddr,
		"debugPath":     debugBasePath + "/",
		"upstreamURL":   server.upstream.String(),
		"running":       true,
		"databasePath":  databasePath,
		"databaseError": databaseError,
	})
}

// handleExchangeList 按可选会话标识列出请求摘要。
func (server *Server) handleExchangeList(writer http.ResponseWriter, request *http.Request) {
	conversationID := strings.TrimSpace(request.URL.Query().Get("conversation_id"))
	summaries, err := server.store.summaries(conversationID)
	if err != nil {
		writeJSON(writer, http.StatusInternalServerError, map[string]string{"error": err.Error()})
		return
	}
	writeJSON(writer, http.StatusOK, summaries)
}

// handleExchangeDetail 返回单条请求的完整捕获详情。
func (server *Server) handleExchangeDetail(writer http.ResponseWriter, request *http.Request) {
	id := strings.TrimSpace(request.PathValue("id"))
	exchange, ok, err := server.store.get(id)
	if err != nil {
		writeJSON(writer, http.StatusInternalServerError, map[string]string{"error": err.Error()})
		return
	}
	if !ok {
		writeJSON(writer, http.StatusNotFound, map[string]string{"error": "请求记录不存在"})
		return
	}
	writeJSON(writer, http.StatusOK, exchange)
}

// handleClearExchanges 清除内存和 SQLite 中的捕获记录。
func (server *Server) handleClearExchanges(writer http.ResponseWriter, _ *http.Request) {
	if err := server.clearExchanges(); err != nil {
		writeJSON(writer, http.StatusInternalServerError, map[string]string{"error": err.Error()})
		return
	}
	writer.WriteHeader(http.StatusNoContent)
}

// handleConversationList 返回持久化流量的会话分组。
func (server *Server) handleConversationList(writer http.ResponseWriter, _ *http.Request) {
	conversations, err := server.store.conversations()
	if err != nil {
		writeJSON(writer, http.StatusInternalServerError, map[string]string{"error": err.Error()})
		return
	}
	writeJSON(writer, http.StatusOK, conversations)
}

// handleEvents 通过 SSE 推送捕获记录变化和保活心跳。
func (server *Server) handleEvents(writer http.ResponseWriter, request *http.Request) {
	flusher, ok := writer.(http.Flusher)
	if !ok {
		http.Error(writer, "当前响应不支持流式刷新", http.StatusInternalServerError)
		return
	}
	writer.Header().Set("Content-Type", "text/event-stream")
	writer.Header().Set("Cache-Control", "no-cache")
	writer.Header().Set("Connection", "keep-alive")
	updates, unsubscribe := server.store.subscribe()
	defer unsubscribe()
	fmt.Fprint(writer, "event: ready\ndata: {}\n\n")
	flusher.Flush()
	heartbeat := time.NewTicker(15 * time.Second)
	defer heartbeat.Stop()
	for {
		select {
		case <-request.Context().Done():
			return
		case event, open := <-updates:
			if !open {
				return
			}
			payload, _ := json.Marshal(event)
			fmt.Fprintf(writer, "event: update\ndata: %s\n\n", payload)
			flusher.Flush()
		case <-heartbeat.C:
			fmt.Fprint(writer, ": heartbeat\n\n")
			flusher.Flush()
		}
	}
}

// writeJSON 写入统一 JSON 响应。
func writeJSON(writer http.ResponseWriter, status int, payload any) {
	writer.Header().Set("Content-Type", "application/json; charset=utf-8")
	writer.WriteHeader(status)
	_ = json.NewEncoder(writer).Encode(payload)
}

// securityHeaders 为本地调试页面添加最小浏览器安全策略。
func securityHeaders(next http.Handler) http.Handler {
	return http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		writer.Header().Set("X-Content-Type-Options", "nosniff")
		writer.Header().Set("Referrer-Policy", "no-referrer")
		writer.Header().Set("Content-Security-Policy", "default-src 'self'; script-src 'self' https://cdn.jsdelivr.net; style-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net; font-src 'self' https://cdn.jsdelivr.net data:; connect-src 'self'; worker-src 'self' blob:")
		next.ServeHTTP(writer, request)
	})
}
