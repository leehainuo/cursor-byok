// extractor_test.go 验证压缩 bundle 的字段、别名、服务和合并提取行为。
package main

import "testing"

// TestParseFieldObjectSupportsShorthandType 验证字段类型简写可以解析。
func TestParseFieldObjectSupportsShorthandType(t *testing.T) {
	field, err := parseFieldObject(`{no:4,name:"file_not_found",kind:"message",T,oneof:"result"}`)
	if err != nil {
		t.Fatalf("parse shorthand T: %v", err)
	}
	if field.T != "T" {
		t.Fatalf("parsed shorthand T as %#v, want T", field.T)
	}
}

// TestWebpackExportAliasResolvesServiceMessageType 验证 Webpack 导出别名可解析服务消息。
func TestWebpackExportAliasResolvesServiceMessageType(t *testing.T) {
	const bundle = `
1:(e,t,n)=>{
  n.d(t,{KS:()=>T,_B:()=>r});
  var r;
  class T {}
  T.typeName="agent.v1.AgentClientMessage";
  n.proto3.util.setEnumType(r,"agent.v1.DiagnosticSeverity",[]);
},
2:(e,t,n)=>{
  var r=n(1);
  const service={typeName:"agent.v1.AgentService",methods:{run:{name:"Run",I:r.KS,O:r.KS,kind:n.MethodKind.BiDiStreaming}}};
}`

	moduleStarts := buildModuleStarts(bundle)
	messages := []Message{{
		TypeName:     "agent.v1.AgentClientMessage",
		VarName:      "T",
		InternalName: "T",
		Package:      "agent.v1",
		Pos:          35,
		ModuleStart:  moduleStartForPos(moduleStarts, 35),
	}}
	enums := []Enum{{
		TypeName:    "agent.v1.DiagnosticSeverity",
		VarName:     "r",
		Package:     "agent.v1",
		Pos:         100,
		ModuleStart: moduleStartForPos(moduleStarts, 100),
	}}

	resolver := newTypeResolver(messages, enums, buildAliasIndex(bundle, moduleStarts), buildWebpackExportAliasIndex(bundle, moduleStarts))
	resolver.moduleImports = buildModuleImportIndex(bundle, moduleStarts)
	typeName, ok := resolver.ResolveTypeName("r.KS", len(bundle)-1, moduleStartForPos(moduleStarts, len(bundle)-1), "agent.v1", "message")
	if !ok {
		t.Fatal("expected webpack export alias to resolve")
	}
	if typeName != "agent.v1.AgentClientMessage" {
		t.Fatalf("resolved r.KS to %q, want agent.v1.AgentClientMessage", typeName)
	}
}

// TestResolverPrefersExpectedKindOverCurrentPackage 验证类型类别优先于当前包候选。
func TestResolverPrefersExpectedKindOverCurrentPackage(t *testing.T) {
	resolver := &TypeResolver{bySymbol: map[string][]symbolDef{
		"nt": {
			{TypeName: "git_forge.v1.GetTagResponse", Kind: "message", Pos: 10, ModuleStart: 1},
			{TypeName: "origin.v1.TeamGroupKind", Kind: "enum", Pos: 20, ModuleStart: 1},
		},
	}}

	typeName, ok := resolver.ResolveTypeName("nt", 30, 1, "origin.v1", "message")
	if !ok {
		t.Fatal("expected cross-package message type to resolve")
	}
	if typeName != "git_forge.v1.GetTagResponse" {
		t.Fatalf("resolved nt to %q, want git_forge.v1.GetTagResponse", typeName)
	}
}

// TestModernFactorySyntaxExtractsInAppAdServiceTypes 验证现代工厂语法提取完整服务类型。
func TestModernFactorySyntaxExtractsInAppAdServiceTypes(t *testing.T) {
	const bundle = `
42:(e,t,n)=>{
  var HasSeenAdRequest=n.makeMessageType("aiserver.v1.HasSeenAdRequest",()=>[{no:1,name:"ad_id",kind:"scalar",T:9}]),
      HasSeenAdResponse=n.makeMessageType("aiserver.v1.HasSeenAdResponse",()=>[{no:1,name:"has_seen",kind:"scalar",T:8}]),
      MarkAdAsSeenResponse=n.makeMessageType("aiserver.v1.MarkAdAsSeenResponse",[]),
      Placement=n.makeEnum("aiserver.v1.InAppAdPlacement",[{no:0,name:"IN_APP_AD_PLACEMENT_UNSPECIFIED",localName:"UNSPECIFIED"}]),
      InAppAdService={typeName:"aiserver.v1.InAppAdService",methods:{hasSeenAd:{name:"HasSeenAd",I:HasSeenAdRequest,O:HasSeenAdResponse,kind:n.MethodKind.Unary},markAdAsSeen:{name:"MarkAdAsSeen",I:HasSeenAdRequest,O:MarkAdAsSeenResponse,kind:n.MethodKind.Unary}}};
}`

	moduleStarts := buildModuleStarts(bundle)
	messages := extractMessages(bundle, moduleStarts)
	enums := extractEnums(bundle, moduleStarts)
	services := extractServices(bundle, moduleStarts)

	if len(messages) != 3 {
		t.Fatalf("extracted %d messages, want 3", len(messages))
	}
	if len(messages[0].Fields) != 1 || messages[0].Fields[0].Name != "ad_id" {
		t.Fatalf("unexpected request fields: %#v", messages[0].Fields)
	}
	if len(enums) != 1 || enums[0].TypeName != "aiserver.v1.InAppAdPlacement" {
		t.Fatalf("unexpected enums: %#v", enums)
	}
	if len(services) != 1 || len(services[0].Methods) != 2 {
		t.Fatalf("unexpected services: %#v", services)
	}

	resolver := newTypeResolver(messages, enums, buildAliasIndex(bundle, moduleStarts), buildWebpackExportAliasIndex(bundle, moduleStarts))
	method := services[0].Methods[0]
	input, inputOK := resolver.ResolveTypeName(method.InputType, services[0].Pos, services[0].ModuleStart, services[0].Package, "message")
	output, outputOK := resolver.ResolveTypeName(method.OutputType, services[0].Pos, services[0].ModuleStart, services[0].Package, "message")
	if !inputOK || input != "aiserver.v1.HasSeenAdRequest" {
		t.Fatalf("resolved input to %q (ok=%v)", input, inputOK)
	}
	if !outputOK || output != "aiserver.v1.HasSeenAdResponse" {
		t.Fatalf("resolved output to %q (ok=%v)", output, outputOK)
	}
}

// TestResolverMapsDottedGoogleWellKnownTypes 验证 r.Empty / r.Value 解析为 google.protobuf 标准类型。
func TestResolverMapsDottedGoogleWellKnownTypes(t *testing.T) {
	resolver := newTypeResolver(nil, nil, nil, nil)
	typeName, ok := resolver.ResolveTypeName("r.Empty", 0, 0, "aiserver.v1", "message")
	if !ok || typeName != "google.protobuf.Empty" {
		t.Fatalf("resolved r.Empty to %q (ok=%v)", typeName, ok)
	}
	typeName, ok = resolver.ResolveTypeName("r.Value", 0, 0, "aiserver.v1", "message")
	if !ok || typeName != "google.protobuf.Value" {
		t.Fatalf("resolved r.Value to %q (ok=%v)", typeName, ok)
	}
}

// TestAssignmentAliasResolvesStandardProtobufType 验证赋值别名解析标准协议类型。
func TestAssignmentAliasResolvesStandardProtobufType(t *testing.T) {
	const bundle = `
1:(e,t,n)=>{
  var Timestamp=class TimestampMessage extends Base{};
  Timestamp.typeName="google.protobuf.Timestamp",Timestamp.fields=n.proto3.util.newFieldList(()=>[]),ua=Timestamp;
  var Request=n.makeMessageType("aiserver.v1.Request",()=>[{no:1,name:"created_at",kind:"message",T:ua}]);
}`

	moduleStarts := buildModuleStarts(bundle)
	messages := extractMessages(bundle, moduleStarts)
	resolver := newTypeResolver(messages, nil, buildAliasIndex(bundle, moduleStarts), nil)

	typeName, ok := resolver.ResolveTypeName("ua", len(bundle)-1, moduleStartForPos(moduleStarts, len(bundle)-1), "aiserver.v1", "message")
	if !ok || typeName != "google.protobuf.Timestamp" {
		t.Fatalf("resolved ua to %q (ok=%v), want google.protobuf.Timestamp", typeName, ok)
	}
}

// TestDeclarationCoverageReportsUnparsedTypesAndIgnoresGoogleTypes 验证覆盖率忽略标准类型并报告遗漏。
func TestDeclarationCoverageReportsUnparsedTypesAndIgnoresGoogleTypes(t *testing.T) {
	const bundle = `
var Request=n.makeMessageType("aiserver.v1.Request",()=>[]);
var Missing=n.makeMessageType("aiserver.v1.Missing",()=>[]);
var Timestamp=n.makeMessageType("google.protobuf.Timestamp",()=>[]);
var Service={typeName:"aiserver.v1.TestService",methods:{}};
`
	messages := []Message{{TypeName: "aiserver.v1.Request"}}
	services := []Service{{TypeName: "aiserver.v1.TestService"}}

	declared, extracted, missing := declarationCoverage(bundle, messages, nil, services)
	if declared != 3 || extracted != 2 {
		t.Fatalf("coverage=%d/%d, want 2/3", extracted, declared)
	}
	if len(missing) != 1 || missing[0] != "aiserver.v1.Missing" {
		t.Fatalf("unexpected missing declarations: %#v", missing)
	}
}

// TestExtractServicesSupportsAnonymousDescriptors 验证匿名服务描述符可以提取。
func TestExtractServicesSupportsAnonymousDescriptors(t *testing.T) {
	const bundle = `services.push({typeName:"aiserver.v1.FileSyncService",methods:{sync:{name:"Sync",I:Request,O:Response,kind:n.MethodKind.Unary}}})`
	services := extractServices(bundle, nil)
	if len(services) != 1 || services[0].TypeName != "aiserver.v1.FileSyncService" {
		t.Fatalf("unexpected services: %#v", services)
	}
	if len(services[0].Methods) != 1 || services[0].Methods[0].Name != "Sync" {
		t.Fatalf("unexpected methods: %#v", services[0].Methods)
	}
}

// TestMergeMessagesPrefersPrimaryBundleAndKeepsSupplementalTypes 验证合并优先主 bundle 并保留补充类型。
func TestMergeMessagesPrefersPrimaryBundleAndKeepsSupplementalTypes(t *testing.T) {
	primary := Message{
		TypeName: "aiserver.v1.Shared",
		Fields:   []Field{{No: 1, Name: "primary", Kind: "scalar", T: 9}},
	}
	supplemental := Message{
		TypeName: "aiserver.v1.Shared",
		Fields:   []Field{{No: 1, Name: "supplemental", Kind: "scalar", T: 9}},
	}
	legacy := Message{TypeName: "aiserver.v1.LegacyOnly"}

	merged := mergeMessagesByTypeName([]Message{primary, supplemental, legacy})
	if len(merged) != 2 {
		t.Fatalf("merged %d messages, want 2", len(merged))
	}
	if len(merged[0].Fields) != 1 {
		t.Fatalf("duplicate type lost a definition: %#v", merged[0])
	}
	if merged[1].TypeName != "aiserver.v1.LegacyOnly" {
		t.Fatalf("supplemental-only type missing: %#v", merged)
	}
}

// TestMergeMessagesPrefersQualifiedTypeReferences 验证合并保留已解析的全限定字段类型。
func TestMergeMessagesPrefersQualifiedTypeReferences(t *testing.T) {
	unresolved := Message{
		TypeName: "aiserver.v1.BuiltinToolResult",
		Fields: []Field{
			{No: 5, Name: "edit_result", Kind: "message", T: "uR", Oneof: "result"},
		},
	}
	qualified := Message{
		TypeName: "aiserver.v1.BuiltinToolResult",
		Fields: []Field{
			{No: 5, Name: "edit_result", Kind: "message", T: "aiserver.v1.EditResult", Oneof: "result"},
		},
	}
	merged := mergeMessagesByTypeName([]Message{unresolved, qualified})
	if merged[0].Fields[0].T != "aiserver.v1.EditResult" {
		t.Fatalf("merge kept unresolved type: %#v", merged[0].Fields[0])
	}
}

// TestMergeMessagesPrefersMoreCompleteDefinition 验证合并保留字段更完整的声明。
func TestMergeMessagesPrefersMoreCompleteDefinition(t *testing.T) {
	subset := Message{
		TypeName: "aiserver.v1.RunInferencePromptModelMetadata",
		Fields:   []Field{{No: 1, Name: "vendor", Kind: "scalar", T: 9}},
	}
	complete := Message{
		TypeName: "aiserver.v1.RunInferencePromptModelMetadata",
		Fields: []Field{
			{No: 1, Name: "vendor", Kind: "scalar", T: 9},
			{No: 46, Name: "agent_mode_configs", Kind: "message", T: "aiserver.v1.RunInferenceAgentModeConfig", Repeated: true},
			{No: 48, Name: "ask_question_config", Kind: "message", T: "aiserver.v1.RunInferenceAskQuestionConfig", Opt: true},
		},
	}

	merged := mergeMessagesByTypeName([]Message{subset, complete})
	if len(merged) != 1 || len(merged[0].Fields) != 3 {
		t.Fatalf("merge did not keep the complete definition: %#v", merged)
	}
}

// TestCompactFactoryExtractsRunInferenceModeConfig 验证 compact $() / enumType 提取 per-mode tool decisions。
func TestCompactFactoryExtractsRunInferenceModeConfig(t *testing.T) {
	const bundle = `
D6t="aiserver.v1.";
class F6t extends Base{static $p(){return D6t}}
K6t=(0,s.QT)(r.proto3,D6t,"InferenceMessageRole",[[0,"UNSPECIFIED"],[1,"USER"],[2,"ASSISTANT"],[3,"TOOL"],[4,"SYSTEM"]],1);
class z7t extends F6t{
  static $(){return["RunInferenceClientMessage|1 run_request #0 message|4 finish_run #1 message",Qp1,lf1]}
}
class vf1 extends F6t{
  static $(){return["RunInferencePromptModelMetadata|1 vendor 9|36 feature_flags 9,#0|46 agent_mode_configs #1*|48 ask_question_config #2?",Tf1,Cf1,wf1]}
}
class Cf1 extends F6t{
  static $(){return["RunInferenceAgentModeConfig|1 mode_id 9|2 enable_ask_question_tool 8|3 switch_mode_tool_config #0?",kf1]}
}
class wf1 extends F6t{
  static $(){return["RunInferenceAskQuestionConfig|1 tool_name 9|2 auto_reject_first 8"]}
}
class Tf1 extends F6t{
  static $(){return["RunInferenceFlagValue|1 bool_value 8 value|2 number_value 1 value|3 string_value 9 value"]}
}
class kf1 extends F6t{
  static $(){return["RunInferenceSwitchModeToolConfig|1 target_modes 9*"]}
}
class Lp1 extends F6t{
  static $(){return["InferenceToolResultPart|1 tool_call_id 9|3 result #0",r.Value]}
}
class hf1 extends F6t{
  static $(){return["RunInferenceRunReady|12 metric_labels 9,9|4 prompt_model_metadata #0",vf1]}
}
InferenceReason=enumType(proto3,__protoPackage117,"InferenceReason",[[0,"UNSPECIFIED"],[1,"GEMINI_VIDEO_SUBAGENT"]],1);
__protoPackage117="aiserver.v1.";
var Pf1={typeName:"aiserver.v1.InferenceService",methods:{runInference:{name:"RunInference",I:z7t,O:hf1,kind:ye.BiDiStreaming},runInferenceSSE:{name:"RunInferenceSSE",I:j$e,O:hf1,kind:ye.ServerStreaming},recordEmpty:{name:"RecordEmpty",I:z7t,O:r.Empty,kind:ye.Unary}}};
`
	moduleStarts := buildModuleStarts(bundle)
	messages := extractMessages(bundle, moduleStarts)
	enums := extractEnums(bundle, moduleStarts)
	reclassifyEnumFields(messages, enums)

	byType := map[string]Message{}
	for _, message := range messages {
		byType[message.TypeName] = message
	}
	meta, ok := byType["aiserver.v1.RunInferencePromptModelMetadata"]
	if !ok {
		t.Fatalf("missing metadata message, got %#v", keysOf(byType))
	}
	field := fieldByName(meta.Fields, "agent_mode_configs")
	if field == nil || !field.Repeated || field.T != "aiserver.v1.RunInferenceAgentModeConfig" {
		t.Fatalf("agent_mode_configs: %#v", field)
	}
	ask := fieldByName(meta.Fields, "ask_question_config")
	if ask == nil || !ask.Opt || ask.T != "aiserver.v1.RunInferenceAskQuestionConfig" {
		t.Fatalf("ask_question_config: %#v", ask)
	}
	mode, ok := byType["aiserver.v1.RunInferenceAgentModeConfig"]
	if !ok {
		t.Fatal("missing RunInferenceAgentModeConfig")
	}
	if fieldByName(mode.Fields, "mode_id") == nil || fieldByName(mode.Fields, "enable_ask_question_tool") == nil {
		t.Fatalf("mode config fields: %#v", mode.Fields)
	}
	result := fieldByName(byType["aiserver.v1.InferenceToolResultPart"].Fields, "result")
	if result == nil || result.T != "google.protobuf.Value" {
		t.Fatalf("result type: %#v", result)
	}
	labels := fieldByName(byType["aiserver.v1.RunInferenceRunReady"].Fields, "metric_labels")
	if labels == nil || labels.Kind != "map" || labels.MapKey != 9 || labels.MapValueKind != "scalar" {
		t.Fatalf("metric_labels: %#v", labels)
	}

	role := enumByName(enums, "aiserver.v1.InferenceMessageRole")
	if role == nil || len(role.Values) != 5 || role.Values[1].Name != "INFERENCE_MESSAGE_ROLE_USER" {
		t.Fatalf("unexpected role enum: %#v", role)
	}
	reason := enumByName(enums, "aiserver.v1.InferenceReason")
	if reason == nil || reason.Values[1].Name != "INFERENCE_REASON_GEMINI_VIDEO_SUBAGENT" {
		t.Fatalf("unexpected reason enum: %#v", reason)
	}

	services := extractServices(bundle, moduleStarts)
	if len(services) != 1 || services[0].TypeName != "aiserver.v1.InferenceService" {
		t.Fatalf("unexpected services: %#v", services)
	}
	resolver := newTypeResolver(messages, enums, nil, nil)
	normalizeTypeReferences(messages, services, resolver)
	if services[0].Methods[2].OutputType != "google.protobuf.Empty" {
		t.Fatalf("RecordEmpty output: %#v", services[0].Methods[2])
	}
}

// TestValidateRequiredAgentShapesAcceptsRunInferenceModeConfig 验证 RunInference mode config 形状校验。
func TestValidateRequiredAgentShapesAcceptsRunInferenceModeConfig(t *testing.T) {
	const body = `
message RunInferenceAgentModeConfig {
  string mode_id = 1;
  bool enable_ask_question_tool = 2;
  optional RunInferenceSwitchModeToolConfig switch_mode_tool_config = 3;
}
message RunInferencePromptModelMetadata {
  repeated RunInferenceAgentModeConfig agent_mode_configs = 46;
  optional RunInferenceAskQuestionConfig ask_question_config = 48;
}
service InferenceService {
  rpc RunInference(stream RunInferenceClientMessage) returns (stream RunInferenceServerMessage) {}
  rpc RunInferenceSSE(BidiRequestId) returns (stream RunInferenceServerMessage) {}
}
`
	if err := validateRequiredAgentShapes("aiserver_v1.proto", body); err != nil {
		t.Fatal(err)
	}
}

func keysOf(messages map[string]Message) []string {
	names := make([]string, 0, len(messages))
	for name := range messages {
		names = append(names, name)
	}
	return names
}

func fieldByName(fields []Field, name string) *Field {
	for i := range fields {
		if fields[i].Name == name {
			return &fields[i]
		}
	}
	return nil
}

func enumByName(enums []Enum, typeName string) *Enum {
	for i := range enums {
		if enums[i].TypeName == typeName {
			return &enums[i]
		}
	}
	return nil
}
