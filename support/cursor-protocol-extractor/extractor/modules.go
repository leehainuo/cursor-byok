// modules.go 扫描模块边界、合并声明并执行提取结果校验。
package main

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"github.com/jhump/protoreflect/desc"
	"github.com/jhump/protoreflect/desc/protoparse"
)

// moduleStartRe 匹配 Webpack 数字模块的函数起点。
var moduleStartRe = regexp.MustCompile(`(?:^|,)\s*(\d+)\s*:\s*(?:function\s*\(\s*[\w$,\s]*\s*\)|\(\s*[\w$,\s]*\s*\)\s*=>)\s*\{`)

// buildModuleStarts 收集 bundle 内全部模块起始位置。
func buildModuleStarts(text string) []int {
	matches := moduleStartRe.FindAllStringSubmatchIndex(text, -1)
	starts := make([]int, 0, len(matches))
	for _, match := range matches {
		starts = append(starts, match[0])
	}
	return starts
}

// moduleStartForPos 查找指定源码位置所属的模块起点。
func moduleStartForPos(moduleStarts []int, pos int) int {
	if len(moduleStarts) == 0 {
		return 0
	}
	index := sort.Search(len(moduleStarts), func(i int) bool {
		return moduleStarts[i] > pos
	}) - 1
	if index < 0 {
		return 0
	}
	return moduleStarts[index]
}

// buildModuleImportIndex 建立模块局部变量到导入模块编号的映射。
func buildModuleImportIndex(text string, moduleStarts []int) map[int]map[string]int {
	if len(moduleStarts) == 0 {
		return nil
	}

	moduleMatches := moduleStartRe.FindAllStringSubmatchIndex(text, -1)
	moduleStartByID := make(map[string]int, len(moduleMatches))
	for _, match := range moduleMatches {
		moduleStartByID[text[match[2]:match[3]]] = match[0]
	}

	importsByModule := make(map[int]map[string]int)
	for index, moduleStart := range moduleStarts {
		moduleEnd := len(text)
		if index+1 < len(moduleStarts) {
			moduleEnd = moduleStarts[index+1]
		}
		body := text[moduleStart:moduleEnd]
		for _, match := range moduleImportRe.FindAllStringSubmatch(body, -1) {
			targetModuleStart, ok := moduleStartByID[match[2]]
			if !ok {
				continue
			}
			if importsByModule[moduleStart] == nil {
				importsByModule[moduleStart] = make(map[string]int)
			}
			importsByModule[moduleStart][match[1]] = targetModuleStart
		}
	}
	return importsByModule
}

// ExtractProtosFromFiles 分别提取各 bundle，规范化类型引用后按全限定名合并。
// 多个 bundle 出现同名声明时优先保留靠前输入。
func ExtractProtosFromFiles(inputFiles []string, outputDir string) {
	activeDiagnostics = newExtractionDiagnostics()
	defer func() {
		activeDiagnostics = nil
	}()

	var allMessages []Message
	var allEnums []Enum
	var allServices []Service
	for _, inputFile := range inputFiles {
		content, err := os.ReadFile(inputFile)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error reading file %s: %v\n", inputFile, err)
			os.Exit(1)
		}

		text := string(content)
		moduleStarts := buildModuleStarts(text)
		aliases := buildAliasIndex(text, moduleStarts)
		exportAliases := buildWebpackExportAliasIndex(text, moduleStarts)

		messages := extractMessages(text, moduleStarts)
		enums := extractEnums(text, moduleStarts)
		services := extractServices(text, moduleStarts)
		declared, extracted, missing := declarationCoverage(text, messages, enums, services)
		activeDiagnostics.declaredTypes += declared
		activeDiagnostics.extractedTypes += extracted
		activeDiagnostics.missingDeclarations = append(activeDiagnostics.missingDeclarations, missing...)

		resolver := newTypeResolver(messages, enums, aliases, exportAliases)
		resolver.moduleImports = buildModuleImportIndex(text, moduleStarts)
		normalizeTypeReferences(messages, services, resolver)

		allMessages = append(allMessages, messages...)
		allEnums = append(allEnums, enums...)
		allServices = append(allServices, services...)
	}

	messages := mergeMessagesByTypeName(allMessages)
	enums := mergeEnumsByTypeName(allEnums)
	services := mergeServicesByTypeName(allServices)
	for _, msg := range messages {
		if len(msg.Fields) == 0 {
			activeDiagnostics.emptyMessages = append(activeDiagnostics.emptyMessages, msg.TypeName)
		}
	}
	sort.Strings(activeDiagnostics.missingDeclarations)
	activeDiagnostics.missingDeclarations = compactStrings(activeDiagnostics.missingDeclarations)

	resolver := newTypeResolver(messages, enums, nil, nil)

	generateProtos(messages, enums, services, resolver, outputDir)

	validateErr := validateGeneratedProtos(outputDir, activeDiagnostics)

	printDiagnosticsSummary(activeDiagnostics)

	if strictExtractionValidation && hasValidationFailure(activeDiagnostics, validateErr) {
		if validateErr != nil {
			fmt.Fprintf(os.Stderr, "Validation failed: %v\n", validateErr)
		}
		os.Exit(1)
	}

	if validateErr != nil {
		fmt.Fprintf(os.Stderr, "Validation warning: %v\n", validateErr)
	}

	fmt.Printf("提取完成: %d 个消息, %d 个枚举, %d 个服务\n", len(messages), len(enums), len(services))
}

// normalizeTypeReferences 把字段和方法引用统一转换为全限定类型名。
func normalizeTypeReferences(messages []Message, services []Service, resolver *TypeResolver) {
	resolve := func(ref any, contextPos int, moduleStart int, pkg string, kind string) any {
		symbol, ok := ref.(string)
		if !ok || strings.TrimSpace(symbol) == "" {
			return ref
		}
		if typeName, resolved := resolver.ResolveTypeName(symbol, contextPos, moduleStart, pkg, kind); resolved {
			return typeName
		}
		return ref
	}

	for messageIndex := range messages {
		message := &messages[messageIndex]
		for fieldIndex := range message.Fields {
			field := &message.Fields[fieldIndex]
			if field.Kind == "message" || field.Kind == "enum" {
				field.T = resolve(field.T, message.Pos, message.ModuleStart, message.Package, field.Kind)
			}
			if field.Kind == "map" && (field.MapValueKind == "message" || field.MapValueKind == "enum") {
				field.MapValueT = resolve(field.MapValueT, message.Pos, message.ModuleStart, message.Package, field.MapValueKind)
			}
		}
	}

	for serviceIndex := range services {
		service := &services[serviceIndex]
		for methodIndex := range service.Methods {
			method := &service.Methods[methodIndex]
			if typeName, ok := resolve(method.InputType, service.Pos, service.ModuleStart, service.Package, "message").(string); ok {
				method.InputType = typeName
			}
			if typeName, ok := resolve(method.OutputType, service.Pos, service.ModuleStart, service.Package, "message").(string); ok {
				method.OutputType = typeName
			}
		}
	}
}

// mergeMessagesByTypeName 按全限定名合并消息并保留首次声明。
func mergeMessagesByTypeName(messages []Message) []Message {
	seen := make(map[string]bool)
	merged := make([]Message, 0, len(messages))
	for _, message := range messages {
		if seen[message.TypeName] {
			continue
		}
		seen[message.TypeName] = true
		merged = append(merged, message)
	}
	return merged
}

// mergeEnumsByTypeName 按全限定名合并枚举并保留首次声明。
func mergeEnumsByTypeName(enums []Enum) []Enum {
	seen := make(map[string]bool)
	merged := make([]Enum, 0, len(enums))
	for _, enum := range enums {
		if seen[enum.TypeName] {
			continue
		}
		seen[enum.TypeName] = true
		merged = append(merged, enum)
	}
	return merged
}

// mergeServicesByTypeName 按全限定名合并服务并保留首次声明。
func mergeServicesByTypeName(services []Service) []Service {
	seen := make(map[string]bool)
	merged := make([]Service, 0, len(services))
	for _, service := range services {
		if seen[service.TypeName] {
			continue
		}
		seen[service.TypeName] = true
		merged = append(merged, service)
	}
	return merged
}

// compactStrings 清理、去重并排序诊断字符串。
func compactStrings(values []string) []string {
	if len(values) == 0 {
		return nil
	}
	compacted := values[:1]
	for _, value := range values[1:] {
		if value != compacted[len(compacted)-1] {
			compacted = append(compacted, value)
		}
	}
	return compacted
}

// hasValidationFailure 判断诊断结果是否达到失败条件。
func hasValidationFailure(diag *extractionDiagnostics, validateErr error) bool {
	if validateErr != nil {
		return true
	}
	if diag == nil {
		return false
	}
	if diag.skippedFieldObjects > 0 {
		return true
	}
	if len(diag.unresolvedTypeRefs) > 0 {
		return true
	}
	if len(diag.placeholderHits) > 0 {
		return true
	}
	if len(diag.missingDeclarations) > 0 {
		return true
	}
	return false
}

// printDiagnosticsSummary 输出提取覆盖率和异常样本摘要。
func printDiagnosticsSummary(diag *extractionDiagnostics) {
	if diag == nil {
		return
	}

	fmt.Printf(
		"诊断汇总: fields %d/%d 解析成功, declarations %d/%d 已提取, skipped=%d, unresolved=%d, placeholders=%d, empty_messages=%d\n",
		diag.parsedFieldObjects,
		diag.totalFieldObjects,
		diag.extractedTypes,
		diag.declaredTypes,
		diag.skippedFieldObjects,
		len(diag.unresolvedTypeRefs),
		len(diag.placeholderHits),
		len(diag.emptyMessages),
	)

	if diag.skippedFieldObjects > 0 && len(diag.skippedFieldSamples) > 0 {
		fmt.Println("字段解析失败样例:")
		for _, sample := range diag.skippedFieldSamples {
			fmt.Printf("  - %s\n", sample)
		}
	}

	if len(diag.unresolvedTypeRefs) > 0 {
		keys := make([]string, 0, len(diag.unresolvedTypeRefs))
		for key := range diag.unresolvedTypeRefs {
			keys = append(keys, key)
		}
		sort.Strings(keys)
		fmt.Println("未解析类型引用:")
		for _, key := range keys {
			fmt.Printf("  - %s (%d)\n", key, diag.unresolvedTypeRefs[key])
		}
	}

	if len(diag.placeholderHits) > 0 {
		fmt.Println("占位字段命中:")
		for i, hit := range diag.placeholderHits {
			if i >= 20 {
				fmt.Printf("  - ... and %d more\n", len(diag.placeholderHits)-20)
				break
			}
			fmt.Printf("  - %s\n", hit)
		}
	}

	if len(diag.missingDeclarations) > 0 {
		fmt.Println("未提取的 Proto 声明:")
		for i, typeName := range diag.missingDeclarations {
			if i >= 20 {
				fmt.Printf("  - ... and %d more\n", len(diag.missingDeclarations)-20)
				break
			}
			fmt.Printf("  - %s\n", typeName)
		}
	}
}

// declarationCoverage 比较 bundle 声明数量与实际提取数量。
func declarationCoverage(text string, messages []Message, enums []Enum, services []Service) (int, int, []string) {
	declared := make(map[string]bool)
	collect := func(re *regexp.Regexp) {
		for _, match := range re.FindAllStringSubmatch(text, -1) {
			typeName := strings.TrimSpace(match[1])
			pkg, _ := parseTypeName(typeName)
			if typeName != "" && !isGooglePkg(pkg) {
				declared[typeName] = true
			}
		}
	}
	collect(typeNameDeclarationRe)
	collect(serviceDeclarationRe)
	collect(messageDeclarationRe)
	collect(enumDeclarationRe)
	collect(legacyEnumDeclarationRe)

	extracted := make(map[string]bool)
	for _, message := range messages {
		extracted[message.TypeName] = true
	}
	for _, enum := range enums {
		extracted[enum.TypeName] = true
	}
	for _, service := range services {
		extracted[service.TypeName] = true
	}

	matched := 0
	missing := make([]string, 0)
	for typeName := range declared {
		if extracted[typeName] {
			matched++
			continue
		}
		missing = append(missing, typeName)
	}
	sort.Strings(missing)
	return len(declared), matched, missing
}

// validateGeneratedProtos 检查生成文件语法占位和关键 Agent 结构。
func validateGeneratedProtos(outputDir string, diag *extractionDiagnostics) error {
	entries, err := os.ReadDir(outputDir)
	if err != nil {
		return fmt.Errorf("read output dir failed: %w", err)
	}

	protoFiles := make([]string, 0, len(entries))
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		name := entry.Name()
		if strings.HasSuffix(name, ".proto") {
			protoFiles = append(protoFiles, name)
		}
	}
	if len(protoFiles) == 0 {
		return errors.New("no generated proto files found")
	}
	sort.Strings(protoFiles)

	for _, file := range protoFiles {
		body, readErr := os.ReadFile(filepath.Join(outputDir, file))
		if readErr != nil {
			return fmt.Errorf("read generated proto failed: %s: %w", file, readErr)
		}
		lines := strings.Split(string(body), "\n")
		for idx, line := range lines {
			if placeholderRe.MatchString(line) && diag != nil {
				hit := fmt.Sprintf("%s:%d: %s", file, idx+1, strings.TrimSpace(line))
				diag.placeholderHits = append(diag.placeholderHits, hit)
			}
		}
		if err := validateRequiredAgentShapes(file, string(body)); err != nil {
			return err
		}
	}

	parser := protoparse.Parser{
		ImportPaths:  []string{outputDir},
		LookupImport: desc.LoadFileDescriptor,
	}
	if _, parseErr := parser.ParseFiles(protoFiles...); parseErr != nil {
		return fmt.Errorf("parse generated proto failed: %w", parseErr)
	}

	return nil
}

// validateRequiredAgentShapes 校验 Agent 流控消息的必要字段形状。
func validateRequiredAgentShapes(file string, body string) error {
	if strings.Contains(body, "message ExecClientControlMessage") && !streamCloseRe.MatchString(body) {
		return fmt.Errorf("%s: ExecClientControlMessage.stream_close must be ExecClientStreamClose", file)
	}
	if strings.Contains(body, "message ShellStream") && !shellStdoutRe.MatchString(body) {
		return fmt.Errorf("%s: ShellStream.stdout must be ShellStreamStdout", file)
	}
	return nil
}
