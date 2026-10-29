// services.go 解析枚举、服务方法和压缩对象的配对括号。
package main

import (
	"regexp"
	"strconv"
)

// extractEnums 从旧式和工厂式声明中提取枚举。
func extractEnums(text string, moduleStarts []int) []Enum {
	var enums []Enum
	enumExists := func(typeName, varName string) bool {
		for _, existing := range enums {
			if existing.TypeName == typeName && existing.VarName == varName {
				return true
			}
		}
		return false
	}

	// 匹配任意包中的 setEnumType(XXX, "xxx.v1.EnumName", [...]) 枚举声明。
	// JS 变量名可以包含 $ 符号
	enumRe := regexp.MustCompile(`setEnumType\s*\(\s*([\w$]+)\s*,\s*"([\w.]+)"\s*,\s*\[`)

	matches := enumRe.FindAllStringSubmatchIndex(text, -1)
	for _, match := range matches {
		varName := text[match[2]:match[3]]
		typeName := text[match[4]:match[5]]

		// 提取枚举值数组。
		bracketStart := match[1] - 1
		values := extractEnumValues(text, bracketStart)

		pkg, shortName := parseTypeName(typeName)
		enum := Enum{
			TypeName:    typeName,
			VarName:     varName,
			Values:      values,
			Package:     pkg,
			ShortName:   shortName,
			Pos:         match[0],
			ModuleStart: moduleStartForPos(moduleStarts, match[0]),
		}
		enums = append(enums, enum)
	}

	// 匹配现代 @bufbuild/protobuf 工厂形式，例如 Role=A.makeEnum("aiserver.v1.InferenceMessageRole",[{...}])。
	enumFactoryRe := regexp.MustCompile(`([\w$]+)\s*=\s*[\w$.]+\.makeEnum\s*\(\s*["']([\w.]+)["']\s*,\s*\[`)
	factoryMatches := enumFactoryRe.FindAllStringSubmatchIndex(text, -1)
	for _, match := range factoryMatches {
		varName := text[match[2]:match[3]]
		typeName := text[match[4]:match[5]]
		if enumExists(typeName, varName) {
			continue
		}

		bracketStart := match[1] - 1
		if bracketStart < 0 || bracketStart >= len(text) || text[bracketStart] != '[' {
			continue
		}

		pkg, shortName := parseTypeName(typeName)
		enums = append(enums, Enum{
			TypeName:    typeName,
			VarName:     varName,
			Values:      extractEnumValues(text, bracketStart),
			Package:     pkg,
			ShortName:   shortName,
			Pos:         match[0],
			ModuleStart: moduleStartForPos(moduleStarts, match[0]),
		})
	}

	return enums
}

// extractServices 从命名或匿名描述符中提取服务。
func extractServices(text string, moduleStarts []int) []Service {
	var services []Service
	seenTypeNames := make(map[string]bool)
	appendService := func(varName, typeName string, pos, methodsStart int) {
		if seenTypeNames[typeName] {
			return
		}
		methodsEnd := findMatchingBrace(text, methodsStart)
		if methodsEnd == -1 {
			return
		}

		pkg, shortName := parseTypeName(typeName)
		services = append(services, Service{
			TypeName:    typeName,
			VarName:     varName,
			Methods:     extractMethods(text[methodsStart:methodsEnd]),
			Package:     pkg,
			ShortName:   shortName,
			Pos:         pos,
			ModuleStart: moduleStartForPos(moduleStarts, pos),
		})
		seenTypeNames[typeName] = true
	}

	// 匹配 VarName = { typeName: "xxx.v1.ServiceName", methods: { ... } } 服务对象。
	serviceRe := regexp.MustCompile(`([\w$]+)\s*=\s*\{\s*typeName:\s*"([\w.]+)"\s*,\s*methods:\s*\{`)

	matches := serviceRe.FindAllStringSubmatchIndex(text, -1)
	for _, match := range matches {
		varName := text[match[2]:match[3]]
		typeName := text[match[4]:match[5]]

		appendService(varName, typeName, match[0], match[1]-1)
	}

	// 部分 bundle 把服务描述符直接放入数组，不预先赋给变量。
	anonymousServiceRe := regexp.MustCompile(`\{\s*typeName:\s*["']([\w.]+)["']\s*,\s*methods:\s*\{`)
	for _, match := range anonymousServiceRe.FindAllStringSubmatchIndex(text, -1) {
		typeName := text[match[2]:match[3]]
		appendService("", typeName, match[0], match[1]-1)
	}

	return services
}

// extractMethods 解析服务对象中的 RPC 方法列表。
func extractMethods(methodsText string) []Method {
	var methods []Method

	// 匹配包含方法名、输入、输出和调用类型的方法对象。
	methodRe := regexp.MustCompile(`\w+:\s*\{\s*name:\s*"([^"]+)"\s*,\s*I:\s*([\w$.]+)\s*,\s*O:\s*([\w$.]+)\s*,\s*kind:\s*[\w$.]+\.(Unary|ServerStreaming|ClientStreaming|BiDiStreaming)`)

	matches := methodRe.FindAllStringSubmatch(methodsText, -1)
	for _, m := range matches {
		method := Method{
			Name:       m[1],
			InputType:  m[2],
			OutputType: m[3],
			Kind:       m[4],
		}
		methods = append(methods, method)
	}

	return methods
}

// findMatchingBrace 查找花括号块的结束位置。
func findMatchingBrace(text string, start int) int {
	depth := 0
	for i := start; i < len(text); i++ {
		if text[i] == '{' {
			depth++
		} else if text[i] == '}' {
			depth--
			if depth == 0 {
				return i + 1
			}
		}
	}
	return -1
}

// extractEnumValues 从数组起点解析枚举值。
func extractEnumValues(text string, start int) []EnumValue {
	// 查找数组的配对结束括号。
	depth := 0
	end := start
	for i := start; i < len(text); i++ {
		if text[i] == '[' {
			depth++
		} else if text[i] == ']' {
			depth--
			if depth == 0 {
				end = i + 1
				break
			}
		}
	}

	arrayText := text[start:end]

	var values []EnumValue
	valueRe := regexp.MustCompile(`\{\s*no:\s*(\d+)\s*,\s*name:\s*"([^"]+)"`)

	matches := valueRe.FindAllStringSubmatch(arrayText, -1)
	for _, m := range matches {
		no, _ := strconv.Atoi(m[1])
		values = append(values, EnumValue{No: no, Name: m[2]})
	}

	return values
}
