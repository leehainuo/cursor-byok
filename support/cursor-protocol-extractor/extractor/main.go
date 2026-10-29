// main.go 提供协议提取命令的参数解析、输入保护和输出调度。
package main

import (
	"flag"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
)

// inputPaths 支持命令行重复传入 bundle 路径。
type inputPaths []string

// String 返回已经登记的输入路径列表。
func (paths *inputPaths) String() string {
	return fmt.Sprint([]string(*paths))
}

// Set 追加一个去除空白后的输入路径。
func (paths *inputPaths) Set(value string) error {
	*paths = append(*paths, value)
	return nil
}

// bailIf 在不可恢复错误时打印信息并退出。
func bailIf(err error) {
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

// findPrettier 定位可用的 prettier 命令。
func findPrettier() (string, error) {
	// 尝试常见的 prettier 命令名
	names := []string{"prettier", "prettier.cmd", "npx"}
	for _, name := range names {
		if path, err := exec.LookPath(name); err == nil {
			return path, nil
		}
	}
	return "", fmt.Errorf("prettier not found in PATH, please install: npm install -g prettier")
}

// main 解析参数、保护原始输入并执行协议提取。
func main() {
	// 命令行参数
	var inputs inputPaths
	flag.Var(&inputs, "input", "Path to a JS bundle; repeat to merge multiple bundles")
	outputDir := flag.String("output", "", "Output directory for proto files (required; use scripts/extract.sh for the repository source)")
	skipFormat := flag.Bool("skip-format", false, "Skip prettier formatting")
	strict := flag.Bool("strict", true, "Fail when extraction validation detects unresolved/placeholder output")
	flag.Parse()

	// 如果没有 -input 参数，尝试从位置参数获取
	if len(inputs) == 0 && flag.NArg() > 0 {
		inputs = append(inputs, flag.Args()...)
	}

	if len(inputs) == 0 {
		fmt.Fprintln(os.Stderr, "Usage: ext -input <path-to-js-file> [-input <another-js-file>] [-output <dir>] [-skip-format]")
		fmt.Fprintln(os.Stderr, "       ext <path-to-js-file>")
		fmt.Fprintln(os.Stderr, "\nExample:")
		fmt.Fprintln(os.Stderr, "  ext -input /path/to/extensionHostProcess.js")
		fmt.Fprintln(os.Stderr, "  ext C:\\Users\\xxx\\AppData\\Local\\Programs\\cursor\\resources\\app\\out\\vs\\workbench\\api\\node\\extensionHostProcess.js")
		os.Exit(1)
	}

	for _, inputPath := range inputs {
		info, err := os.Stat(inputPath)
		bailIf(err)
		if info.IsDir() {
			bailIf(fmt.Errorf("expected %s to be file, is dir", inputPath))
		}
	}

	// 要求调用者显式选择输出目录，避免在仓库中产生第二份协议来源。
	if *outputDir == "" {
		bailIf(fmt.Errorf("-output is required; use scripts/extract.sh to update protocols/cursor"))
	}

	// 复制到临时文件后再格式化，避免修改 Cursor 安装目录。
	fmt.Printf("Copying %d source bundle(s) to temp directory...\n", len(inputs))
	tempFileNames := make([]string, 0, len(inputs))
	for _, inputPath := range inputs {
		originalFile, err := os.Open(inputPath)
		bailIf(err)
		tempFile, err := os.CreateTemp(os.TempDir(), "cursor-source-*.js")
		bailIf(err)
		_, err = io.Copy(tempFile, originalFile)
		bailIf(err)
		bailIf(originalFile.Close())
		bailIf(tempFile.Close())
		tempFileNames = append(tempFileNames, tempFile.Name())
		fmt.Printf("Source: %s\n", inputPath)
	}

	if *skipFormat {
		fmt.Println("Skipping formatting (--skip-format)")
	} else if prettierBin, err := findPrettier(); err != nil {
		fmt.Printf("Warning: %v\n", err)
		fmt.Println("Skipping formatting, extraction may be less accurate...")
	} else {
		fmt.Println("Formatting source bundles (this may take a while)...")
		for _, tempFileName := range tempFileNames {
			var prettierCmd *exec.Cmd
			if filepath.Base(prettierBin) == "npx" {
				prettierCmd = exec.Command(prettierBin, "prettier", "--write", tempFileName)
			} else {
				prettierCmd = exec.Command(prettierBin, "--write", tempFileName)
			}
			out, formatErr := prettierCmd.CombinedOutput()
			if formatErr != nil {
				fmt.Printf("Prettier output: %s\n", string(out))
				fmt.Println("Warning: formatting failed for one bundle, continuing anyway...")
			}
		}
	}

	// 运行提取器
	fmt.Println("Extracting Proto definitions...")
	SetStrictMode(*strict)
	ExtractProtosFromFiles(tempFileNames, *outputDir)

	for _, tempFileName := range tempFileNames {
		_ = os.Remove(tempFileName)
	}

	fmt.Printf("\nOutput directory: %s\n", *outputDir)
}
