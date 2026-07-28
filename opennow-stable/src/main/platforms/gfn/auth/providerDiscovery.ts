import type { LoginProvider } from "@shared/gfn";

import { GFN_USER_AGENT } from "../clientHeaders";
import { DEFAULT_IDP_ID, SERVICE_URLS_ENDPOINT } from "./constants";

interface ServiceUrlsResponse {
  gfnServiceInfo?: {
    gfnServiceEndpoints?: Array<{
      idpId: string;
      loginProviderCode: string;
      loginProviderDisplayName: string;
      streamingServiceUrl: string;
      loginProviderPriority?: number;
    }>;
  };
}

export function defaultProvider(): LoginProvider {
  return {
    idpId: DEFAULT_IDP_ID,
    code: "NVIDIA",
    displayName: "NVIDIA",
    streamingServiceUrl: "https://prod.cloudmatchbeta.nvidiagrid.net/",
    priority: 0,
  };
}

export function normalizeProvider(provider: LoginProvider): LoginProvider {
  if (!provider.streamingServiceUrl) {
    return provider;
  }
  return {
    ...provider,
    streamingServiceUrl: provider.streamingServiceUrl.endsWith("/")
      ? provider.streamingServiceUrl
      : `${provider.streamingServiceUrl}/`,
  };
}

export class ProviderDiscovery {
  private providers: LoginProvider[] = [];

  async getProviders(): Promise<LoginProvider[]> {
    if (this.providers.length > 0) {
      return this.providers;
    }

    const chizuiMockProvider: LoginProvider = {
      idpId: "chizui",
      code: "CHIZUI",
      displayName: "Chizui Login",
      streamingServiceUrl: "",
      priority: 999,
    };

    let response: Response;
    try {
      response = await fetch(SERVICE_URLS_ENDPOINT, {
        headers: {
          Accept: "application/json",
          "User-Agent": GFN_USER_AGENT,
        },
      });
    } catch (error) {
      console.warn("Failed to fetch providers, using default:", error);
      this.providers = [defaultProvider(), chizuiMockProvider];
      return this.providers;
    }

    if (!response.ok) {
      console.warn(`Providers fetch failed with status ${response.status}, using default`);
      this.providers = [defaultProvider(), chizuiMockProvider];
      return this.providers;
    }

    try {
      const payload = (await response.json()) as ServiceUrlsResponse;
      const endpoints = payload.gfnServiceInfo?.gfnServiceEndpoints ?? [];
      const providers = endpoints
        .map<LoginProvider>((entry) => ({
          idpId: entry.idpId,
          code: entry.loginProviderCode,
          displayName:
            entry.loginProviderCode === "BPC" ? "bro.game" : entry.loginProviderDisplayName,
          streamingServiceUrl: entry.streamingServiceUrl,
          priority: entry.loginProviderPriority ?? 0,
        }))
        .sort((a, b) => a.priority - b.priority)
        .map(normalizeProvider);

      providers.push(chizuiMockProvider);
      this.providers = providers;
      console.log(`Loaded ${this.providers.length} providers`);
      return this.providers;
    } catch (error) {
      console.warn("Failed to parse providers response, using default:", error);
      this.providers = [defaultProvider(), chizuiMockProvider];
      return this.providers;
    }
  }

  async selectProvider(
    selectedProvider: LoginProvider,
    providerIdpId?: string,
  ): Promise<LoginProvider> {
    const providers = await this.getProviders();
    const selected =
      providers.find((provider) => provider.idpId === providerIdpId) ??
      selectedProvider ??
      providers[0] ??
      defaultProvider();
    return normalizeProvider(selected);
  }
}
