/** Public shop AI tool catalog API. */
import { Injectable, inject } from '@angular/core';
import { firstValueFrom } from 'rxjs';

import { ApiClient } from './api-client';

export interface AiToolDto {
  name: string;
  summary: string;
  openapi_path: string;
  method: string;
  scope: string;
  effect: string;
  human_approve_for_autonomous: boolean;
}

@Injectable({ providedIn: 'root' })
export class ShopAiApi {
  private readonly client = inject(ApiClient);

  listTools(): Promise<AiToolDto[]> {
    return firstValueFrom(
      this.client.http.get<AiToolDto[]>(this.client.url('/v1/ai/tools')),
    );
  }
}
