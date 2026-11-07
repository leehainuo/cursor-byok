// compact.go 解析 Cursor compact protobuf runtime 的 $() schema 与 enumType/QT 枚举。
package main

import (
	"regexp"
	"strconv"
	"strings"
)

var googleWellKnownTypes = map[string]string{
	"Any":       "google.protobuf.Any",
	"Duration":  "google.protobuf.Duration",
	"Empty":     "google.protobuf.Empty",
	"ListValue": "google.protobuf.ListValue",
	"Struct":    "google.protobuf.Struct",
	"Timestamp": "google.protobuf.Timestamp",
	"Value":     "google.protobuf.Value",
}

type compactClass struct {
	Name       string
	Extends    string
	Start      int
	End        int
	PackageRef string
}

type compactPrefix struct {
	Name    string
	Package string
	Pos     int
}

type compactPendingMessage struct {
	msg      Message
	typeRefs []string
}

// extractCompactMessages 从 static $(){return["Type|fields", TypeRef...]} 提取消息。
func extractCompactMessages(text string, moduleStarts []int) []Message {
	prefixes := extractCompactPrefixes(text)
	classes := extractCompactClasses(text)
	matches := compactSchemaRe.FindAllStringIndex(text, -1)
	if len(matches) == 0 {
		return nil
	}

	pending := make([]compactPendingMessage, 0, len(matches))
	seen := make(map[string]bool)
	for _, match := range matches {
		arrayStart := strings.LastIndex(text[match[0]:match[1]], "[")
		if arrayStart < 0 {
			continue
		}
		start := match[0] + arrayStart
		arrayText := extractBracketSlice(text, start)
		if arrayText == "" {
			continue
		}
		shortName, fields, typeRefs, ok := parseCompactSchemaArray(arrayText)
		if !ok || shortName == "" || googleWellKnownTypes[shortName] != "" {
			continue
		}
		pkg := packageForCompactPos(text, start, classes, prefixes)
		if pkg == "" || isGooglePkg(pkg) {
			continue
		}
		typeName := pkg + "." + shortName
		if seen[typeName] {
			continue
		}
		seen[typeName] = true
		pending = append(pending, compactPendingMessage{
			msg: Message{
				TypeName:    typeName,
				VarName:     compactClassNameAt(start, classes),
				Fields:      fields,
				Package:     pkg,
				ShortName:   shortName,
				Pos:         start,
				ModuleStart: moduleStartForPos(moduleStarts, start),
			},
			typeRefs: typeRefs,
		})
	}

	byVar, byShort := compactTypeIndex(pending, extractCompactEnums(text, moduleStarts))
	messages := make([]Message, 0, len(pending))
	for _, item := range pending {
		qualifyCompactFields(item.msg.Fields, item.typeRefs, item.msg.Package, byVar, byShort)
		messages = append(messages, item.msg)
	}
	return messages
}

func compactTypeIndex(pending []compactPendingMessage, enums []Enum) (map[string]string, map[string]string) {
	byVar := make(map[string]string, len(pending)+len(enums))
	byShort := make(map[string]string, len(pending)+len(enums))
	add := func(varName, shortName, typeName string) {
		if typeName == "" {
			return
		}
		if varName != "" {
			byVar[varName] = typeName
		}
		if shortName != "" {
			byShort[shortName] = typeName
			if idx := strings.LastIndex(shortName, "."); idx >= 0 {
				byShort[shortName[idx+1:]] = typeName
			}
		}
	}
	for _, item := range pending {
		add(item.msg.VarName, item.msg.ShortName, item.msg.TypeName)
	}
	for _, enum := range enums {
		add(enum.VarName, enum.ShortName, enum.TypeName)
	}
	return byVar, byShort
}

// extractCompactEnums 从 enumType(...) 与 (0,s.QT)(...) 提取枚举。
func extractCompactEnums(text string, moduleStarts []int) []Enum {
	prefixes := extractCompactPrefixes(text)
	matches := compactEnumRe.FindAllStringSubmatchIndex(text, -1)
	if len(matches) == 0 {
		return nil
	}

	enums := make([]Enum, 0, len(matches))
	seen := make(map[string]bool)
	for _, match := range matches {
		varName := text[match[2]:match[3]]
		pkgExpr := text[match[4]:match[5]]
		shortName := text[match[6]:match[7]]
		bracketStart := match[1] - 1
		if bracketStart < 0 || bracketStart >= len(text) || text[bracketStart] != '[' {
			continue
		}
		pkg := resolveCompactPackageExpr(pkgExpr, match[0], prefixes)
		if pkg == "" || isGooglePkg(pkg) {
			continue
		}
		typeName := pkg + "." + shortName
		if seen[typeName] {
			continue
		}
		seen[typeName] = true
		values := extractCompactEnumValues(extractBracketSlice(text, bracketStart), shortName)
		enums = append(enums, Enum{
			TypeName:    typeName,
			VarName:     varName,
			Values:      values,
			Package:     pkg,
			ShortName:   shortName,
			Pos:         match[0],
			ModuleStart: moduleStartForPos(moduleStarts, match[0]),
		})
	}
	return enums
}

func submatchText(text string, match []int, group int) string {
	start := group * 2
	end := start + 1
	if end >= len(match) || match[start] < 0 || match[end] < 0 {
		return ""
	}
	return text[match[start]:match[end]]
}

func extractCompactPrefixes(text string) []compactPrefix {
	matches := packagePrefixRe.FindAllStringSubmatchIndex(text, -1)
	prefixes := make([]compactPrefix, 0, len(matches))
	for _, match := range matches {
		prefixes = append(prefixes, compactPrefix{
			Name:    text[match[2]:match[3]],
			Package: strings.TrimSuffix(text[match[4]:match[5]], "."),
			Pos:     match[0],
		})
	}
	return prefixes
}

func extractCompactClasses(text string) []compactClass {
	matches := classDeclRe.FindAllStringSubmatchIndex(text, -1)
	classes := make([]compactClass, 0, len(matches))
	for _, match := range matches {
		name := submatchText(text, match, 1)
		if name == "" {
			name = submatchText(text, match, 2)
		}
		brace := strings.LastIndex(text[match[0]:match[1]], "{")
		if brace < 0 {
			continue
		}
		start := match[0] + brace
		end := findMatchingBrace(text, start)
		if end == -1 {
			continue
		}
		body := text[start:end]
		pkgRef := ""
		if pkgMatch := classPackageReturnRe.FindStringSubmatch(body); pkgMatch != nil {
			pkgRef = pkgMatch[1] + pkgMatch[2] + pkgMatch[3]
		}
		classes = append(classes, compactClass{
			Name:       name,
			Extends:    submatchText(text, match, 4),
			Start:      match[0],
			End:        end,
			PackageRef: pkgRef,
		})
	}
	return classes
}

func packageForCompactPos(_ string, pos int, classes []compactClass, prefixes []compactPrefix) string {
	current := innermostCompactClass(pos, classes)
	seen := make(map[string]bool)
	for current != nil {
		if seen[current.Name] {
			break
		}
		seen[current.Name] = true
		if pkg := resolveCompactPackageExpr(current.PackageRef, current.Start, prefixes); pkg != "" {
			return pkg
		}
		current = compactClassByName(current.Extends, current.Start, classes)
	}
	return ""
}

func innermostCompactClass(pos int, classes []compactClass) *compactClass {
	var best *compactClass
	for i := range classes {
		class := &classes[i]
		if class.Start <= pos && pos < class.End {
			if best == nil || class.Start > best.Start {
				best = class
			}
		}
	}
	return best
}

func compactClassByName(name string, pos int, classes []compactClass) *compactClass {
	name = strings.TrimSpace(name)
	if name == "" {
		return nil
	}
	if idx := strings.LastIndex(name, "."); idx >= 0 {
		name = name[idx+1:]
	}
	var best *compactClass
	bestDistance := -1
	for i := range classes {
		class := &classes[i]
		if class.Name != name {
			continue
		}
		distance := absInt(class.Start - pos)
		if best == nil || distance < bestDistance {
			best = class
			bestDistance = distance
		}
	}
	return best
}

func compactClassNameAt(pos int, classes []compactClass) string {
	if class := innermostCompactClass(pos, classes); class != nil {
		return class.Name
	}
	return ""
}

func resolveCompactPackageExpr(expr string, pos int, prefixes []compactPrefix) string {
	expr = strings.Trim(strings.TrimSpace(expr), `"'`)
	expr = strings.TrimSuffix(expr, ".")
	if expr == "" {
		return ""
	}
	if looksLikePackageName(expr) {
		return expr
	}
	var best *compactPrefix
	bestDistance := -1
	for i := range prefixes {
		prefix := &prefixes[i]
		if prefix.Name != expr {
			continue
		}
		distance := absInt(prefix.Pos - pos)
		if best == nil || distance < bestDistance || (distance == bestDistance && prefix.Pos < best.Pos) {
			best = prefix
			bestDistance = distance
		}
	}
	if best == nil {
		return ""
	}
	return best.Package
}

func looksLikePackageName(value string) bool {
	matched, _ := regexp.MatchString(`^[A-Za-z][\w]*(\.[A-Za-z][\w]*)*\.v\d+$`, value)
	return matched
}

func parseCompactSchemaArray(arrayText string) (string, []Field, []string, bool) {
	body := strings.TrimSpace(strings.TrimSuffix(strings.TrimPrefix(strings.TrimSpace(arrayText), "["), "]"))
	if body == "" {
		return "", nil, nil, false
	}
	schema, rest, ok := splitCompactFirstString(body)
	if !ok {
		return "", nil, nil, false
	}
	schema = strings.ReplaceAll(schema, "\\\n", "")
	schema = strings.ReplaceAll(schema, "\\\r\n", "")
	parts := strings.Split(schema, "|")
	if len(parts) == 0 || strings.TrimSpace(parts[0]) == "" {
		return "", nil, nil, false
	}
	shortName := strings.TrimSpace(parts[0])
	typeRefs := parseCompactTypeRefs(rest)
	fields := make([]Field, 0, len(parts)-1)
	for _, part := range parts[1:] {
		field, err := parseCompactField(strings.TrimSpace(part))
		if err != nil {
			activeDiagnostics.addSkippedField(part, err)
			continue
		}
		activeDiagnostics.addParsedField()
		fields = append(fields, *field)
	}
	return shortName, fields, typeRefs, true
}

func splitCompactFirstString(body string) (string, string, bool) {
	body = strings.TrimSpace(body)
	if body == "" || (body[0] != '"' && body[0] != '\'') {
		return "", "", false
	}
	quote := body[0]
	escaped := false
	for i := 1; i < len(body); i++ {
		ch := body[i]
		if escaped {
			escaped = false
			continue
		}
		if ch == '\\' {
			escaped = true
			continue
		}
		if ch == quote {
			return body[1:i], strings.TrimSpace(strings.TrimPrefix(strings.TrimSpace(body[i+1:]), ",")), true
		}
	}
	return "", "", false
}

func parseCompactTypeRefs(rest string) []string {
	if rest == "" {
		return nil
	}
	var refs []string
	depth := 0
	start := 0
	for i := 0; i <= len(rest); i++ {
		if i < len(rest) {
			switch rest[i] {
			case '[', '(':
				depth++
				continue
			case ']', ')':
				depth--
				continue
			case ',':
				if depth != 0 {
					continue
				}
			default:
				continue
			}
		}
		ref := strings.TrimSpace(rest[start:i])
		if ref != "" {
			refs = append(refs, ref)
		}
		start = i + 1
	}
	return refs
}

func parseCompactField(spec string) (*Field, error) {
	match := compactFieldRe.FindStringSubmatch(spec)
	if match == nil {
		return nil, errCompactField(spec)
	}
	no, _ := strconv.Atoi(match[1])
	name := match[2]
	if !fieldNameRe.MatchString(name) {
		return nil, errCompactField(spec)
	}
	typeMatch := compactTypeRe.FindStringSubmatch(strings.TrimSpace(match[3]))
	if typeMatch == nil {
		return nil, errCompactField(spec)
	}
	field := &Field{
		No:       no,
		Name:     name,
		Repeated: typeMatch[3] == "*",
		Opt:      typeMatch[3] == "?",
		Oneof:    typeMatch[4],
	}
	key := typeMatch[1]
	value := typeMatch[2]
	if value != "" {
		field.Kind = "map"
		if scalar, ok := compactScalar(key); ok {
			field.MapKey = scalar
		} else {
			return nil, errCompactField(spec)
		}
		kind, token := compactTypeToken(value)
		field.MapValueKind = kind
		field.MapValueT = token
		return field, nil
	}
	kind, token := compactTypeToken(key)
	field.Kind = kind
	field.T = token
	return field, nil
}

type compactFieldError string

func (err compactFieldError) Error() string { return string(err) }

func errCompactField(spec string) error {
	return compactFieldError("invalid compact field: " + spec)
}

func compactScalar(token string) (int, bool) {
	value, err := strconv.Atoi(token)
	if err != nil {
		return 0, false
	}
	if _, ok := scalarTypes[value]; !ok {
		return 0, false
	}
	return value, true
}

func compactTypeToken(token string) (string, any) {
	if scalar, ok := compactScalar(token); ok {
		return "scalar", scalar
	}
	return "message", token
}

func qualifyCompactFields(fields []Field, typeRefs []string, pkg string, byVar, byShort map[string]string) {
	qualify := func(token any) any {
		ref, ok := token.(string)
		if !ok || !strings.HasPrefix(ref, "#") {
			return token
		}
		index, err := strconv.Atoi(strings.TrimPrefix(ref, "#"))
		if err != nil || index < 0 || index >= len(typeRefs) {
			return token
		}
		return qualifyCompactTypeRef(typeRefs[index], pkg, byVar, byShort)
	}
	for i := range fields {
		field := &fields[i]
		if field.Kind == "map" {
			field.MapValueT = qualify(field.MapValueT)
			continue
		}
		field.T = qualify(field.T)
	}
}

func qualifyCompactTypeRef(ref, pkg string, byVar, byShort map[string]string) string {
	ref = strings.TrimSpace(ref)
	if ref == "" {
		return ref
	}
	if typeName, ok := byVar[ref]; ok {
		return typeName
	}
	last := ref
	if idx := strings.LastIndex(ref, "."); idx >= 0 {
		last = ref[idx+1:]
	}
	if typeName, ok := byVar[last]; ok {
		return typeName
	}
	if typeName, ok := googleWellKnownTypes[last]; ok {
		return typeName
	}
	if looksLikeFullTypeName(ref) {
		return ref
	}
	if typeName, ok := byShort[last]; ok {
		return typeName
	}
	if pkg == "" {
		return last
	}
	return pkg + "." + last
}

func extractCompactEnumValues(arrayText, shortName string) []EnumValue {
	matches := compactEnumValueRe.FindAllStringSubmatch(arrayText, -1)
	values := make([]EnumValue, 0, len(matches))
	prefix := compactEnumPrefix(shortName)
	seen := make(map[int]bool)
	for _, match := range matches {
		no, _ := strconv.Atoi(match[1])
		if seen[no] {
			continue
		}
		seen[no] = true
		name := match[2]
		if prefix != "" && !strings.HasPrefix(name, prefix) {
			name = prefix + name
		}
		values = append(values, EnumValue{No: no, Name: name})
	}
	return values
}

func compactEnumPrefix(shortName string) string {
	if shortName == "" {
		return ""
	}
	parts := strings.Split(shortName, ".")
	name := parts[len(parts)-1]
	var b strings.Builder
	for i, r := range name {
		if i > 0 && r >= 'A' && r <= 'Z' {
			b.WriteByte('_')
		}
		b.WriteRune(r)
	}
	prefix := strings.ToUpper(b.String()) + "_"
	if strings.HasSuffix(prefix, "_") {
		return prefix
	}
	return prefix
}

func extractBracketSlice(text string, start int) string {
	if start < 0 || start >= len(text) || text[start] != '[' {
		return ""
	}
	depth := 0
	inString := false
	escaped := false
	quote := byte(0)
	for i := start; i < len(text); i++ {
		ch := text[i]
		if inString {
			if escaped {
				escaped = false
				continue
			}
			if ch == '\\' {
				escaped = true
				continue
			}
			if ch == quote {
				inString = false
			}
			continue
		}
		switch ch {
		case '"', '\'':
			inString = true
			quote = ch
		case '[':
			depth++
		case ']':
			depth--
			if depth == 0 {
				return text[start : i+1]
			}
		}
	}
	return ""
}
