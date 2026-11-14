import type { JsonValue, PluginContext } from "./plugin.ts";
import type { ResourceSnapshot } from "./resource.ts";

export type ModelCapabilities = {
  images?: boolean;
};

export type ModelDefinition = {
  id: string;
  displayName: string;
  description?: string;
  maxOutputTokens?: number;
  capabilities?: ModelCapabilities;
  /** 之后的调用原样传回;永远不会展示给用户。 */
  privateData?: JsonValue;
};

/** 宿主目录中持久化的一条模型。 */
export type ModelSnapshot = ModelDefinition;

export type ModelListInput = {
  /** 模型发现需要认证时为首个可用资源,否则为 null。 */
  resource: ResourceSnapshot | null;
};

export type ModelCatalogMode = "snapshot" | "live";

export type ModelSupport = {
  /**
   * snapshot(缺省):列举成功后宿主把目录写入 `models-*.json`。
   * live:目录不落盘,宿主按资源短缓存 `list` 的返回值。
   */
  catalog?: ModelCatalogMode;
  /** snapshot 成功后整体替换落盘目录;live 只更新该资源的内存缓存。 */
  list(input: ModelListInput, context: PluginContext): Promise<ModelDefinition[]>;
};
