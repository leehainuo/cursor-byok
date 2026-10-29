module github.com/leookun/cursor-proxy-debugger

go 1.25.8

require (
	github.com/andybalholm/brotli v1.2.0
	github.com/leookun/cursor-byok/cursor-proto v0.0.0
	github.com/pkg/browser v0.0.0-20240102092130-5ac0b6a4141c
	google.golang.org/protobuf v1.36.11
	modernc.org/sqlite v1.50.1
)

require (
	github.com/dustin/go-humanize v1.0.1 // indirect
	github.com/google/uuid v1.6.0 // indirect
	github.com/mattn/go-isatty v0.0.20 // indirect
	github.com/ncruces/go-strftime v1.0.0 // indirect
	github.com/remyoudompheng/bigfft v0.0.0-20230129092748-24d4a6f8daec // indirect
	golang.org/x/sys v0.42.0 // indirect
	modernc.org/libc v1.72.3 // indirect
	modernc.org/mathutil v1.7.1 // indirect
	modernc.org/memory v1.11.0 // indirect
)

replace github.com/leookun/cursor-byok/cursor-proto => ../cursor-protocol-extractor
