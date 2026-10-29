// renderer.go 把协议声明树渲染为稳定的 proto 文本。
package main

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// generateProtoFile 把单个协议包的声明渲染并写入文件。
func generateProtoFile(pkgName string, messages []Message, enums []Enum, services []Service, resolver *TypeResolver, outputDir string) {
	// 先收集全部跨包标准依赖。
	imports := collectImports(pkgName, messages, services, resolver)

	var sb strings.Builder

	sb.WriteString(`syntax = "proto3";` + "\n\n")
	sb.WriteString(fmt.Sprintf("package %s;\n\n", pkgName))

	// 按稳定顺序写入 import。
	if len(imports) > 0 {
		sortedImports := make([]string, 0, len(imports))
		for imp := range imports {
			sortedImports = append(sortedImports, imp)
		}
		sort.Strings(sortedImports)
		for _, imp := range sortedImports {
			sb.WriteString(fmt.Sprintf("import \"%s\";\n", imp))
		}
		sb.WriteString("\n")
	}

	goPackagePath := strings.ReplaceAll(pkgName, ".", "/")
	goPackageName := strings.ReplaceAll(pkgName, ".", "")
	sb.WriteString(fmt.Sprintf(`option go_package = "github.com/leookun/cursor-byok/cursor-proto/gen/%s;%s";`+"\n\n", goPackagePath, goPackageName))

	// 建立嵌套类型树。
	root := &TypeNode{Children: make(map[string]*TypeNode)}

	for i := range messages {
		msg := &messages[i]
		path := getNestedPath(msg.ShortName)
		insertMessage(root, path, msg)
	}

	for i := range enums {
		enum := &enums[i]
		path := getNestedPath(enum.ShortName)
		insertEnum(root, path, enum)
	}

	// 写入全部顶层类型。
	writeTypeTree(root, &sb, resolver, 0, pkgName)

	// 写入服务声明。
	sort.Slice(services, func(i, j int) bool {
		return services[i].ShortName < services[j].ShortName
	})

	for _, svc := range services {
		// 写入服务来源注释。
		sb.WriteString(fmt.Sprintf("// Source: %s (var: %s)\n", svc.TypeName, svc.VarName))
		sb.WriteString(fmt.Sprintf("service %s {\n", svc.ShortName))
		for _, m := range svc.Methods {
			inputType := resolveMethodType(m.InputType, resolver, pkgName, svc.Pos, svc.ModuleStart)
			outputType := resolveMethodType(m.OutputType, resolver, pkgName, svc.Pos, svc.ModuleStart)

			switch m.Kind {
			case "ServerStreaming":
				sb.WriteString(fmt.Sprintf("  rpc %s(%s) returns (stream %s) {}\n", m.Name, inputType, outputType))
			case "ClientStreaming":
				sb.WriteString(fmt.Sprintf("  rpc %s(stream %s) returns (%s) {}\n", m.Name, inputType, outputType))
			case "BiDiStreaming":
				sb.WriteString(fmt.Sprintf("  rpc %s(stream %s) returns (stream %s) {}\n", m.Name, inputType, outputType))
			default: // 默认为一元调用。
				sb.WriteString(fmt.Sprintf("  rpc %s(%s) returns (%s) {}\n", m.Name, inputType, outputType))
			}
		}
		sb.WriteString("}\n\n")
	}

	// 每个协议包写入扁平输出目录中的单个文件。
	fileName := strings.ReplaceAll(pkgName, ".", "_") + ".proto"
	filePath := filepath.Join(outputDir, fileName)

	os.WriteFile(filePath, []byte(sb.String()), 0644)
	fmt.Printf("Generated: %s (%d messages, %d enums, %d services)\n", filePath, len(messages), len(enums), len(services))
}

// resolveMethodType 解析方法消息类型并处理本地复制类型。
func resolveMethodType(ref string, resolver *TypeResolver, currentPkg string, contextPos int, contextModuleStart int) string {
	typeName, ok := resolver.ResolveTypeName(ref, contextPos, contextModuleStart, currentPkg, "message")
	if !ok {
		activeDiagnostics.addUnresolvedType("method:" + ref)
		return fallbackTypeToken(ref)
	}

	refPkg, shortName := parseTypeName(typeName)
	if refPkg == currentPkg || refPkg == "" {
		return shortName
	}
	// 检查类型是否由其他包复制到当前包。
	if copied := copiedTypes[currentPkg]; copied != nil {
		if _, isCopied := copied[shortName]; isCopied {
			return shortName
		}
	}
	return refPkg + "." + shortName
}

// insertMessage 把消息插入嵌套类型树。
func insertMessage(node *TypeNode, path []string, msg *Message) {
	if len(path) == 0 {
		return
	}

	name := path[0]
	if node.Children == nil {
		node.Children = make(map[string]*TypeNode)
	}

	child, exists := node.Children[name]
	if !exists {
		child = &TypeNode{Name: name, Children: make(map[string]*TypeNode)}
		node.Children[name] = child
	}

	if len(path) == 1 {
		child.Message = msg
	} else {
		insertMessage(child, path[1:], msg)
	}
}

// insertEnum 把枚举插入嵌套类型树。
func insertEnum(node *TypeNode, path []string, enum *Enum) {
	if len(path) == 0 {
		return
	}

	name := path[0]
	if node.Children == nil {
		node.Children = make(map[string]*TypeNode)
	}

	child, exists := node.Children[name]
	if !exists {
		child = &TypeNode{Name: name, Children: make(map[string]*TypeNode)}
		node.Children[name] = child
	}

	if len(path) == 1 {
		child.Enum = enum
	} else {
		insertEnum(child, path[1:], enum)
	}
}

// writeTypeTree 按名称稳定输出嵌套消息和枚举。
func writeTypeTree(node *TypeNode, sb *strings.Builder, resolver *TypeResolver, indent int, currentPkg string) {
	// 对子节点排序以保证输出稳定。
	var names []string
	for name := range node.Children {
		names = append(names, name)
	}
	sort.Strings(names)

	indentStr := strings.Repeat("  ", indent)

	for _, name := range names {
		child := node.Children[name]

		if child.Enum != nil {
			// 检查枚举是否来自其他包。
			originalType := ""
			if copied := copiedTypes[currentPkg]; copied != nil {
				if orig, ok := copied[child.Enum.ShortName]; ok {
					originalType = orig
				}
			}

			// 写入枚举来源注释。
			if originalType != "" {
				sb.WriteString(fmt.Sprintf("%s// Copied from: %s (var: %s)\n", indentStr, originalType, child.Enum.VarName))
			} else {
				sb.WriteString(fmt.Sprintf("%s// Source: %s (var: %s)\n", indentStr, child.Enum.TypeName, child.Enum.VarName))
			}
			// 写入枚举声明。
			sb.WriteString(fmt.Sprintf("%senum %s {\n", indentStr, name))
			for _, v := range child.Enum.Values {
				sb.WriteString(fmt.Sprintf("%s  %s = %d;\n", indentStr, v.Name, v.No))
			}
			sb.WriteString(fmt.Sprintf("%s}\n\n", indentStr))
		} else if child.Message != nil || len(child.Children) > 0 {
			// 写入消息来源注释。
			if child.Message != nil {
				varInfo := child.Message.VarName
				if child.Message.InternalName != "" && child.Message.InternalName != child.Message.VarName {
					varInfo = fmt.Sprintf("%s, class: %s", child.Message.VarName, child.Message.InternalName)
				}

				// 检查消息是否来自其他包。
				originalType := ""
				if copied := copiedTypes[currentPkg]; copied != nil {
					if orig, ok := copied[child.Message.ShortName]; ok {
						originalType = orig
					}
				}

				if originalType != "" {
					sb.WriteString(fmt.Sprintf("%s// Copied from: %s (var: %s)\n", indentStr, originalType, varInfo))
				} else {
					sb.WriteString(fmt.Sprintf("%s// Source: %s (var: %s)\n", indentStr, child.Message.TypeName, varInfo))
				}
			}
			// 即使节点只承载嵌套类型，也要写入消息容器。
			sb.WriteString(fmt.Sprintf("%smessage %s {\n", indentStr, name))

			// 先写入嵌套类型。
			writeTypeTree(child, sb, resolver, indent+1, currentPkg)

			// 当前节点有消息声明时再写字段。
			if child.Message != nil {
				writeMessageFields(child.Message, sb, resolver, indent+1)
			}

			sb.WriteString(fmt.Sprintf("%s}\n\n", indentStr))
		}
	}
}

// writeMessageFields 输出普通字段和 oneof 分组。
func writeMessageFields(msg *Message, sb *strings.Builder, resolver *TypeResolver, indent int) {
	indentStr := strings.Repeat("  ", indent)

	// 获取当前消息路径，用于解析相对嵌套类型。
	msgPath := msg.ShortName
	currentPkg := msg.Package
	preferredPkg, _ := parseTypeName(msg.TypeName)

	// 按 oneof 分组字段。
	oneofGroups := make(map[string][]Field)
	var regularFields []Field

	for _, f := range msg.Fields {
		if f.Oneof != "" {
			oneofGroups[f.Oneof] = append(oneofGroups[f.Oneof], f)
		} else {
			regularFields = append(regularFields, f)
		}
	}

	// 先写普通字段。
	for _, f := range regularFields {
		fieldType := resolveFieldTypeWithPkg(f, resolver, msgPath, currentPkg, preferredPkg, msg.Pos, msg.ModuleStart)
		prefix := ""
		if f.Repeated {
			prefix = "repeated "
		} else if f.Opt {
			prefix = "optional "
		}
		sb.WriteString(fmt.Sprintf("%s%s%s %s = %d;\n", indentStr, prefix, fieldType, f.Name, f.No))
	}

	// 再写 oneof 字段组。
	var oneofNames []string
	for name := range oneofGroups {
		oneofNames = append(oneofNames, name)
	}
	sort.Strings(oneofNames)

	for _, oneofName := range oneofNames {
		fields := oneofGroups[oneofName]
		sb.WriteString(fmt.Sprintf("%soneof %s {\n", indentStr, oneofName))
		for _, f := range fields {
			fieldType := resolveFieldTypeWithPkg(f, resolver, msgPath, currentPkg, preferredPkg, msg.Pos, msg.ModuleStart)
			sb.WriteString(fmt.Sprintf("%s  %s %s = %d;\n", indentStr, fieldType, f.Name, f.No))
		}
		sb.WriteString(fmt.Sprintf("%s}\n", indentStr))
	}
}

// parseTypeName 从全限定类型名拆出协议包和完整嵌套路径。
func parseTypeName(typeName string) (pkg, shortName string) {
	// 优先匹配 xxx.vN.Rest 形式的版本化协议包。
	versionRe := regexp.MustCompile(`^([\w.]+\.v\d+)\.(.+)$`)
	if match := versionRe.FindStringSubmatch(typeName); match != nil {
		return match[1], match[2]
	}

	// 单独处理 google.protobuf 标准类型。
	if strings.HasPrefix(typeName, "google.protobuf.") {
		rest := strings.TrimPrefix(typeName, "google.protobuf.")
		return "google.protobuf", rest
	}

	// 单独处理 google.rpc 标准类型。
	if strings.HasPrefix(typeName, "google.rpc.") {
		rest := strings.TrimPrefix(typeName, "google.rpc.")
		return "google.rpc", rest
	}

	// 无法识别包版本时按最后一个点回退拆分。
	parts := strings.Split(typeName, ".")
	if len(parts) > 1 {
		return strings.Join(parts[:len(parts)-1], "."), parts[len(parts)-1]
	}
	return "", typeName
}

// getNestedPath 把嵌套类型名拆成逐级路径。
func getNestedPath(shortName string) []string {
	return strings.Split(shortName, ".")
}

// resolveFieldTypeWithPkg 结合当前包和父消息路径解析字段类型。
func resolveFieldTypeWithPkg(f Field, resolver *TypeResolver, parentPath string, currentPkg string, preferredPkg string, contextPos int, contextModuleStart int) string {
	resolveNamedType := func(ref string, expectedKind string) string {
		typeName, ok := resolver.ResolveTypeName(ref, contextPos, contextModuleStart, preferredPkg, expectedKind)
		if !ok {
			activeDiagnostics.addUnresolvedType(expectedKind + ":" + ref)
			return fallbackTypeToken(ref)
		}

		refPkg, shortName := parseTypeName(typeName)

		// 类型位于同一父消息下时使用相对路径。
		if parentPath != "" && strings.HasPrefix(shortName, parentPath+".") {
			// 例如消息内部将 ConversationMessage.CodeChunk 缩短为 CodeChunk。
			return strings.TrimPrefix(shortName, parentPath+".")
		}

		// 同包类型只使用短名称。
		if refPkg == currentPkg || refPkg == "" {
			return shortName
		}

		// 循环依赖中优先使用已经复制到当前包的类型。
		if copied := copiedTypes[currentPkg]; copied != nil {
			if _, isCopied := copied[shortName]; isCopied {
				// 本地存在复制类型时使用短名称。
				return shortName
			}
		}

		// 其余跨包引用保留全限定类型名。
		return refPkg + "." + shortName
	}

	if f.Kind == "scalar" {
		if t, ok := f.T.(int); ok {
			return scalarTypes[t]
		}
		if t, ok := f.T.(float64); ok {
			return scalarTypes[int(t)]
		}
	}

	if f.Kind == "message" || f.Kind == "enum" {
		if ref, ok := f.T.(string); ok {
			return resolveNamedType(ref, f.Kind)
		}
	}

	if f.Kind == "map" {
		// map 字段分别解析键和值类型。
		keyType := scalarTypes[f.MapKey]
		if keyType == "" {
			keyType = "string" // 未知标量默认使用字符串。
		}

		var valueType string
		if f.MapValueKind == "scalar" {
			if t, ok := f.MapValueT.(int); ok {
				valueType = scalarTypes[t]
			} else if t, ok := f.MapValueT.(float64); ok {
				valueType = scalarTypes[int(t)]
			}
		} else if f.MapValueKind == "message" || f.MapValueKind == "enum" {
			if ref, ok := f.MapValueT.(string); ok {
				valueType = resolveNamedType(ref, f.MapValueKind)
			}
		}
		if valueType == "" {
			valueType = "bytes"
		}

		return fmt.Sprintf("map<%s, %s>", keyType, valueType)
	}

	return "bytes" // 未识别字段类型时回退为字节串。
}
