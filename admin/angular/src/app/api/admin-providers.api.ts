/** Admin AI model provider status API (env-backed; no secret upsert). */
import { Injectable, inject } from '@angular/core';
import { firstValueFrom } from 'rxjs';

import { ApiClient } from './api-client';

export type AiCredentialSource = 'env' | 'default' | 'none';

export interface AiProviderStatusDto {
  id: string;
  display_name: string;
  available: boolean;
  source: AiCredentialSource;
  hint?: string | null;
  base_url?: string | null;
  is_default: boolean;
}

export interface AiProvidersStatusDto {
  providers: AiProviderStatusDto[];
  default_provider_id?: string | null;
}

export interface AiProviderCatalogItemDto {
  id: string;
  display_name: string;
  group: string;
  kind: string;
  default_base_url: string;
  env_api_key: string;
  env_base_url: string;
}

export interface AiProvidersCatalogDto {
  providers: AiProviderCatalogItemDto[];
}

export interface AiProviderTestResultDto {
  ok: boolean;
  provider_id: string;
  error?: string | null;
}

@Injectable({ providedIn: 'root' })
export class AdminProvidersApi {
  private readonly api = inject(ApiClient);

  listStatus(token: string): Promise<AiProvidersStatusDto> {
    return firstValueFrom(
      this.api.http.get<AiProvidersStatusDto>(this.api.adminUrl('ai/providers'), {
        headers: { Authorization: `Bearer ${token}` },
      }),
    );
  }

  listCatalog(token: string): Promise<AiProvidersCatalogDto> {
    return firstValueFrom(
      this.api.http.get<AiProvidersCatalogDto>(this.api.adminUrl('ai/providers/catalog'), {
        headers: { Authorization: `Bearer ${token}` },
      }),
    );
  }

  testProvider(token: string, providerId: string): Promise<AiProviderTestResultDto> {
    return firstValueFrom(
      this.api.http.post<AiProviderTestResultDto>(
        this.api.adminUrl('ai/providers/test'),
        { provider_id: providerId },
        { headers: { Authorization: `Bearer ${token}` } },
      ),
    );
  }
}
