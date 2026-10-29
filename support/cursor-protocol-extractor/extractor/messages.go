// messages.go 解析消息声明、字段数组和字段类型信息。
package main

import (
	"errors"
	"fmt"
	"regexp"
	"strconv"
	"strings"
)

// extractMessages 从多种 bundle 语法中提取消息声明。
func extractMessages(text string, moduleStarts []int) []Message {
	var messages []Message
	messageExists := func(typeName, varName string) bool {
		for _, existing := range messages {
			if existing.TypeName == typeName && existing.VarName == varName {
				return true
			}
		}
		return false
	}

	// 形式一：变量引用继承基类并在类体中声明 typeName 和 fields。
	// 先找所有 "变量名 = class 内部类名" 定义
	// JS 变量名可以包含 $ 符号，如 B$e, qg 等
	// 需要同时捕获外部变量名和内部类名，因为字段引用可能用任一个
	classDefRe := regexp.MustCompile(`([\w$]+)\s*=\s*class\s+([\w$]+)\s+extends\s+[\w$.]+\s*\{`)
	classMatches := classDefRe.FindAllStringSubmatchIndex(text, -1)

	// 从任意包的 this.typeName 字段读取完整类型名。
	typeNameRe := regexp.MustCompile(`this\.typeName\s*=\s*"([\w.]+)"`)

	// 从 this.fields 的 newFieldList 回调读取字段数组。
	fieldsRe := regexp.MustCompile(`this\.fields\s*=\s*\w+(?:\.proto3)?\.util\.newFieldList\s*\(\s*\(\s*\)\s*=>\s*\[`)

	for _, classMatch := range classMatches {
		varName := text[classMatch[2]:classMatch[3]]
		internalName := text[classMatch[4]:classMatch[5]]
		classStart := classMatch[0]

		// 找到类的结束位置（匹配大括号）
		classEnd := findClassEnd(text, classMatch[1]-1)
		if classEnd == -1 {
			continue
		}

		classBody := text[classStart:classEnd]

		// 在类体内查找 typeName
		typeMatch := typeNameRe.FindStringSubmatch(classBody)
		if typeMatch == nil {
			continue
		}
		typeName := typeMatch[1]

		// 在类体内查找 fields
		fieldsMatch := fieldsRe.FindStringIndex(classBody)
		if fieldsMatch == nil {
			continue
		}

		// 找到 fields 数组的开始位置
		bracketPos := classStart + fieldsMatch[1] - 1
		fields := extractFieldArray(text, bracketPos)

		pkg, shortName := parseTypeName(typeName)
		msg := Message{
			TypeName:     typeName,
			VarName:      varName,
			InternalName: internalName,
			Fields:       fields,
			Package:      pkg,
			ShortName:    shortName,
			Pos:          classStart,
			ModuleStart:  moduleStartForPos(moduleStarts, classStart),
		}
		messages = append(messages, msg)
	}

	// 形式二：匹配转译或压缩 bundle 中连续赋值的消息声明。
	// 例如 i.runtime=n.proto3,i.typeName="agent.v1.McpArgs",i.fields=n.proto3.util.newFieldList(()=>[{...}])。
	assignmentRe := regexp.MustCompile(`([\w$]+)\.typeName\s*=\s*"([\w.]+)"\s*,\s*[\w$]+\.fields\s*=\s*\w+(?:\.\w+)*\.util\.newFieldList\s*\(\s*\(\s*\)\s*=>\s*\[`)
	assignmentMatches := assignmentRe.FindAllStringSubmatchIndex(text, -1)
	for _, m := range assignmentMatches {
		varName := text[m[2]:m[3]]
		typeName := text[m[4]:m[5]]

		// 跳过已经由类体形式提取的重复消息。
		if messageExists(typeName, varName) {
			continue
		}

		// 正则停在左方括号之前，从匹配尾部定位数组起点。
		start := m[1] - 1
		if start < 0 || start >= len(text) || text[start] != '[' {
			continue
		}
		fields := extractFieldArray(text, start)

		pkg, shortName := parseTypeName(typeName)
		messages = append(messages, Message{
			TypeName:     typeName,
			VarName:      varName,
			InternalName: "",
			Fields:       fields,
			Package:      pkg,
			ShortName:    shortName,
			Pos:          m[0],
			ModuleStart:  moduleStartForPos(moduleStarts, m[0]),
		})
	}

	// 形式三：匹配现代 @bufbuild/protobuf 工厂调用。
	// 例如 Req=A.makeMessageType("aiserver.v1.HasSeenAdRequest",()=>[{...}])。
	messageFactoryRe := regexp.MustCompile(`([\w$]+)\s*=\s*[\w$.]+\.makeMessageType\s*\(\s*["']([\w.]+)["']\s*,\s*\(\s*\)\s*=>\s*\[`)
	factoryMatches := messageFactoryRe.FindAllStringSubmatchIndex(text, -1)
	for _, m := range factoryMatches {
		varName := text[m[2]:m[3]]
		typeName := text[m[4]:m[5]]
		if messageExists(typeName, varName) {
			continue
		}

		bracketStart := m[1] - 1
		if bracketStart < 0 || bracketStart >= len(text) || text[bracketStart] != '[' {
			continue
		}

		pkg, shortName := parseTypeName(typeName)
		messages = append(messages, Message{
			TypeName:    typeName,
			VarName:     varName,
			Fields:      extractFieldArray(text, bracketStart),
			Package:     pkg,
			ShortName:   shortName,
			Pos:         m[0],
			ModuleStart: moduleStartForPos(moduleStarts, m[0]),
		})
	}

	// 空消息直接传字段数组，不使用延迟回调。
	// 例如 Res=A.makeMessageType("aiserver.v1.MarkAdAsSeenResponse",[])。
	emptyMessageFactoryRe := regexp.MustCompile(`([\w$]+)\s*=\s*[\w$.]+\.makeMessageType\s*\(\s*["']([\w.]+)["']\s*,\s*\[`)
	emptyFactoryMatches := emptyMessageFactoryRe.FindAllStringSubmatchIndex(text, -1)
	for _, m := range emptyFactoryMatches {
		varName := text[m[2]:m[3]]
		typeName := text[m[4]:m[5]]
		if messageExists(typeName, varName) {
			continue
		}

		bracketStart := m[1] - 1
		if bracketStart < 0 || bracketStart >= len(text) || text[bracketStart] != '[' {
			continue
		}

		pkg, shortName := parseTypeName(typeName)
		messages = append(messages, Message{
			TypeName:    typeName,
			VarName:     varName,
			Fields:      extractFieldArray(text, bracketStart),
			Package:     pkg,
			ShortName:   shortName,
			Pos:         m[0],
			ModuleStart: moduleStartForPos(moduleStarts, m[0]),
		})
	}

	return messages
}

// findClassEnd 查找类定义的配对右花括号。
func findClassEnd(text string, openBrace int) int {
	depth := 0
	for i := openBrace; i < len(text); i++ {
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

// extractFieldArray 从左方括号位置解析完整字段数组。
func extractFieldArray(text string, start int) []Field {
	// 查找字段数组的配对右方括号。
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

	// 按每个花括号块解析独立字段对象。
	var fields []Field

	// 依次查找字段对象。
	fieldObjects := extractFieldObjects(arrayText)

	for _, fieldObj := range fieldObjects {
		field, parseErr := parseFieldObject(fieldObj)
		if parseErr != nil {
			activeDiagnostics.addSkippedField(fieldObj, parseErr)
			continue
		}
		activeDiagnostics.addParsedField()
		fields = append(fields, *field)
	}

	return fields
}

// extractFieldObjects 从数组文本中提取独立字段对象。
func extractFieldObjects(arrayText string) []string {
	var objects []string
	depth := 0
	start := -1

	for i := 0; i < len(arrayText); i++ {
		if arrayText[i] == '{' {
			if depth == 0 {
				start = i
			}
			depth++
		} else if arrayText[i] == '}' {
			depth--
			if depth == 0 && start >= 0 {
				objects = append(objects, arrayText[start:i+1])
				start = -1
			}
		}
	}

	return objects
}

// parseFieldObject 解析包含编号、名称、类型和修饰符的单个字段对象。
func parseFieldObject(obj string) (*Field, error) {
	// 提取字段编号。
	noMatch := noRe.FindStringSubmatch(obj)
	if noMatch == nil {
		return nil, errors.New("missing field no")
	}
	no, _ := strconv.Atoi(noMatch[1])

	// 提取字段名称。
	nameMatch := nameRe.FindStringSubmatch(obj)
	if nameMatch == nil {
		return nil, errors.New("missing field name")
	}
	name := strings.TrimSpace(nameMatch[1])
	if !fieldNameRe.MatchString(name) {
		return nil, fmt.Errorf("invalid field name: %s", name)
	}

	// 提取字段类别。
	kindMatch := kindRe.FindStringSubmatch(obj)
	if kindMatch == nil {
		return nil, errors.New("missing field kind")
	}
	kind := strings.TrimSpace(kindMatch[1])

	field := &Field{
		No:   no,
		Name: name,
		Kind: kind,
	}

	// 类型 T 可以是标量编号、变量名或 getEnumType 枚举调用。

	// 枚举优先匹配 getEnumType 调用。
	if enumMatch := enumTypeRe.FindStringSubmatch(obj); enumMatch != nil {
		field.T = enumMatch[1]
	} else {
		// 其余类型匹配普通 T 属性值。
		if tMatch := tRe.FindStringSubmatch(obj); tMatch != nil {
			if t, err := strconv.Atoi(tMatch[1]); err == nil {
				field.T = t
			} else {
				field.T = tMatch[1]
			}
		} else if shorthandTRe.MatchString(obj) {
			field.T = "T"
		}
	}

	// 仅在当前字段对象内检查 oneof 分组。
	if oneofMatch := oneofRe.FindStringSubmatch(obj); oneofMatch != nil {
		candidate := strings.TrimSpace(oneofMatch[1])
		if oneofNameRe.MatchString(candidate) {
			field.Oneof = candidate
		}
	}

	// 仅在当前字段对象内检查 repeated；压缩 JS 中 !0 表示真。
	if repeatedRe.MatchString(obj) {
		field.Repeated = true
	}

	// 仅在当前字段对象内检查 optional。
	if optRe.MatchString(obj) {
		field.Opt = true
	}

	// map 字段通过 K 键类型和 V 值描述共同表示。
	if field.Kind == "map" {
		// 提取 map 键类型。
		if keyMatch := keyRe.FindStringSubmatch(obj); keyMatch != nil {
			field.MapKey, _ = strconv.Atoi(keyMatch[1])
		}

		// 提取 map 值类型，兼容属性顺序变化。
		if valueMatch := mapValueRe.FindStringSubmatch(obj); valueMatch != nil {
			valueObj := valueMatch[1]
			if kindMatch := mapValueKRe.FindStringSubmatch(valueObj); kindMatch != nil {
				field.MapValueKind = kindMatch[1]
			}
			if tMatch := mapValueTRe.FindStringSubmatch(valueObj); tMatch != nil {
				if t, err := strconv.Atoi(tMatch[1]); err == nil {
					field.MapValueT = t
				} else {
					field.MapValueT = tMatch[1]
				}
			}
		}
	}

	return field, nil
}
