import { defineProviderPlugin } from "cursor-byok:plugin";
import { magpieProvider } from "./provider.ts";
import { magpieConnect, presentGateway, RESOURCE_TYPE } from "./resources.ts";

export default defineProviderPlugin({
  providers: [magpieProvider],
  resources: [{
    type: RESOURCE_TYPE,
    displayName: { "en-US": "Gateways", "zh-CN": "网关" },
    add: [magpieConnect],
    present: presentGateway,
  }],
});
