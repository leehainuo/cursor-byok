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
	if merged[0].Fields[0].Name != "primary" {
		t.Fatalf("duplicate type did not preserve primary definition: %#v", merged[0])
	}
	if merged[1].TypeName != "aiserver.v1.LegacyOnly" {
		t.Fatalf("supplemental-only type missing: %#v", merged)
	}
}
