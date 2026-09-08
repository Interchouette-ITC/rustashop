/** Admin AI tool catalog API. */
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
export class AdminAiApi {
  private readonly api = inject(ApiClient);

  listTools(token: string): Promise<AiToolDto[]> {
    return firstValueFrom(
      this.api.http.get<AiToolDto[]>(this.api.adminUrl('ai/tools'), {
        headers: { Authorization: `Bearer ${token}` },
      }),
    );
  }
}
