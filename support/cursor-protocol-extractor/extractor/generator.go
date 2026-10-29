// generator.go 计算跨包依赖并为各协议包准备完整声明集合。
package main

import (
	"fmt"
	"os"
)

// generateProtos 按协议包聚合声明并生成对应文件。
func generateProtos(messages []Message, enums []Enum, services []Service, resolver *TypeResolver, outputDir string) {
	os.MkdirAll(outputDir, 0755)

	// 按协议包聚合声明。
	packages := make(map[string]struct {
		messages []Message
		enums    []Enum
		services []Service
	})

	for _, msg := range messages {
		pkg := packages[msg.Package]
		pkg.messages = append(pkg.messages, msg)
		packages[msg.Package] = pkg
	}

	for _, enum := range enums {
		pkg := packages[enum.Package]
		pkg.enums = append(pkg.enums, enum)
		packages[enum.Package] = pkg
	}

	for _, svc := range services {
		pkg := packages[svc.Package]
		pkg.services = append(pkg.services, svc)
		packages[svc.Package] = pkg
	}

	// 建立跨包复制使用的全局类型索引。
	allMessages := make(map[string]*Message)
	allEnums := make(map[string]*Enum)

	for pkgName, pkg := range packages {
		if isGooglePkg(pkgName) {
			continue
		}
		for i := range pkg.messages {
			msg := &pkg.messages[i]
			allMessages[msg.TypeName] = msg
		}
		for i := range pkg.enums {
			enum := &pkg.enums[i]
			allEnums[enum.TypeName] = enum
		}
	}

	// 每轮生成前重置已复制类型索引。
	copiedTypes = make(map[string]map[string]string)

	for pkgName, pkg := range packages {
		// Google 标准包直接使用官方协议文件。
		if isGooglePkg(pkgName) {
			fmt.Printf("跳过: %s (使用官方 proto 文件)\n", pkgName)
			continue
		}

		// 把当前包引用的外部类型复制到本地。
		augmentedPkg := copyAllExternalTypes(pkgName, pkg, resolver, allMessages, allEnums)
		generateProtoFile(pkgName, augmentedPkg.messages, augmentedPkg.enums, pkg.services, resolver, outputDir)
	}
}

// copyAllExternalTypes 递归复制当前包引用的全部外部类型。
func copyAllExternalTypes(pkgName string, pkg struct {
	messages []Message
	enums    []Enum
	services []Service
}, resolver *TypeResolver, allMessages map[string]*Message, allEnums map[string]*Enum) struct {
	messages []Message
	enums    []Enum
	services []Service
} {
	if copiedTypes[pkgName] == nil {
		copiedTypes[pkgName] = make(map[string]string)
	}

	// 建立当前包已有类型集合，并登记本地名称供字段解析使用。
	localTypes := make(map[string]bool)
	for _, msg := range pkg.messages {
		localTypes[msg.ShortName] = true
		// 空来源名表示该类型原本就在当前包。
		if copiedTypes[pkgName][msg.ShortName] == "" {
			copiedTypes[pkgName][msg.ShortName] = "local:" + msg.TypeName
		}
	}
	for _, enum := range pkg.enums {
		localTypes[enum.ShortName] = true
		if copiedTypes[pkgName][enum.ShortName] == "" {
			copiedTypes[pkgName][enum.ShortName] = "local:" + enum.TypeName
		}
	}

	// 结果先保留当前包原始声明。
	result := struct {
		messages []Message
		enums    []Enum
		services []Service
	}{
		messages: append([]Message{}, pkg.messages...),
		enums:    append([]Enum{}, pkg.enums...),
		services: pkg.services,
	}

	totalCopied := 0

	// 持续迭代，直到不再发现新的外部依赖。
	for round := 1; ; round++ {
		// 收集当前消息中的外部类型引用。
		neededTypes := make(map[string]bool)

		for _, msg := range result.messages {
			preferredPkg, _ := parseTypeName(msg.TypeName)
			for _, f := range msg.Fields {
				collectFieldRefsSimple(f, pkgName, preferredPkg, msg.Pos, msg.ModuleStart, resolver, neededTypes, localTypes)
			}
		}
		for _, svc := range result.services {
			for _, m := range svc.Methods {
				collectMethodRefsSimple(m.InputType, pkgName, svc.Pos, svc.ModuleStart, resolver, neededTypes, localTypes)
				collectMethodRefsSimple(m.OutputType, pkgName, svc.Pos, svc.ModuleStart, resolver, neededTypes, localTypes)
			}
		}

		// 复制本轮新增依赖类型。
		copiedThisRound := 0
		for typeName := range neededTypes {
			refPkg, shortName := parseTypeName(typeName)
			if refPkg == pkgName || isGooglePkg(refPkg) {
				continue
			}

			// 已存在于本地时无需重复复制。
			if localTypes[shortName] {
				continue
			}

			// 复制消息声明。
			if msg, ok := allMessages[typeName]; ok {
				msgCopy := *msg
				msgCopy.Package = pkgName
				// 保留原始完整类型名，用于生成来源注释。
				result.messages = append(result.messages, msgCopy)
				copiedTypes[pkgName][shortName] = typeName // 保存原始完整类型名。
				localTypes[shortName] = true
				copiedThisRound++
				fmt.Printf("  [%s] 轮%d 复制: %s\n", pkgName, round, typeName)
			} else if enum, ok := allEnums[typeName]; ok {
				// 复制枚举声明。
				enumCopy := *enum
				enumCopy.Package = pkgName
				result.enums = append(result.enums, enumCopy)
				copiedTypes[pkgName][shortName] = typeName
				localTypes[shortName] = true
				copiedThisRound++
				fmt.Printf("  [%s] 轮%d 复制枚举: %s\n", pkgName, round, typeName)
			} else {
				// 未找到声明时仍登记本地引用，兼容提取结果缺少但 bundle 实际存在的类型。
				copiedTypes[pkgName][shortName] = typeName
				localTypes[shortName] = true
				fmt.Printf("  [%s] 轮%d 警告: 类型未找到 %s，标记为本地引用\n", pkgName, round, typeName)
			}
		}

		totalCopied += copiedThisRound

		if copiedThisRound == 0 {
			break // 没有新增依赖时结束迭代。
		}

		if round > 20 {
			fmt.Printf("  [%s] 警告: 复制轮次超过20，可能存在问题\n", pkgName)
			break
		}
	}

	if totalCopied > 0 {
		fmt.Printf("  [%s] 共复制 %d 个外部类型\n", pkgName, totalCopied)
	}

	return result
}

// collectFieldRefsSimple 收集单个字段直接引用的外部类型。
func collectFieldRefsSimple(f Field, currentPkg string, preferredPkg string, contextPos int, contextModuleStart int, resolver *TypeResolver,
	neededTypes map[string]bool, localTypes map[string]bool) {

	type refWithKind struct {
		ref  string
		kind string
	}

	var refs []refWithKind
	if f.Kind == "message" || f.Kind == "enum" {
		if v, ok := f.T.(string); ok {
			refs = append(refs, refWithKind{ref: v, kind: f.Kind})
		}
	}
	if f.Kind == "map" && (f.MapValueKind == "message" || f.MapValueKind == "enum") {
		if v, ok := f.MapValueT.(string); ok {
			refs = append(refs, refWithKind{ref: v, kind: f.MapValueKind})
		}
	}

	for _, item := range refs {
		typeName, ok := resolver.ResolveTypeName(item.ref, contextPos, contextModuleStart, preferredPkg, item.kind)
		if !ok {
			continue
		}

		refPkg, shortName := parseTypeName(typeName)
		if refPkg == "" || refPkg == currentPkg || isGooglePkg(refPkg) {
			continue
		}

		// 已在当前包中的类型无需收集。
		if localTypes[shortName] {
			continue
		}

		neededTypes[typeName] = true
	}
}

// collectMethodRefsSimple 收集服务方法输入或输出引用的外部类型。
func collectMethodRefsSimple(ref string, currentPkg string, contextPos int, contextModuleStart int, resolver *TypeResolver,
	neededTypes map[string]bool, localTypes map[string]bool) {

	typeName, ok := resolver.ResolveTypeName(ref, contextPos, contextModuleStart, currentPkg, "message")
	if !ok {
		return
	}

	refPkg, shortName := parseTypeName(typeName)
	if refPkg == "" || refPkg == currentPkg || isGooglePkg(refPkg) {
		return
	}

	if localTypes[shortName] {
		return
	}

	neededTypes[typeName] = true
}

// copiedTypes 按目标包和短名称记录被复制类型的原始全限定名。
var copiedTypes = make(map[string]map[string]string)

// TypeNode 表示嵌套消息与枚举组成的类型树节点。
type TypeNode struct {
	// Name 是当前嵌套层级的类型名。
	Name string
	// Message 保存当前节点的消息声明。
	Message *Message
	// Enum 保存当前节点的枚举声明。
	Enum *Enum
	// Children 保存下一层嵌套类型。
	Children map[string]*TypeNode
}

// collectImports 只收集 Google 标准依赖，其余类型会复制到本地。
func collectImports(currentPkg string, messages []Message, services []Service, resolver *TypeResolver) map[string]bool {
	imports := make(map[string]bool)

	addImport := func(ref string, contextPos int, contextModuleStart int, expectedKind string) {
		typeName, ok := resolver.ResolveTypeName(ref, contextPos, contextModuleStart, currentPkg, expectedKind)
		if !ok {
			return
		}

		refPkg, shortName := parseTypeName(typeName)
		// 仅导入 Google 标准类型。
		if refPkg == "google.protobuf" {
			var importFile string
			switch shortName {
			case "Struct", "Value", "ListValue", "NullValue":
				importFile = "google/protobuf/struct.proto"
			case "Timestamp":
				importFile = "google/protobuf/timestamp.proto"
			case "Duration":
				importFile = "google/protobuf/duration.proto"
			case "Any":
				importFile = "google/protobuf/any.proto"
			case "Empty":
				importFile = "google/protobuf/empty.proto"
			case "FieldMask":
				importFile = "google/protobuf/field_mask.proto"
			case "BoolValue", "BytesValue", "DoubleValue", "FloatValue",
				"Int32Value", "Int64Value", "StringValue", "UInt32Value", "UInt64Value":
				importFile = "google/protobuf/wrappers.proto"
			default:
				importFile = "google/protobuf/descriptor.proto"
			}
			imports[importFile] = true
		} else if refPkg == "google.rpc" {
			var importFile string
			switch shortName {
			case "Status":
				importFile = "google/rpc/status.proto"
			case "Code":
				importFile = "google/rpc/code.proto"
			default:
				importFile = "google/rpc/status.proto"
			}
			imports[importFile] = true
		}
	}

	for _, msg := range messages {
		for _, f := range msg.Fields {
			if f.Kind == "message" || f.Kind == "enum" {
				if ref, ok := f.T.(string); ok {
					addImport(ref, msg.Pos, msg.ModuleStart, f.Kind)
				}
			}
			// map 值类型也可能引用标准包。
			if f.Kind == "map" && (f.MapValueKind == "message" || f.MapValueKind == "enum") {
				if ref, ok := f.MapValueT.(string); ok {
					addImport(ref, msg.Pos, msg.ModuleStart, f.MapValueKind)
				}
			}
		}
	}

	for _, svc := range services {
		for _, m := range svc.Methods {
			addImport(m.InputType, svc.Pos, svc.ModuleStart, "message")
			addImport(m.OutputType, svc.Pos, svc.ModuleStart, "message")
		}
	}

	return imports
}
