// types.go 定义协议提取器的领域结构、诊断状态和基础类型映射。
package main

import (
	"fmt"
	"regexp"
	"strings"
)

// isGooglePkg 判断是否为无需重复生成的 Google 标准包。
func isGooglePkg(pkg string) bool {
	return pkg == "google.protobuf" || pkg == "google.rpc"
}

// scalarTypes 把运行时标量编号映射为 proto 类型。
var scalarTypes = map[int]string{
	1:  "double",
	2:  "float",
	3:  "int64",
	4:  "uint64",
	5:  "int32",
	6:  "fixed64",
	7:  "fixed32",
	8:  "bool",
	9:  "string",
	12: "bytes",
	13: "uint32",
	15: "sfixed32",
	16: "sfixed64",
	17: "sint32",
	18: "sint64",
}

// strictExtractionValidation 控制校验失败是否终止提取。
var strictExtractionValidation = true

// extractionDiagnostics 汇总字段解析和类型解析诊断。
type extractionDiagnostics struct {
	totalFieldObjects   int
	parsedFieldObjects  int
	skippedFieldObjects int
	skippedFieldSamples []string
	unresolvedTypeRefs  map[string]int
	emptyMessages       []string
	placeholderHits     []string
	declaredTypes       int
	extractedTypes      int
	missingDeclarations []string
}

// newExtractionDiagnostics 创建一次提取任务的诊断容器。
func newExtractionDiagnostics() *extractionDiagnostics {
	return &extractionDiagnostics{
		unresolvedTypeRefs: make(map[string]int),
	}
}

// addSkippedField 记录未能解析的字段样本和原因。
func (d *extractionDiagnostics) addSkippedField(fieldObject string, reason error) {
	if d == nil {
		return
	}
	d.totalFieldObjects++
	d.skippedFieldObjects++
	if len(d.skippedFieldSamples) < 20 {
		trimmed := strings.TrimSpace(fieldObject)
		if len(trimmed) > 140 {
			trimmed = trimmed[:140] + "..."
		}
		if reason != nil {
			d.skippedFieldSamples = append(d.skippedFieldSamples, fmt.Sprintf("%s | %s", reason.Error(), trimmed))
		} else {
			d.skippedFieldSamples = append(d.skippedFieldSamples, trimmed)
		}
	}
}

// addParsedField 累计成功解析的字段数量。
func (d *extractionDiagnostics) addParsedField() {
	if d == nil {
		return
	}
	d.totalFieldObjects++
	d.parsedFieldObjects++
}

// addUnresolvedType 按引用名称累计类型解析失败次数。
func (d *extractionDiagnostics) addUnresolvedType(ref string) {
	if d == nil {
		return
	}
	key := strings.TrimSpace(ref)
	if key == "" {
		key = "<empty>"
	}
	d.unresolvedTypeRefs[key]++
}

// SetStrictMode 设置校验失败是否终止提取。
func SetStrictMode(enabled bool) {
	strictExtractionValidation = enabled
}

// activeDiagnostics 指向当前提取任务的诊断状态。
var activeDiagnostics *extractionDiagnostics

// 字段解析正则覆盖压缩 bundle 的各类声明形式。
var (
	noRe                    = regexp.MustCompile(`(?:^|[,{]\s*)no:\s*(\d+)`)
	nameRe                  = regexp.MustCompile(`(?:^|[,{]\s*)name:\s*["']([^"']+)["']`)
	kindRe                  = regexp.MustCompile(`(?:^|[,{]\s*)kind:\s*["']([^"']+)["']`)
	enumTypeRe              = regexp.MustCompile(`[,\s]T:\s*[\w$.]+\.getEnumType\s*\(\s*([\w$.]+)\s*\)`)
	tRe                     = regexp.MustCompile(`[,\s]T:\s*([\w$.]+)`)
	oneofRe                 = regexp.MustCompile(`oneof:\s*["']([^"']+)["']`)
	repeatedRe              = regexp.MustCompile(`repeated:\s*(!0|true)`)
	optRe                   = regexp.MustCompile(`opt:\s*(!0|true)`)
	keyRe                   = regexp.MustCompile(`[,\s]K:\s*(\d+)`)
	mapValueRe              = regexp.MustCompile(`V:\s*\{([^}]*)\}`)
	mapValueKRe             = regexp.MustCompile(`(?:^|[,{]\s*)kind:\s*["'](\w+)["']`)
	mapValueTRe             = regexp.MustCompile(`[,\s]T:\s*([\w$.]+)`)
	shorthandTRe            = regexp.MustCompile(`(?:^|[,\{])\s*T\s*(?:[,\}])`)
	oneofNameRe             = regexp.MustCompile(`^[A-Za-z_][A-Za-z0-9_]*$`)
	fieldNameRe             = regexp.MustCompile(`^[A-Za-z_][A-Za-z0-9_]*$`)
	placeholderRe           = regexp.MustCompile(`^\s*(optional\s+|repeated\s+)?[A-Za-z_][A-Za-z0-9_.<>]*\s+(field_\d+|unknown(?:_[A-Za-z0-9_]+)?)\s*=\s*\d+\s*;`)
	varAliasRe              = regexp.MustCompile(`\b(?:let|const|var)\s+([\w$]+)\s*=\s*([\w$]+)\s*(?:[,;])`)
	assignmentAliasRe       = regexp.MustCompile(`(?:^|[;,({])\s*([\w$]+)\s*=\s*([\w$]+)\s*([,;}])`)
	webpackExportBlockRe    = regexp.MustCompile(`[\w$]+\.d\(\s*[\w$]+\s*,\s*\{`)
	webpackExportEntryRe    = regexp.MustCompile(`(?:^|[,\{])\s*([\w$]+)\s*:\s*\(\s*\)\s*=>\s*([\w$]+)`)
	moduleImportRe          = regexp.MustCompile(`(?:\b(?:var|let|const)\s+|,)\s*([\w$]+)\s*=\s*[\w$]+\(\s*(\d+)\s*\)`)
	typeNameDeclarationRe   = regexp.MustCompile(`(?:\bthis|[\w$]+)\.typeName\s*=\s*["']([\w.]+)["']`)
	serviceDeclarationRe    = regexp.MustCompile(`\{\s*typeName\s*:\s*["']([\w.]+)["']\s*,\s*methods\s*:`)
	messageDeclarationRe    = regexp.MustCompile(`\.makeMessageType\s*\(\s*["']([\w.]+)["']`)
	enumDeclarationRe       = regexp.MustCompile(`\.makeEnum\s*\(\s*["']([\w.]+)["']`)
	legacyEnumDeclarationRe = regexp.MustCompile(`\.setEnumType\s*\(\s*[\w$]+\s*,\s*["']([\w.]+)["']`)
	streamCloseRe           = regexp.MustCompile(`(?s)message\s+ExecClientControlMessage\s*\{.*?ExecClientStreamClose\s+stream_close\s*=\s*1\s*;`)
	shellStdoutRe           = regexp.MustCompile(`(?s)message\s+ShellStream\s*\{.*?ShellStreamStdout\s+stdout\s*=\s*1\s*;`)
)

// Field 描述一个待渲染的 protobuf 字段。
type Field struct {
	// No 是字段编号。
	No int `json:"no"`
	// Name 是字段名称。
	Name string `json:"name"`
	// Kind 是标量、消息、枚举或映射类别。
	Kind string `json:"kind"`
	// T 保存标量编号或消息引用变量。
	T any `json:"T"`
	// Oneof 是字段所属的互斥分组。
	Oneof string `json:"oneof"`
	// Repeated 表示字段可以重复。
	Repeated bool `json:"repeated"`
	// Opt 表示字段为显式可选。
	Opt bool `json:"opt"`
	// MapKey 是映射键的标量编号。
	MapKey int `json:"K"`
	// MapValueKind 是映射值的标量或消息类别。
	MapValueKind string
	// MapValueT 保存映射值的标量编号或消息引用。
	MapValueT any
}

// Message 描述提取出的消息及其源码位置。
type Message struct {
	// TypeName 是消息的全限定类型名。
	TypeName string
	// VarName 是 JS 外部变量名。
	VarName string
	// InternalName 是 JS 内部类名。
	InternalName string
	// Fields 是消息字段列表。
	Fields []Field
	// Package 是消息所属协议包。
	Package string
	// ShortName 是包内嵌套类型名。
	ShortName string
	// Pos 是消息在 bundle 中的字节位置。
	Pos int
	// ModuleStart 是消息所在模块的起始位置。
	ModuleStart int
}

// Enum 描述提取出的枚举及其源码位置。
type Enum struct {
	// TypeName 是枚举的全限定类型名。
	TypeName string
	// VarName 是枚举对应的 JS 变量名。
	VarName string
	// Values 是枚举值列表。
	Values []EnumValue
	// Package 是枚举所属协议包。
	Package string
	// ShortName 是包内嵌套类型名。
	ShortName string
	// Pos 是枚举在 bundle 中的字节位置。
	Pos int
	// ModuleStart 是枚举所在模块的起始位置。
	ModuleStart int
}

// EnumValue 描述单个枚举编号和名称。
type EnumValue struct {
	// No 是枚举编号。
	No int
	// Name 是枚举名称。
	Name string
}

// Service 描述提取出的服务及其源码位置。
type Service struct {
	// TypeName 是服务的全限定类型名。
	TypeName string
	// VarName 是服务对应的 JS 变量名。
	VarName string
	// Methods 是服务方法列表。
	Methods []Method
	// Package 是服务所属协议包。
	Package string
	// ShortName 是服务包内名称。
	ShortName string
	// Pos 是服务在 bundle 中的字节位置。
	Pos int
	// ModuleStart 是服务所在模块的起始位置。
	ModuleStart int
}

// Method 描述一个 RPC 方法的输入、输出和流模式。
type Method struct {
	// Name 是 RPC 方法名。
	Name string
	// InputType 是输入消息引用变量。
	InputType string
	// OutputType 是输出消息引用变量。
	OutputType string
	// Kind 是一元或不同方向的流式调用类型。
	Kind string
}

// symbolDef 保存符号对应的类型、类别和模块位置。
type symbolDef struct {
	// TypeName 是符号对应的全限定类型名。
	TypeName string
	// Pos 是符号定义位置。
	Pos int
	// Kind 是消息或枚举类别。
	Kind string
	// ModuleStart 是符号所在模块起点。
	ModuleStart int
}

// TypeResolver 通过局部符号、别名和短名称解析协议类型。
type TypeResolver struct {
	bySymbol      map[string][]symbolDef
	byAlias       map[string][]symbolDef
	byShort       map[string][]symbolDef
	moduleImports map[int]map[string]int
}

// aliasIndex 按模块和目标符号保存别名集合。
type aliasIndex map[int]map[string][]string
