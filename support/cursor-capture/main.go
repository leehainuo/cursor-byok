// cursor-proxy-debugger 提供独立 Cursor API 调试服务的进程入口。
package main

import (
	"flag"
	"fmt"
	"log"
	"os"
	"os/signal"
	"syscall"

	"github.com/pkg/browser"
)

// main 解析启动参数，并管理调试服务的完整生命周期。
func main() {
	config := Config{}
	openBrowser := true
	flag.StringVar(&config.ServiceAddr, "addr", defaultServiceAddr, "Cursor API 调试服务监听地址")
	flag.IntVar(&config.MaxExchanges, "max-exchanges", 200, "内存中保留的最大请求数")
	flag.StringVar(&config.DatabasePath, "db", "", "SQLite 数据库路径（默认使用用户配置目录）")
	flag.BoolVar(&openBrowser, "open", true, "启动后打开浏览器")
	flag.Parse()

	server, err := New(config)
	if err != nil {
		log.Fatal(err)
	}
	if err := server.Start(); err != nil {
		log.Fatal(err)
	}

	fmt.Printf("Cursor API 调试服务已启动\n")
	fmt.Printf("服务地址: http://%s\n", server.ServiceAddr())
	fmt.Printf("固定上游: %s\n", defaultUpstreamURL)
	fmt.Printf("调试界面: %s\n", server.UIURL())
	fmt.Printf("SQLite: %s\n", server.DatabasePath())
	if openBrowser {
		_ = browser.OpenURL(server.UIURL())
	}

	signals := make(chan os.Signal, 1)
	signal.Notify(signals, syscall.SIGINT, syscall.SIGTERM)
	<-signals
	signal.Stop(signals)

	if err := server.Close(); err != nil {
		log.Printf("关闭调试服务失败：%v", err)
	}
}
