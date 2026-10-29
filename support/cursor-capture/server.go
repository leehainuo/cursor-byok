// server.go 负责固定上游服务、流量捕获和调试界面的生命周期。
package main

import (
	"errors"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"net/http/httputil"
	"net/url"
	"sync"
	"sync/atomic"
	"time"
)

// Server 运行 Cursor API 转发服务及其本机调试界面。
type Server struct {
	config        Config
	upstream      *url.URL
	store         *exchangeStore
	counter       atomic.Uint64
	serviceServer *http.Server
	serviceLn     net.Listener
	runMu         sync.Mutex
	captureMu     sync.RWMutex
}

// New 创建固定转发到 Cursor API 的协议调试服务。
func New(config Config) (*Server, error) {
	config = config.normalized()
	if err := validateLoopbackAddress(config.ServiceAddr); err != nil {
		return nil, fmt.Errorf("服务监听地址无效：%w", err)
	}
	upstream, err := url.Parse(defaultUpstreamURL)
	if err != nil {
		return nil, fmt.Errorf("解析固定上游地址：%w", err)
	}
	store, err := newPersistentExchangeStore(config.DatabasePath, config.MaxExchanges)
	if err != nil {
		return nil, err
	}
	server := &Server{
		config:   config,
		upstream: upstream,
		store:    store,
	}
	server.counter.Store(store.maxNumericID())
	server.serviceServer = &http.Server{
		Handler:  server.newServiceHandler(),
		ErrorLog: log.New(io.Discard, "", 0),
	}
	return server, nil
}

// Start 启动同时承载 API 转发和调试界面的单端口服务。
func (server *Server) Start() error {
	server.runMu.Lock()
	defer server.runMu.Unlock()
	if server.serviceLn != nil {
		return errors.New("Cursor API 调试服务已经启动")
	}
	serviceListener, err := net.Listen("tcp", server.config.ServiceAddr)
	if err != nil {
		return fmt.Errorf("启动 API 服务监听失败：%w", err)
	}
	server.serviceLn = serviceListener
	go func() { _ = server.serviceServer.Serve(serviceListener) }()
	return nil
}

// Close 立即关闭监听器、活跃连接并释放捕获存储。
func (server *Server) Close() error {
	server.runMu.Lock()
	serviceServer := server.serviceServer
	server.serviceLn = nil
	server.runMu.Unlock()
	var errorsList []error
	if serviceServer != nil {
		if err := serviceServer.Close(); err != nil && !errors.Is(err, http.ErrServerClosed) {
			errorsList = append(errorsList, err)
		}
	}
	if server.store != nil {
		if err := server.store.close(); err != nil {
			errorsList = append(errorsList, err)
		}
	}
	return errors.Join(errorsList...)
}

// ServiceAddr 返回 Cursor API 服务监听地址。
func (server *Server) ServiceAddr() string { return server.config.ServiceAddr }

// UIURL 返回可在浏览器中打开的调试界面地址。
func (server *Server) UIURL() string {
	return "http://" + browserAddress(server.config.ServiceAddr) + debugBasePath + "/"
}

// DatabasePath 返回捕获数据库路径。
func (server *Server) DatabasePath() string {
	return server.config.DatabasePath
}

// newServiceHandler 创建单端口调试路由和固定上游流式转发。
func (server *Server) newServiceHandler() http.Handler {
	reverseProxy := httputil.NewSingleHostReverseProxy(server.upstream)
	reverseProxy.FlushInterval = -1
	reverseProxy.ErrorLog = log.New(io.Discard, "", 0)
	originalDirector := reverseProxy.Director
	reverseProxy.Director = func(request *http.Request) {
		originalDirector(request)
		request.Host = server.upstream.Host
		request.Header["X-Forwarded-For"] = nil
	}
	reverseProxy.Transport = &http.Transport{
		Proxy:                 nil,
		DialContext:           (&net.Dialer{Timeout: 10 * time.Second, KeepAlive: 30 * time.Second}).DialContext,
		ForceAttemptHTTP2:     true,
		DisableCompression:    true,
		MaxIdleConns:          200,
		MaxIdleConnsPerHost:   32,
		IdleConnTimeout:       90 * time.Second,
		TLSHandshakeTimeout:   10 * time.Second,
		ExpectContinueTimeout: 1 * time.Second,
	}
	reverseProxy.ModifyResponse = server.captureResponse
	reverseProxy.ErrorHandler = func(writer http.ResponseWriter, request *http.Request, upstreamErr error) {
		server.failExchange(request, upstreamErr)
		http.Error(writer, "Cursor API upstream unavailable", http.StatusBadGateway)
	}
	forwardHandler := http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		reverseProxy.ServeHTTP(writer, server.captureRequest(request))
	})
	debugHandler := http.StripPrefix(debugBasePath, server.newUIHandler())
	mux := http.NewServeMux()
	mux.Handle(debugBasePath+"/", debugHandler)
	mux.HandleFunc(debugBasePath, func(writer http.ResponseWriter, request *http.Request) {
		http.Redirect(writer, request, debugBasePath+"/", http.StatusTemporaryRedirect)
	})
	mux.Handle("/", forwardHandler)
	return mux
}
